use std::{
    env,
    error::Error,
    fs,
    io::{self, BufRead, BufReader, Write},
    net::UdpSocket,
    path::{Path, PathBuf},
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
    pub preview_image: Option<PathBuf>,
    pub preview_port: Option<u16>,
    pub preview_only: bool,
    pub preview_size: (u16, u16),
    pub preview_fps: u16,
    pub preview_quality: u8,
    pub overlay: Option<PathBuf>,
    pub overlay_opacity: u8,
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
            preview_image: None,
            preview_port: None,
            preview_only: false,
            preview_size: (640, 360),
            preview_fps: 30,
            preview_quality: 84,
            overlay: None,
            overlay_opacity: 100,
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
        if !valid_size(self.preview_size)
            || self.preview_size.0 > 1920
            || self.preview_size.1 > 1080
            || !(1..=60).contains(&self.preview_fps)
            || !(1..=100).contains(&self.preview_quality)
        {
            return Err(
                "Preview requires even dimensions up to 1920x1080, 1-60 fps, and quality 1-100"
                    .into(),
            );
        }
        if self.overlay_opacity > 100 {
            return Err("Use --overlay-opacity between 0 and 100".into());
        }
        let overlay = self.overlay.as_deref().map(validate_overlay).transpose()?;
        let preview = self
            .preview_image
            .as_ref()
            .map(std::path::absolute)
            .transpose()?;
        if self.preview_port == Some(0) || (preview.is_some() && self.preview_port.is_some()) {
            return Err("Choose either --preview-image or a nonzero --preview-port".into());
        }
        let has_preview = preview.is_some() || self.preview_port.is_some();
        if preview.as_ref().is_some_and(|path| {
            path.exists() || !path.parent().is_some_and(|parent| parent.is_dir())
        }) {
            return Err("Preview image must be a new file in an existing directory".into());
        }
        if self.preview_only {
            if !has_preview {
                return Err("Preview requires --preview-image or --preview-port".into());
            }
            if self.stream || self.highlight_seconds.is_some() {
                return Err("Preview does not accept streaming or highlights".into());
            }
            let mut command = ffmpeg();
            command.args(["-hide_banner", "-loglevel", "warning", "-nostats", "-n"]);
            self.add_input(&mut command, false)?;
            if let Some(overlay) = overlay.as_ref() {
                self.add_overlay(&mut command, overlay, false, true);
            }
            self.add_preview(&mut command, preview.as_deref(), overlay.is_some());
            if !command.status()?.success() {
                return Err("FFmpeg could not start the preview".into());
            }
            return Ok(());
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
        let audio_map = self.add_input(&mut command, true)?;
        if let Some(overlay) = overlay.as_ref() {
            self.add_overlay(&mut command, overlay, true, has_preview);
        }
        command.args([
            "-map",
            if overlay.is_some() {
                "[program]"
            } else {
                "0:v:0"
            },
            "-map",
            audio_map,
        ]);
        if self.audio_peaks {
            command.args(["-af", AudioPeaks::FILTER]);
        }
        if let Some(seconds) = self.seconds {
            command.args(["-t", &seconds.to_string()]);
        }
        if overlay.is_none() && self.needs_scaling() {
            command.args(["-vf", &self.program_filter()]);
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
        if has_preview {
            self.add_preview(&mut command, preview.as_deref(), overlay.is_some());
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

    fn program_filter(&self) -> String {
        let (width, height) = self.size;
        let method = self.quality.scale_filter();
        format!(
            "scale={width}:{height}:flags={method}:force_original_aspect_ratio=decrease:force_divisible_by=2,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        )
    }

    fn add_overlay(&self, command: &mut Command, path: &Path, recording: bool, preview: bool) {
        let (width, height) = self.size;
        let index = if recording && matches!(self.input, Input::TestPattern) {
            2
        } else {
            1
        };
        let opacity = f64::from(self.overlay_opacity) / 100.0;
        let input = if self.needs_scaling() {
            self.program_filter()
        } else {
            "null".into()
        };
        let outputs = match (recording, preview) {
            (true, true) => format!(
                ",split[program][monitor];[monitor]{}[preview]",
                self.preview_filter()
            ),
            (true, false) => "[program]".into(),
            (false, _) => format!(",{}[preview]", self.preview_filter()),
        };
        let graph = format!(
            "[0:v:0]{input}[base];[{index}:v:0]scale={width}:{height}:flags=bilinear,format=rgba,colorchannelmixer=aa={opacity}[art];[base][art]overlay=alpha=straight:eof_action=repeat{outputs}"
        );
        command
            .args(["-f", "image2", "-pattern_type", "none", "-c:v", "png", "-i"])
            .arg(path);
        command.args(["-filter_complex", &graph]);
    }

    fn preview_filter(&self) -> String {
        let (width, height) = self.size;
        let scale = (f64::from(self.preview_size.0) / f64::from(width))
            .min(f64::from(self.preview_size.1) / f64::from(height))
            .min(1.0);
        let (preview_width, preview_height) = if self.preview_port.is_some() {
            self.preview_size
        } else {
            (
                ((f64::from(width) * scale) as u16 & !1).max(2),
                ((f64::from(height) * scale) as u16 & !1).max(2),
            )
        };
        let fps = self.fps.min(self.preview_fps);
        let scaler = if self.preview_quality >= 90 {
            "lanczos"
        } else {
            "fast_bilinear"
        };
        format!(
            "fps={fps},scale={preview_width}:{preview_height}:flags={scaler}:force_original_aspect_ratio=decrease:force_divisible_by=2:reset_sar=1,pad={preview_width}:{preview_height}:(ow-iw)/2:(oh-ih)/2"
        )
    }

    fn add_preview(&self, command: &mut Command, path: Option<&Path>, composited: bool) {
        if let Some(seconds) = self.seconds {
            command.args(["-t", &seconds.to_string()]);
        }
        command.args(["-map", if composited { "[preview]" } else { "0:v:0" }]);
        if !composited {
            command.args(["-vf", &self.preview_filter()]);
        }
        if let Some(port) = self.preview_port {
            // Queue complete frames separately so a slow or closed viewer cannot hold up recording.
            let output = format!(
                "[onfail=ignore:f=fifo:fifo_format=rawvideo:queue_size=2:drop_pkts_on_overflow=1]tcp://127.0.0.1:{port}?timeout=300000|[f=null]-"
            );
            command.args([
                "-an",
                "-c:v",
                "rawvideo",
                "-pix_fmt",
                "bgra",
                "-threads:v",
                "1",
                "-f",
                "tee",
                &output,
            ]);
            return;
        }
        let quantizer = 2 + (100 - self.preview_quality) / 4;
        command
            .args([
                "-an",
                "-c:v",
                "mjpeg",
                "-threads:v",
                "1",
                "-q:v",
                &quantizer.to_string(),
                "-f",
                "image2",
                "-update",
                "1",
                "-atomic_writing",
                "1",
            ])
            .arg(path.expect("JPEG preview path"));
    }

    fn add_input(&self, command: &mut Command, with_audio: bool) -> Result<&'static str> {
        match &self.input {
            Input::TestPattern => {
                let (width, height) = self.size;
                let video = format!("testsrc2=size={width}x{height}:rate={}", self.fps);
                command.args(["-re", "-f", "lavfi", "-i", &video]);
                if with_audio {
                    command.args([
                        "-re",
                        "-f",
                        "lavfi",
                        "-i",
                        "sine=frequency=440:sample_rate=48000",
                    ]);
                }
                return Ok("1:a:0");
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
                if with_audio && let Some(audio) = audio {
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
                command.args(["-i", &devices]);
            }
            Input::File(path) => {
                command.args(["-re", "-i"]).arg(path.canonicalize()?);
            }
        }
        Ok("0:a:0?")
    }
}

fn validate_overlay(path: &Path) -> Result<PathBuf> {
    use io::Read;
    let path = path.canonicalize()?;
    let mut header = [0u8; 24];
    let mut file = fs::File::open(&path)?;
    if file.metadata()?.len() > 32 * 1024 * 1024
        || file.read_exact(&mut header).is_err()
        || &header[..8] != b"\x89PNG\r\n\x1a\n"
        || &header[12..16] != b"IHDR"
        || header[16..24].chunks_exact(4).any(|bytes| {
            !(1..=4096).contains(&u32::from_be_bytes(
                bytes.try_into().expect("4-byte PNG dimension"),
            ))
        })
    {
        return Err("Overlay must be a PNG up to 4096x4096 and 32 MiB".into());
    }
    Ok(path)
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
