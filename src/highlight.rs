use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    thread,
    time::{Duration, Instant},
};

use crate::session::ffmpeg;

#[derive(Clone, Copy)]
pub enum Request {
    Manual,
    AudioPeak,
}

#[derive(Default)]
pub struct AudioPeaks {
    baseline: f64,
    samples: u64,
    above_baseline: u8,
    next_peak: u64,
}

impl AudioPeaks {
    pub const FILTER: &str = concat!(
        "aresample=48000,asetnsamples=n=4800:p=0,",
        "astats=metadata=1:reset=1:measure_perchannel=none:measure_overall=RMS_level,",
        r"ametadata=mode=print:key=lavfi.astats.Overall.RMS_level:file='pipe\:2':direct=1"
    );

    pub fn update(&mut self, level: f64) -> bool {
        if level.is_nan() {
            return false;
        }
        let level = level.clamp(-60.0, 0.0);
        self.samples += 1;
        if self.samples == 1 {
            self.baseline = level;
        }
        let peak = level >= -30.0 && level - self.baseline >= 10.0;
        self.baseline += (level - self.baseline) * 0.06;
        self.above_baseline = if peak {
            self.above_baseline.saturating_add(1)
        } else {
            0
        };
        // Warm up for 1.5 s, require a sustained 200 ms rise, and allow one auto-clip per 12 s.
        if self.samples < 15 || self.above_baseline < 2 || self.samples < self.next_peak {
            return false;
        }
        self.above_baseline = 0;
        self.next_peak = self.samples + 120;
        true
    }
}

pub struct Highlights {
    folder: PathBuf,
    relative_folder: String,
    stem: String,
    segments_to_keep: usize,
}

impl Highlights {
    pub fn new(output: &Path, recording: &str, seconds: u16) -> io::Result<Self> {
        if !matches!(seconds, 10 | 30 | 60) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Highlights must be 10, 30, or 60 seconds",
            ));
        }
        let stem = recording.trim_end_matches(".mkv").to_owned();
        let relative_folder = format!(".{stem}-replay");
        let folder = output.join(&relative_folder);
        fs::create_dir(&folder)?;
        Ok(Self {
            folder,
            relative_folder,
            stem,
            segments_to_keep: usize::from(seconds / 2),
        })
    }

    pub fn output(&self) -> String {
        format!(
            "|[f=segment:onfail=ignore:segment_format=matroska:segment_time=2:reset_timestamps=1]{}/part-%08d.mkv",
            self.relative_folder
        )
    }

    pub fn run(&self, requests: Receiver<Request>, done: &AtomicBool, finalized: &AtomicBool) {
        let mut sequence = 1;
        let mut pending = None;
        loop {
            let stopped = done.load(Ordering::Acquire);
            let request = if pending.is_none() {
                requests.try_recv().ok()
            } else {
                None
            };
            if stopped && request.is_none() && pending.is_none() {
                break;
            }
            let mut segments = self.segments().unwrap_or_default();
            if let Some(request) = request {
                // The highest segment is still open. Wait for its successor to include the user's moment.
                pending = Some((segments.last().cloned(), Instant::now(), request));
                match request {
                    Request::Manual => {
                        println!("Highlight pending: waiting for the current segment.")
                    }
                    Request::AudioPeak => {
                        println!("Audio highlight pending: detected an audio peak.")
                    }
                }
            }
            if !stopped || !finalized.load(Ordering::Relaxed) {
                segments.pop();
            }
            let excess = segments.len().saturating_sub(self.segments_to_keep);
            for old in segments.drain(..excess) {
                let _ = fs::remove_file(self.folder.join(old));
            }
            if let Some((target, since, request)) = pending.as_ref() {
                let ready = match target {
                    Some(target) => segments.last().is_some_and(|last| last >= target),
                    None => !segments.is_empty(),
                };
                if ready || stopped || since.elapsed() >= Duration::from_secs(8) {
                    let prefix = match request {
                        Request::Manual => "Highlight",
                        Request::AudioPeak => "Audio highlight",
                    };
                    match self.export(&segments, sequence, *request) {
                        Ok(path) => println!("{prefix} saved: {}", path.display()),
                        Err(error) => eprintln!("{prefix} unavailable: {error}"),
                    }
                    sequence += 1;
                    pending = None;
                }
            }
            if !stopped {
                thread::sleep(Duration::from_millis(150));
            }
        }
    }

    fn segments(&self) -> io::Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.folder)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name
                .strip_prefix("part-")
                .and_then(|name| name.strip_suffix(".mkv"))
                .is_some_and(|digits| {
                    digits.len() == 8 && digits.bytes().all(|byte| byte.is_ascii_digit())
                })
            {
                names.push(name);
            }
        }
        names.sort_unstable();
        Ok(names)
    }

    fn export(&self, segments: &[String], sequence: u32, request: Request) -> io::Result<PathBuf> {
        if segments.is_empty() {
            return Err(io::Error::other("No footage available yet"));
        }
        let list: String = segments
            .iter()
            .map(|name| format!("file '{name}'\n"))
            .collect();
        fs::write(self.folder.join("list.ffconcat"), list)?;
        let temporary = format!("clip-{sequence:03}.mkv");
        let status = ffmpeg()
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-n",
                "-f",
                "concat",
                "-safe",
                "1",
                "-i",
                "list.ffconcat",
                "-map",
                "0",
                "-c",
                "copy",
                &temporary,
            ])
            .current_dir(&self.folder)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()?;
        if !status.success() {
            return Err(io::Error::other("FFmpeg could not join the highlight"));
        }
        let suffix = match request {
            Request::Manual => "",
            Request::AudioPeak => "-Audio",
        };
        let path = self
            .folder
            .with_file_name(format!("{}-Highlight-{sequence:03}{suffix}.mkv", self.stem));
        fs::rename(self.folder.join(temporary), &path)?;
        Ok(path)
    }
}

impl Drop for Highlights {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.folder);
    }
}
