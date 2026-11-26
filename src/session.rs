use std::{
    env,
    error::Error,
    fs,
    io::{self, BufRead, BufReader, Write},
    net::UdpSocket,
    path::PathBuf,
    process::{self, Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    highlight::{AudioPeaks, Highlights, Request},
    stream,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub enum Input {
    TestPattern,
    DirectShow(String, Option<String>),
    File(PathBuf),
}

pub struct Session {
    pub input: Input,
    pub output_dir: PathBuf,
    pub size: (u16, u16),
    pub fps: u16,
    pub seconds: Option<u64>,
    pub stream: bool,
    pub highlight_seconds: Option<u16>,
    pub audio_peaks: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            input: Input::TestPattern,
            output_dir: "recordings".into(),
            size: (1920, 1080),
            fps: 60,
            seconds: None,
            stream: false,
            highlight_seconds: None,
            audio_peaks: false,
        }
    }
}

impl Session {
    pub fn run(self) -> Result<()> {
        let (width, height) = self.size;
        if !(2..=3840).contains(&width)
            || !(2..=2160).contains(&height)
            || width % 2 != 0
            || height % 2 != 0
        {
            return Err("Use even dimensions between 2x2 and 3840x2160".into());
        }
        if !(1..=120).contains(&self.fps) || self.seconds == Some(0) {
            return Err("Use 1–120 fps and a positive duration".into());
        }
        if self.audio_peaks && self.highlight_seconds.is_none() {
            return Err("--audio-peaks requires --highlights".into());
        }
        let url = if self.stream {
            let url = env::var("STREAM_SMASH_RTMP_URL")
                .map_err(|_| "Set STREAM_SMASH_RTMP_URL before using --stream")?;
            stream::validate_url(&url)?;
            Some(url)
        } else {
            None
        };
        let mut command = ffmpeg();
        command.args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-nostats",
            "-n",
            "-progress",
            "pipe:1",
        ]);
        self.add_input(&mut command)?;
        if self.audio_peaks {
            command.args(["-af", AudioPeaks::FILTER]);
        }
        if let Some(seconds) = self.seconds {
            command.args(["-t", &seconds.to_string()]);
        }
        let filter = format!(
            "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        );
        command.args([
            "-vf",
            &filter,
            "-r",
            &self.fps.to_string(),
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-x264-params",
            "repeat-headers=1:scenecut=0",
            "-g",
            &(self.fps * 2).to_string(),
            "-b:v",
            "6000k",
            "-maxrate",
            "6000k",
            "-bufsize",
            "12000k",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-ar",
            "48000",
            "-flags",
            "+global_header",
        ]);
        let name = format!(
            "StreamSmash-{}-{}.mkv",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            process::id()
        );
        fs::create_dir_all(&self.output_dir)?;
        let output_dir = self.output_dir.canonicalize()?;
        let path = output_dir.join(&name);
        let highlights = self
            .highlight_seconds
            .map(|seconds| Highlights::new(&output_dir, &name, seconds))
            .transpose()?;
        let relay = if let Some(url) = url {
            let address = UdpSocket::bind("127.0.0.1:0")?.local_addr()?;
            Some((address, url))
        } else {
            None
        };
        if relay.is_some() || highlights.is_some() {
            let mut output = format!("[f=matroska:onfail=abort]{name}");
            if let Some((address, _)) = relay {
                output.push_str(&format!(
                    "|[f=mpegts:onfail=ignore:bsfs/v=h264_mp4toannexb]udp://{address}?pkt_size=1316"
                ));
            }
            if let Some(ref highlights) = highlights {
                output.push_str(&highlights.output());
            }
            command.args(["-f", "tee", &output]);
        } else {
            command.arg(&name);
        }
        command
            .current_dir(&output_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("Could not start FFmpeg: {error}"))?;
        println!("Recording: {}", path.display());
        if self.stream {
            println!("RTMP output enabled. Check the receiving service for live status.");
        }
        let stderr = child.stderr.take().expect("piped diagnostics");
        let stdout = child.stdout.take().expect("piped progress");
        let mut stdin = child.stdin.take().expect("piped controls");
        let (save, requests) = std::sync::mpsc::sync_channel(4);
        let manual = save.clone();
        let can_save = highlights.is_some();
        thread::spawn(move || {
            for line in io::stdin()
                .lock()
                .lines()
                .map_while(std::result::Result::ok)
            {
                match line.trim() {
                    "q" => {
                        let _ = stdin.write_all(b"q\n");
                        break;
                    }
                    "h" if can_save => {
                        if manual.try_send(Request::Manual).is_err() {
                            eprintln!("Highlight unavailable: request queue is full.");
                        }
                    }
                    "h" => eprintln!("Start recording with --highlights to save a highlight."),
                    _ => {}
                }
            }
        });
        let retries = AtomicU64::new(0);
        let done = AtomicBool::new(false);
        let finalized = AtomicBool::new(false);
        let result = thread::scope(|scope| {
            let diagnostics = scope.spawn(|| {
                let mut peaks = AudioPeaks::default();
                for line in BufReader::new(stderr)
                    .lines()
                    .map_while(std::result::Result::ok)
                {
                    if self.audio_peaks {
                        if let Some(level) = line.strip_prefix("lavfi.astats.Overall.RMS_level=") {
                            if level.parse().is_ok_and(|level| peaks.update(level)) {
                                let _ = save.try_send(Request::AudioPeak);
                            }
                            continue;
                        }
                        if line.starts_with("frame:") && line.contains("pts_time:") {
                            continue;
                        }
                    }
                    eprintln!("{line}");
                }
            });
            scope.spawn(|| read_progress(stdout, &retries));
            if let Some((address, url)) = relay.as_ref() {
                scope.spawn(|| stream::relay(*address, url, &done, &retries));
            }
            if let Some(ref highlights) = highlights {
                let done = &done;
                let finalized = &finalized;
                scope.spawn(move || highlights.run(requests, done, finalized));
            }
            let result = child.wait();
            diagnostics.join().expect("diagnostics thread");
            finalized.store(
                result.as_ref().is_ok_and(|status| status.success()),
                Ordering::Relaxed,
            );
            done.store(true, Ordering::Release);
            result
        })?;
        if !result.success() {
            return Err(format!(
                "FFmpeg exited with {result}. Check the recording: {}",
                path.display()
            )
            .into());
        }
        if fs::metadata(&path)?.len() == 0 {
            return Err("FFmpeg produced an empty recording".into());
        }
        println!("Saved: {}", path.display());
        Ok(())
    }

    fn add_input(&self, command: &mut Command) -> Result<()> {
        match &self.input {
            Input::TestPattern => {
                let (width, height) = self.size;
                let video = format!("testsrc2=size={width}x{height}:rate={}", self.fps);
                command.args([
                    "-re",
                    "-f",
                    "lavfi",
                    "-i",
                    &video,
                    "-re",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=48000",
                    "-map",
                    "0:v:0",
                    "-map",
                    "1:a:0",
                ]);
            }
            Input::DirectShow(video, audio) => {
                if !cfg!(windows) {
                    return Err("DirectShow is available on Windows".into());
                }
                if video.is_empty()
                    || video.contains(':')
                    || audio
                        .as_ref()
                        .is_some_and(|name| name.is_empty() || name.contains(':'))
                {
                    return Err(
                        "Use a valid DirectShow device name from the devices command".into(),
                    );
                }
                let mut devices = format!("video={video}");
                if let Some(audio) = audio {
                    devices.push_str(&format!(":audio={audio}"));
                }
                let (width, height) = self.size;
                command.args([
                    "-f",
                    "dshow",
                    "-rtbufsize",
                    "256M",
                    "-video_size",
                    &format!("{width}x{height}"),
                    "-framerate",
                    &self.fps.to_string(),
                    "-i",
                    &devices,
                    "-map",
                    "0:v:0",
                    "-map",
                    "0:a:0?",
                ]);
            }
            Input::File(path) => {
                command.args(["-re", "-i"]).arg(path.canonicalize()?);
                command.args(["-map", "0:v:0", "-map", "0:a:0?"]);
            }
        }
        Ok(())
    }
}

pub fn list_devices(names_only: bool) -> Result<()> {
    if !cfg!(windows) {
        return Err("Device listing is available on Windows".into());
    }
    // The DirectShow device probe reports its list on stderr and normally exits nonzero.
    let output = ffmpeg()
        .args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            "dshow",
            "-i",
            "dummy",
        ])
        .output()?;
    let listing = String::from_utf8_lossy(&output.stderr);
    let names = capture_names(&listing);
    if names.is_empty() {
        return Err(format!("No DirectShow devices found.\n{listing}").into());
    }
    if names_only {
        for (kind, name) in names {
            println!("{kind}\t{name}");
        }
    } else {
        eprint!("{listing}");
    }
    Ok(())
}

fn capture_names(listing: &str) -> Vec<(&'static str, &str)> {
    let mut names = Vec::new();
    for line in listing.lines() {
        let Some((_, message)) = line.split_once("] ") else {
            continue;
        };
        let Some(quoted) = message.strip_prefix('"') else {
            continue;
        };
        let Some((name, kind)) = quoted.rsplit_once("\" (") else {
            continue;
        };
        let kind = match kind {
            "video)" => "video",
            "audio)" => "audio",
            _ => continue,
        };
        if !name.contains(['\t', '\r', '\n']) && !names.contains(&(kind, name)) {
            names.push((kind, name));
        }
    }
    names
}

pub fn ffmpeg() -> Command {
    let mut command =
        Command::new(env::var_os("STREAM_SMASH_FFMPEG").unwrap_or_else(|| "ffmpeg".into()));
    command
        .env_remove("STREAM_SMASH_RTMP_URL")
        .env_remove("FFREPORT");
    command
}

fn read_progress(stdout: impl io::Read, retries: &AtomicU64) {
    let (mut frame, mut dropped, mut duplicated, mut elapsed) =
        ("0".into(), "0".into(), "0".into(), "00:00:00".into());
    for line in BufReader::new(stdout)
        .lines()
        .map_while(std::result::Result::ok)
    {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "frame" => frame = value.to_owned(),
                "drop_frames" => dropped = value.to_owned(),
                "dup_frames" => duplicated = value.to_owned(),
                "out_time" => elapsed = value.to_owned(),
                "progress" => {
                    println!(
                        "{elapsed} | frames {frame} | encoder dropped {dropped} | duplicated {duplicated} | RTMP retries {}",
                        retries.load(Ordering::Relaxed)
                    );
                }
                _ => {}
            }
        }
    }
}
