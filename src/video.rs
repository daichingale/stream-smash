use std::{
    io,
    process::{Command, Stdio},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use crate::session::ffmpeg;

#[derive(Clone, Copy)]
pub enum Quality {
    Fast,
    Balanced,
    Detail,
}

impl Quality {
    fn pick<T>(self, fast: T, balanced: T, detail: T) -> T {
        match self {
            Self::Fast => fast,
            Self::Balanced => balanced,
            Self::Detail => detail,
        }
    }

    pub fn scale_filter(self) -> &'static str {
        self.pick("fast_bilinear", "bicubic", "lanczos")
    }
}

impl FromStr for Quality {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "fast" => Ok(Self::Fast),
            "balanced" => Ok(Self::Balanced),
            "detail" => Ok(Self::Detail),
            _ => Err("Use --quality fast, balanced, or detail"),
        }
    }
}

#[derive(Clone, Copy)]
pub enum Encoder {
    Auto,
    Cpu,
    Amd,
    Nvidia,
    Intel,
}

impl FromStr for Encoder {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "cpu" => Ok(Self::Cpu),
            "amd" => Ok(Self::Amd),
            "nvidia" => Ok(Self::Nvidia),
            "intel" => Ok(Self::Intel),
            _ => Err("Use --encoder auto, cpu, amd, nvidia, or intel"),
        }
    }
}

impl Encoder {
    const HARDWARE: [Self; 3] = [Self::Amd, Self::Nvidia, Self::Intel];

    pub fn resolve(
        self,
        size: (u16, u16),
        fps: u16,
        kbps: u32,
        quality: Quality,
    ) -> io::Result<Self> {
        if !matches!(self, Self::Auto) {
            return Ok(self);
        }
        for encoder in Self::HARDWARE {
            if encoder.probe(size, fps, kbps, quality)? {
                return Ok(encoder);
            }
        }
        Ok(Self::Cpu)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto | Self::Cpu => "libx264",
            Self::Amd => "h264_amf",
            Self::Nvidia => "h264_nvenc",
            Self::Intel => "h264_qsv",
        }
    }

    pub fn configure(self, command: &mut Command, fps: u16, kbps: u32, quality: Quality) {
        let bitrate = format!("{kbps}k");
        let buffer = format!("{}k", kbps * 2);
        let pixel_format = if matches!(self, Self::Cpu | Self::Auto) {
            "yuv420p"
        } else {
            "nv12"
        };
        command.args([
            "-c:v",
            self.name(),
            "-pix_fmt",
            pixel_format,
            "-profile:v",
            "high",
            "-g",
            &(fps * 2).to_string(),
            "-b:v",
            &bitrate,
            "-minrate",
            &bitrate,
            "-maxrate",
            &bitrate,
            "-bufsize",
            &buffer,
            "-flags",
            "+global_header",
        ]);
        match self {
            Self::Cpu | Self::Auto => {
                let preset = quality.pick("superfast", "veryfast", "fast");
                command.args([
                    "-preset",
                    preset,
                    "-x264-params",
                    "repeat-headers=1:scenecut=0",
                ]);
                if matches!(quality, Quality::Fast) {
                    command.args(["-tune", "zerolatency"]);
                }
            }
            Self::Amd => {
                let preset = quality.pick("speed", "balanced", "quality");
                command.args(["-quality", preset, "-rc", "cbr", "-forced_idr", "1"]);
            }
            Self::Nvidia => {
                let preset = quality.pick("p2", "p4", "p5");
                command.args(["-preset", preset, "-rc", "cbr", "-forced-idr", "1"]);
            }
            Self::Intel => {
                let preset = quality.pick("veryfast", "faster", "medium");
                command.args([
                    "-preset",
                    preset,
                    "-look_ahead",
                    "0",
                    "-idr_interval",
                    "0",
                    "-forced_idr",
                    "1",
                ]);
            }
        }
    }

    fn probe(
        self,
        (width, height): (u16, u16),
        fps: u16,
        kbps: u32,
        quality: Quality,
    ) -> io::Result<bool> {
        let mut command = ffmpeg();
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size={width}x{height}:rate={fps}"),
            "-frames:v",
            "2",
        ]);
        self.configure(&mut command, fps, kbps, quality);
        let mut child = command
            .args(["-f", "null", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status.success()),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(30)),
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return result.map(|_| false);
                }
            }
        }
    }
}
