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
    video::{Encoder, Quality},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub enum Input {
    TestPattern,
    DirectShow(String, Option<String>),
    File(PathBuf),
}

#[derive(Clone, Copy)]
pub enum CaptureSize {
    MatchOutput,
    Auto,
    Fixed((u16, u16)),
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
    pub encoder: Encoder,
    pub quality: Quality,
    pub bitrate: u32,
    pub capture_size: CaptureSize,
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
            encoder: Encoder::Cpu,
            quality: Quality::Balanced,
            bitrate: 6000,
            capture_size: CaptureSize::MatchOutput,
        }
    }
}

impl Session {
    pub fn run(self) -> Result<()> {
        let (width, height) = self.size;
        if !valid_size(self.size) {
            return Err("Use even dimensions between 2x2 and 3840x2160".into());
        }
        if let CaptureSize::Fixed(size) = self.capture_size
            && !valid_size(size)
        {
            return Err("Capture size must use even dimensions up to 3840x2160".into());
        }
        if !matches!(self.capture_size, CaptureSize::MatchOutput)
            && !matches!(self.input, Input::DirectShow(..))
        {
            return Err("--capture-size requires --video-device".into());
        }
        if !(500..=100_000).contains(&self.bitrate) {
            return Err("Use --bitrate between 500 and 100000 kbps".into());
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
        if self.needs_scaling() {
            let method = self.quality.scale_filter();
            let filter = format!(
                "scale={width}:{height}:flags={method}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
            );
            command.args(["-vf", &filter]);
        }
        let encoder = self
            .encoder
            .resolve(self.size, self.fps, self.bitrate, self.quality)
            .map_err(|error| format!("Could not probe FFmpeg encoders: {error}"))?;
        encoder.configure(&mut command, self.fps, self.bitrate, self.quality);
        command.args([
            "-r",
            &self.fps.to_string(),
            "-force_key_frames",
            "expr:gte(t,n_forced*2)",
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-ar",
            "48000",
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
        println!(
            "Encoder: {} | {width}x{height} {} fps | {} kbps target",
            encoder.name(),
            self.fps,
            self.bitrate
        );
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
                scope.spawn(|| stream::relay(*address, url, self.bitrate, &done, &retries));
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

    fn directshow_size(&self) -> Option<(u16, u16)> {
        match self.capture_size {
            CaptureSize::MatchOutput => Some(self.size),
            CaptureSize::Auto => None,
            CaptureSize::Fixed(size) => Some(size),
        }
    }

    fn needs_scaling(&self) -> bool {
        match self.input {
            Input::TestPattern => false,
            Input::DirectShow(..) => self.directshow_size() != Some(self.size),
            Input::File(_) => true,
        }
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
                command.args(["-f", "dshow", "-rtbufsize", "256M"]);
                if let Some((width, height)) = self.directshow_size() {
                    command.args([
                        "-video_size",
                        &format!("{width}x{height}"),
                        "-framerate",
                        &self.fps.to_string(),
                    ]);
                }
                command.args(["-i", &devices, "-map", "0:v:0", "-map", "0:a:0?"]);
            }
            Input::File(path) => {
                command.args(["-re", "-i"]).arg(path.canonicalize()?);
                command.args(["-map", "0:v:0", "-map", "0:a:0?"]);
            }
        }
        Ok(())
    }
}

fn valid_size((width, height): (u16, u16)) -> bool {
    (2..=3840).contains(&width) && (2..=2160).contains(&height) && width % 2 == 0 && height % 2 == 0
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
    let (mut fps, mut speed) = ("0.0".into(), "N/A".into());
    for line in BufReader::new(stdout)
        .lines()
        .map_while(std::result::Result::ok)
    {
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            match key {
                "frame" => frame = value.to_owned(),
                "drop_frames" => dropped = value.to_owned(),
                "dup_frames" => duplicated = value.to_owned(),
                "out_time" => elapsed = value.to_owned(),
                "fps" => fps = value.to_owned(),
                "speed" => speed = value.to_owned(),
                "progress" => {
                    println!(
                        "{elapsed} | frames {frame} | encoder dropped {dropped} | duplicated {duplicated} | RTMP retries {} | fps {fps} | speed {speed}",
                        retries.load(Ordering::Relaxed)
                    );
                }
                _ => {}
            }
        }
    }
}
