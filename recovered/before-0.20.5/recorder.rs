//! Real capture/recording backend. One FFmpeg input is shared by preview and recording.
//! All process/device work runs on a controller thread, never the UI thread.
#[cfg(test)]
#[path = "performance_fixture.rs"]
mod performance_fixture;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Default)]
pub enum PreviewQuality {
    #[default]
    FullHd,
    Hd,
    Economy,
}
impl PreviewQuality {
    pub fn format(self) -> (u32, u32, u32) {
        match self {
            Self::FullHd => (1920, 1080, 30),
            Self::Hd => (1280, 720, 30),
            Self::Economy => (640, 360, 15),
        }
    }
}
#[derive(Clone)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
pub type LatestFrame = Arc<Mutex<Option<PreviewFrame>>>;
type Log = Arc<Mutex<VecDeque<String>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub id: String,
}
#[derive(Default, Debug)]
pub struct Devices {
    pub video: Vec<Device>,
    pub audio: Vec<Device>,
    pub microphones: Vec<Device>,
}
#[derive(Clone, Debug)]
pub enum Source {
    /// Deterministic real-time fixture, never captures the desktop.
    Synthetic,
    /// Native output-sized/fps fixture for performance tests; no physical inputs.
    SyntheticNative,
    Desktop {
        monitor: u32,
    },
    DesktopCompatible,
    Device(String),
}
#[derive(Clone, Debug)]
pub enum Audio {
    None,
    System,
    Tone,
    Device(String),
}
#[derive(Clone, Copy, Debug)]
pub enum Encoder {
    Software,
    Amd,
    Nvidia,
    Intel,
}
/// Opus uses Matroska so codec pre-skip and timestamps are preserved by the container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecordingFormat {
    #[default]
    OpusMkv,
    AacMp4,
}
impl RecordingFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::OpusMkv => "mkv",
            Self::AacMp4 => "mp4",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Config {
    pub virtual_camera: bool,
    /// Flip only virtual camera output; recording and other outputs stay unchanged.
    pub virtual_camera_mirror: bool,
    /// None keeps the extra encoder and disk ring disabled.
    pub replay_seconds: Option<u32>,
    pub scene: crate::scene::Scene,
    pub recording_format: RecordingFormat,
    pub tuning: crate::tuning::OutputTuning,
    /// Positive delays audio, negative advances it. Zero keeps captured timing.
    pub audio_offset_ms: i32,
    pub preview: PreviewQuality,
    pub source: Source,
    pub audio: Audio,
    pub microphone: Option<String>,
    pub encoder: Encoder,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub folder: PathBuf,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        self.scene.validate()?;
        if self
            .replay_seconds
            .is_some_and(|s| !(10..=600).contains(&s))
        {
            return Err("Replay must be 10..600 seconds".into());
        }
        if self
            .scene
            .layers
            .iter()
            .any(|l| l.enabled && matches!(l.source, crate::scene::LayerSource::Waveform))
            && matches!(self.audio, Audio::None)
            && self.microphone.is_none()
        {
            return Err("波形表示には音声入力を選んでください".into());
        }
        if !(-3000..=3000).contains(&self.audio_offset_ms) {
            return Err("音声同期補正は-3000〜3000msで指定してください".into());
        }
        if let Some(id) = &self.microphone {
            if id == "synthetic-tone" {
                if !matches!(self.source, Source::Synthetic | Source::SyntheticNative) {
                    return Err("合成マイクはテスト映像でのみ使えます".into());
                }
            } else if id != "default" {
                let (index, name): (usize, String) =
                    serde_json::from_str(id).map_err(|_| "マイクを選び直してください")?;
                if index > 1024 || name.is_empty() || name.len() > 1024 {
                    return Err("マイクを選び直してください".into());
                }
            }
        }
        self.tuning.validate()?;
        crate::tuning::validate_video(self.width, self.height, self.fps)?;
        if let Source::Device(id) = &self.source {
            if id.is_empty() {
                return Err("映像デバイスを選んでください".into());
            }
        }
        if self.folder.as_os_str().is_empty() {
            return Err("保存先を指定してください".into());
        }
        Ok(())
    }
}

pub enum Control {
    SetReplayOverlay(bool),
    PlayHighlights(Vec<PathBuf>),
    PlayHighlightsAfter {
        paths: Vec<PathBuf>,
        lead_ms: u32,
    },
    CancelHighlights,
    Preview(Config),
    Record(Config),
    Stream {
        config: Config,
        target: crate::streaming::StreamTarget,
        record: bool,
    },
    SaveReplay,
    SaveMatchReplay {
        seconds: Option<u32>,
        match_id: String,
        reply: mpsc::Sender<Result<PathBuf, String>>,
    },
    SetReplayFolder(PathBuf),
    SaveReplayRemote(mpsc::Sender<Result<PathBuf, String>>),
    SaveHighlight {
        seconds: u32,
        reply: mpsc::Sender<Result<PathBuf, String>>,
    },
    ReplayStatus(mpsc::Sender<crate::replaylab::Status>),
    Timer(crate::timer::TimerAction),
    Stop,
    Refresh,
    Quit,
}
pub enum Event {
    HighlightsFinished,
    HighlightsFailed(String),
    HighlightsStarted,
    Devices(Devices),
    Starting,
    Started {
        recording: bool,
        streaming: bool,
        path: Option<PathBuf>,
    },
    Stopped {
        saved: Option<PathBuf>,
    },
    Error(String),
    Warning(String),
    ReplaySaved(PathBuf),
    ReplayFolderChanged(PathBuf),
}

pub fn ffmpeg_path() -> PathBuf {
    if let Some(path) = std::env::var_os("STREAMSMASH_FFMPEG") {
        return path.into();
    }
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for path in [parent.join("tools").join(name), parent.join(name)] {
                if path.is_file() {
                    return path;
                }
            }
        }
    }
    PathBuf::from(name)
}

pub(crate) fn command() -> Command {
    let cmd = Command::new(ffmpeg_path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = cmd;
        cmd.creation_flags(0x08000000);
        cmd
    }
    #[cfg(not(windows))]
    cmd
}

pub fn list_devices() -> Result<Devices, String> {
    if !cfg!(windows) {
        return Err("実キャプチャは現在Windows版のみ対応しています".into());
    }
    let output = command()
        .args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            "dshow",
            "-i",
            "dummy",
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("FFmpegを起動できません。tools/ffmpeg.exeを確認してください: {e}"))?;
    let result = parse_devices(&String::from_utf8_lossy(&output.stderr));
    #[cfg(windows)]
    let mut result = result;
    #[cfg(windows)]
    {
        result.microphones = system_audio::input_devices().unwrap_or_default();
    }
    Ok(result)
}

fn parse_devices(log: &str) -> Devices {
    let mut result = Devices::default();
    let mut previous_video = None;
    for line in log.lines() {
        let video = line.ends_with(" (video)");
        let audio = line.ends_with(" (audio)");
        if video || audio {
            if let (Some(start), Some(end)) = (line.find('"'), line.rfind('"')) {
                if end > start {
                    let name = line[start + 1..end].to_string();
                    let device = Device {
                        id: name.clone(),
                        name,
                    };
                    if video {
                        result.video.push(device);
                    } else {
                        result.audio.push(device);
                    }
                    previous_video = Some(video);
                }
            }
        } else if line.contains("Alternative name") {
            if let (Some(video), Some(start), Some(end)) =
                (previous_video.take(), line.find('"'), line.rfind('"'))
            {
                if end > start {
                    let list = if video {
                        &mut result.video
                    } else {
                        &mut result.audio
                    };
                    if let Some(device) = list.last_mut() {
                        device.id = line[start + 1..end].to_string();
                    }
                }
            }
        }
    }
    result
}

pub struct Controller {
    pub commands: mpsc::Sender<Control>,
    pub events: mpsc::Receiver<Event>,
    pub latest: LatestFrame,
    pub recognition_frames: crate::replaylab::Frames,
    pub camera_state: Arc<AtomicU8>,
    pub timer: crate::timer::RaceTimer,
    worker: Option<JoinHandle<()>>,
}
impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}
impl Controller {
    pub fn new() -> Self {
        let (commands, rx) = mpsc::channel();
        let (tx, events) = mpsc::channel();
        let latest = Arc::new(Mutex::new(None));
        let shared = latest.clone();
        let recognition_frames = crate::replaylab::Frames::default();
        let recognition = recognition_frames.clone();
        let timer = crate::timer::RaceTimer::default();
        let clock = timer.clone();
        let camera_state = Arc::new(AtomicU8::new(0));
        let camera_status = camera_state.clone();
        let worker = thread::spawn(move || {
            controller_loop(rx, tx, shared, clock, camera_status, recognition)
        });
        Self {
            commands,
            timer,
            camera_state,
            events,
            latest,
            recognition_frames,
            worker: Some(worker),
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.commands.send(Control::Quit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn controller_loop(
    rx: mpsc::Receiver<Control>,
    tx: mpsc::Sender<Event>,
    latest: LatestFrame,
    clock: crate::timer::RaceTimer,
    camera_state: Arc<AtomicU8>,
    recognition_frames: crate::replaylab::Frames,
) {
    let mut exports: Vec<JoinHandle<()>> = Vec::new();
    let mut session: Option<Session> = None;
    let mut replay_folder: Option<PathBuf> = None;
    let mut replay_overlay = true;
    loop {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Control::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(mut current) = session.take() {
                    let _ = current.stop();
                }
                break;
            }
            Ok(Control::SetReplayOverlay(enabled)) => {
                replay_overlay = enabled;
                if let Some(p) = session.as_ref().and_then(|s| s.program.as_ref()) {
                    p.set_overlay(enabled);
                }
            }
            Ok(command @ (Control::PlayHighlights(_) | Control::PlayHighlightsAfter { .. })) => {
                let (paths, lead_ms) = match command {
                    Control::PlayHighlights(paths) => (paths, 1000),
                    Control::PlayHighlightsAfter { paths, lead_ms } => (paths, lead_ms),
                    _ => unreachable!(),
                };
                let result = session
                    .as_ref()
                    .and_then(|s| s.program.as_ref())
                    .ok_or("リプレイを有効にして入力を開始してください".to_string())
                    .and_then(|p| {
                        p.set_overlay(replay_overlay);
                        p.play(paths, lead_ms)
                    });
                match result {
                    Ok(()) => {
                        if let Some(s) = session.as_mut() {
                            s.program_playing = true;
                        }
                        let _ = tx.send(Event::HighlightsStarted);
                    }
                    Err(e) => {
                        let _ = tx.send(Event::HighlightsFailed(e));
                    }
                }
            }
            Ok(Control::CancelHighlights) => {
                if let Some(s) = session.as_ref() {
                    if let Some(p) = &s.program {
                        p.cancel();
                    }
                }
            }
            Ok(Control::Timer(action)) => clock.apply(action),
            Ok(Control::SetReplayFolder(folder)) => {
                match std::fs::create_dir_all(&folder).and_then(|_| std::path::absolute(&folder)) {
                    Ok(folder) => {
                        replay_folder = Some(folder.clone());
                        let _ = tx.send(Event::ReplayFolderChanged(folder));
                    }
                    Err(e) => {
                        let _ = tx.send(Event::Warning(format!(
                            "リプレイ保存先を設定できません: {e}"
                        )));
                    }
                }
            }
            Ok(Control::ReplayStatus(reply)) => {
                let _ = reply.send(crate::replaylab::Status {
                    completed_until: session
                        .as_ref()
                        .and_then(|s| s.replay.as_ref())
                        .and_then(|r| r.completed_until()),
                    replay_active: session.as_ref().is_some_and(|s| s.replay.is_some()),
                    seconds: session
                        .as_ref()
                        .and_then(|s| s.replay.as_ref().map(|r| r.seconds)),
                });
            }
            Ok(
                command @ (Control::SaveReplay
                | Control::SaveReplayRemote(_)
                | Control::SaveHighlight { .. }
                | Control::SaveMatchReplay { .. }),
            ) => {
                let (reply, seconds, match_id) = match command {
                    Control::SaveReplayRemote(reply) => (Some(reply), None, None),
                    Control::SaveHighlight { seconds, reply } => (Some(reply), Some(seconds), None),
                    Control::SaveMatchReplay {
                        seconds,
                        match_id,
                        reply,
                    } => (Some(reply), seconds, Some(match_id)),
                    _ => (None, None, None),
                };
                if exports.iter().any(|w| !w.is_finished()) {
                    if let Some(reply) = reply {
                        let _ = reply.send(Err("リプレイを保存中です".into()));
                    }
                    let _ = tx.send(Event::Warning(
                        "リプレイを保存中です。完了してから再度保存してください".into(),
                    ));
                    continue;
                }
                // Keep the ring alive for the worker, including during Stop.
                // Copying completed segments can be slow on an external disk;
                // never block frame reception or bridge status on that I/O.
                let source = session
                    .as_ref()
                    .ok_or("リプレイを開始してください".to_string())
                    .and_then(|s| {
                        s.replay
                            .as_ref()
                            .ok_or("リプレイを有効にして入力を適用してください".to_string())
                            .map(|b| {
                                (
                                    Arc::clone(b),
                                    replay_folder.as_ref().unwrap_or(&s.folder).clone(),
                                )
                            })
                    });
                match source {
                    Ok((buffer, folder)) => {
                        let sender = tx.clone();
                        exports.push(thread::spawn(move || {
                            let result = if let Some(id) = match_id.as_deref() {
                                buffer.snapshot_match(&folder, seconds, id)
                            } else {
                                buffer.snapshot_tail(&folder, seconds)
                            }
                            .and_then(|snapshot| snapshot.export());
                            if let Some(reply) = reply {
                                let _ = reply.send(result.clone());
                            }
                            let _ = sender.send(match result {
                                Ok(path) => Event::ReplaySaved(path),
                                Err(e) => Event::Warning(e),
                            });
                        }));
                    }
                    Err(e) => {
                        if let Some(reply) = reply {
                            let _ = reply.send(Err(e.clone()));
                        }
                        let _ = tx.send(Event::Warning(e));
                    }
                }
            }
            Ok(Control::Refresh) => match list_devices() {
                Ok(devices) => {
                    let _ = tx.send(Event::Devices(devices));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(e));
                }
            },
            Ok(Control::Stop) => {
                let _ = tx.send(Event::Starting);
                if let Some(mut current) = session.take() {
                    match current.stop() {
                        Ok(()) => {
                            let _ = tx.send(Event::Stopped {
                                saved: current.path.clone(),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Error(e));
                        }
                    }
                } else {
                    let _ = tx.send(Event::Stopped { saved: None });
                }
                *latest.lock().unwrap() = None;
            }
            Ok(control @ (Control::Preview(_) | Control::Record(_) | Control::Stream { .. })) => {
                let _ = tx.send(Event::Starting);
                if let Some(mut current) = session.take() {
                    if let Err(e) = current.stop() {
                        let _ = tx.send(Event::Warning(e));
                    }
                }
                *latest.lock().unwrap() = None;
                let (config, record, target) = match control {
                    Control::Preview(c) => (c, false, None),
                    Control::Record(c) => (c, true, None),
                    Control::Stream {
                        config,
                        target,
                        record,
                    } => (config, record, Some(target)),
                    _ => unreachable!(),
                };
                match Session::start(
                    &config,
                    record,
                    target,
                    latest.clone(),
                    clock.clone(),
                    camera_state.clone(),
                    recognition_frames.clone(),
                ) {
                    Ok(current) => {
                        session = Some(current);
                    }
                    Err(e) => {
                        let _ = tx.send(Event::Error(e));
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        exports.retain(|w| !w.is_finished());
        if let Some(current) = &mut session {
            if let Some(program) = &current.program {
                if current.program_playing && !program.playing() {
                    current.program_playing = false;
                    let _ = tx.send(Event::HighlightsFinished);
                }
                if let Some(e) = program.error() {
                    let _ = tx.send(Event::Warning(e));
                }
            }
            match current.child.try_wait() {
                Ok(Some(status)) => {
                    let mut message = current.diagnostics();
                    if message.contains("Could not run graph")
                        || message.contains("Unable to BindToObject")
                    {
                        message = format!("カメラ／キャプチャーボードを開けません。OBSなどで同じ入力を使用している場合は、その入力を無効化するかアプリを終了してください。USB・HDMI接続も確認してください。\n{message}");
                    }
                    let path = current.path.clone();
                    current.finish_threads();
                    let _ = tx.send(Event::Error(format!("入力が終了しました ({status})。録画がある場合はファイルを確認してください。{path:?}\n{message}")));
                    session = None;
                }
                Ok(None) => {
                    if let Some(error) = current.browsers.iter().find_map(|b| b.error()) {
                        let _ = current.stop();
                        let _ = tx.send(Event::Error(error));
                        session = None;
                        continue;
                    }
                    if let Some(error) = current.output_error.lock().unwrap().take() {
                        camera_state.store(3, Ordering::Relaxed);
                        let _=tx.send(Event::Warning(format!("仮想カメラを開始できません。プレビュー・録画・配信は継続します：{error}")));
                    }
                    let output_error = current.timer_overlay.as_ref().and_then(|t| t.error());
                    if let Some(error) = output_error {
                        let _ = current.stop();
                        let _ = tx.send(Event::Error(error));
                        session = None;
                        continue;
                    }
                    #[cfg(windows)]
                    if let Some(error) = current.audio.as_ref().and_then(|a| a.error()) {
                        let _ = current.stop();
                        let _ = tx.send(Event::Error(format!(
                            "システム音声入力が終了しました。録画を停止しました: {error}"
                        )));
                        session = None;
                        continue;
                    }
                    #[cfg(windows)]
                    if !current.microphone_warned {
                        if let Some(error) =
                            current.microphone_audio.as_ref().and_then(|m| m.error())
                        {
                            current.microphone_warned = true;
                            let _=tx.send(Event::Warning(format!("マイクが切断されたかエラーになりました。映像・録画は継続します。マイクを確認してください：{error}")));
                        }
                    }
                    if !current.announced && current.received.load(Ordering::Acquire) {
                        current.announced = true;
                        let _ = tx.send(Event::Started {
                            recording: current.path.is_some(),
                            streaming: current.target.is_some(),
                            path: current.path.clone(),
                        });
                    } else if !current.announced
                        && current.started.elapsed() > Duration::from_secs(15)
                    {
                        let message = current.diagnostics();
                        let _ = current.stop();
                        let _ = tx.send(Event::Error(format!("15秒間映像を受信できませんでした。入力・接続・使用中のアプリを確認してください。\n{message}")));
                        session = None;
                    }
                }
                Err(e) => {
                    let _ = current.stop();
                    let _ = tx.send(Event::Error(e.to_string()));
                    session = None;
                }
            }
        }
    }
    for export in exports {
        let _ = export.join();
    }
}

struct Session {
    program: Option<crate::program::Insert>,
    program_playing: bool,
    timer_overlay: Option<crate::timer::Overlay>,
    replay: Option<Arc<crate::replay::ReplayBuffer>>,
    folder: PathBuf,
    child: Child,
    browsers: Vec<crate::browser::BrowserOverlay>,
    target: Option<crate::streaming::StreamTarget>,
    reader: Option<JoinHandle<()>>,
    logger: Option<JoinHandle<()>>,
    log: Log,
    received: Arc<AtomicBool>,
    output_error: Arc<Mutex<Option<String>>>,
    started: Instant,
    announced: bool,
    path: Option<PathBuf>,
    #[cfg(windows)]
    audio: Option<system_audio::Bridge>,
    #[cfg(windows)]
    microphone_audio: Option<system_audio::Bridge>,
    microphone_warned: bool,
}

fn input_args(config: &Config, combined_audio: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        if std::env::var_os("STREAMSMASH_DEBUG").is_some() {
            "info".into()
        } else {
            "warning".into()
        },
        "-nostats".into(),
    ];
    // Do not replace DirectShow sample timestamps with demuxer arrival times:
    // audio packets describe the START of a buffered block, not its delivery time.
    // A combined graph gives both devices one clock and one start operation.
    if combined_audio.is_none()
        && !matches!(config.source, Source::Synthetic | Source::SyntheticNative)
    {
        args.extend(["-use_wallclock_as_timestamps", "1"].map(String::from));
    }
    let fps = config.fps.to_string();
    match &config.source {
        Source::Synthetic => args.extend(
            ["-re", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=30"].map(String::from),
        ),
        Source::SyntheticNative => args.extend([
            "-re".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!(
                "testsrc2=size={}x{}:rate={}",
                config.width, config.height, config.fps
            ),
        ]),
        Source::Desktop { monitor } => args.extend([
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!(
                "ddagrab=output_idx={monitor}:framerate={fps}:draw_mouse=1,hwdownload,format=bgra"
            ),
        ]),
        Source::DesktopCompatible => args.extend([
            "-f".into(),
            "gdigrab".into(),
            "-framerate".into(),
            fps,
            "-i".into(),
            "desktop".into(),
        ]),
        Source::Device(id) => {
            // Use the device default input mode; forcing 1080p/60 excludes many cameras.
            args.extend([
                "-thread_queue_size".into(),
                "8".into(),
                "-rtbufsize".into(),
                "32M".into(),
                "-f".into(),
                "dshow".into(),
                "-use_video_device_timestamps".into(),
                "false".into(),
            ]);
            if let Some(audio) = combined_audio {
                args.extend([
                    "-audio_buffer_size".into(),
                    "50".into(),
                    "-i".into(),
                    format!("video={id}:audio={audio}"),
                ]);
            } else {
                args.extend(["-i".into(), format!("video={id}")]);
            }
        }
    }
    args
}

fn audio_filter(config: &Config) -> String {
    let mut filter = format!(
        "asetpts=PTS+({})/1000/TB,aresample=48000:async=100:min_hard_comp=0.100:first_pts=0",
        config.audio_offset_ms
    );
    if config.microphone.is_none()
        && (config.tuning.gain_percent != 100 || config.tuning.primary_muted)
    {
        filter.push_str(&format!(
            ",volume={:.2}",
            if config.tuning.primary_muted {
                0.
            } else {
                config.tuning.gain_percent as f64 / 100.
            }
        ));
    }
    if config.tuning.limiter {
        filter.push_str(",alimiter=limit=0.95:level=0:latency=1");
    }
    filter
}

#[cfg(test)]
fn record_args(config: &Config, path: &Path, audio_map: Option<&str>) -> Vec<String> {
    record_args_with_video(config, path, audio_map, "0:v:0")
}
fn record_args_with_video(
    config: &Config,
    path: &Path,
    audio_map: Option<&str>,
    video_map: &str,
) -> Vec<String> {
    let mut args = vec!["-map".into(), video_map.into()];
    if let Some(audio_map) = audio_map {
        args.extend([
            "-map".into(),
            audio_map.into(),
            "-ar".into(),
            "48000".into(),
        ]);
        match config.recording_format {
            RecordingFormat::OpusMkv => args.extend(
                [
                    "-c:a",
                    "libopus",
                    "-application",
                    "lowdelay",
                    "-frame_duration",
                    "10",
                    "-compression_level",
                    "5",
                    "-b:a",
                    "160k",
                    "-vbr",
                    "constrained",
                ]
                .map(String::from),
            ),
            RecordingFormat::AacMp4 => {
                args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from))
            }
        }
    } else {
        args.push("-an".into());
    }
    if let Some(index) = args.iter().position(|s| s == "-b:a") {
        args[index + 1] =
            config
                .tuning
                .audio_rate(if config.recording_format == RecordingFormat::OpusMkv {
                    160
                } else {
                    192
                });
    }
    if audio_map.is_some_and(|a| !a.starts_with('[')) {
        args.extend(["-af".into(), audio_filter(config)]);
    }
    if video_map == "0:v:0" {
        args.extend(["-vf".into(), format!("scale={}:{}:force_original_aspect_ratio=decrease:flags=fast_bilinear,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p",config.width,config.height,config.width,config.height)]);
    }
    args.extend(["-r".into(), config.fps.to_string(), "-c:v".into()]);
    match config.encoder {
        Encoder::Software => {
            args.extend(["libx264", "-preset", "veryfast", "-crf", "21"].map(String::from))
        }
        Encoder::Amd => {
            args.extend(["h264_amf", "-quality", "speed", "-b:v", "12M"].map(String::from))
        }
        Encoder::Nvidia => {
            args.extend(["h264_nvenc", "-preset", "p4", "-b:v", "12M"].map(String::from))
        }
        Encoder::Intel => {
            args.extend(["h264_qsv", "-preset", "veryfast", "-b:v", "12M"].map(String::from))
        }
    }
    if let Some(index) = args.iter().position(|s| s == "-crf") {
        args[index + 1] = config.tuning.record_crf.to_string();
    }
    if let Some(index) = args.iter().position(|s| s == "-b:v") {
        args[index + 1] = format!("{}k", config.tuning.gpu_record_kbps);
    }
    args.extend(["-g".into(), (config.fps * 2).to_string()]);
    match config.recording_format {
        RecordingFormat::OpusMkv => {
            args.extend(["-f", "matroska", "-cluster_time_limit", "1000"].map(String::from))
        }
        RecordingFormat::AacMp4 => args.extend(
            [
                "-movflags",
                "+frag_keyframe+empty_moov+default_base_moof",
                "-f",
                "mp4",
            ]
            .map(String::from),
        ),
    }
    args.extend(["-n".into(), path.to_string_lossy().into_owned()]);
    args
}

fn stream_args(
    config: &Config,
    target: &crate::streaming::StreamTarget,
    audio_map: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "-map".into(),
        "[v_stream]".into(),
        "-r".into(),
        config.fps.to_string(),
        "-c:v".into(),
    ];
    match config.encoder {
        Encoder::Software => args
            .extend(["libx264", "-preset", "veryfast", "-tune", "zerolatency"].map(String::from)),
        Encoder::Amd => args.extend(["h264_amf", "-quality", "speed"].map(String::from)),
        Encoder::Nvidia => args.extend(["h264_nvenc", "-preset", "p4"].map(String::from)),
        Encoder::Intel => args.extend(["h264_qsv", "-preset", "veryfast"].map(String::from)),
    }
    let rate = format!("{}k", target.bitrate_kbps);
    args.extend([
        "-b:v".into(),
        rate.clone(),
        "-maxrate".into(),
        rate,
        "-bufsize".into(),
        format!("{}k", target.bitrate_kbps * 2),
        "-g".into(),
        (config.fps * 2).to_string(),
        "-bf".into(),
        "0".into(),
    ]);
    if let Some(audio) = audio_map {
        args.extend([
            "-map".into(),
            audio.into(),
            "-c:a".into(),
            "aac".into(),
            "-ar".into(),
            "48000".into(),
            "-ac".into(),
            "2".into(),
            "-b:a".into(),
            config.tuning.audio_rate(192),
        ]);
    } else {
        args.push("-an".into());
    }
    if audio_map.is_some_and(|a| !a.starts_with('[')) {
        args.extend(["-af".into(), audio_filter(config)]);
    }
    if target.is_tls() {
        args.extend(["-tls_verify", "1"].map(String::from));
    }
    args.extend([
        "-rw_timeout".into(),
        "5000000".into(),
        "-f".into(),
        "flv".into(),
        "-flvflags".into(),
        "no_duration_filesize".into(),
        target.publish_url(),
    ]);
    args
}

impl Session {
    fn start(
        config: &Config,
        record: bool,
        target: Option<crate::streaming::StreamTarget>,
        latest: LatestFrame,
        clock: crate::timer::RaceTimer,
        camera_state: Arc<AtomicU8>,
        recognition_frames: crate::replaylab::Frames,
    ) -> Result<Self, String> {
        if !cfg!(windows) {
            return Err("実キャプチャはWindows版のみ対応しています".into());
        }
        config.validate()?;
        let path = if record {
            std::fs::create_dir_all(&config.folder)
                .map_err(|e| format!("保存先を作成できません: {e}"))?;
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            Some(config.folder.join(format!(
                "StreamSmash-{stamp}.{}",
                config.recording_format.extension()
            )))
        } else {
            None
        };
        let replay = config
            .replay_seconds
            .map(crate::replay::ReplayBuffer::new)
            .transpose()?
            .map(Arc::new);
        let output_active = record
            || target.is_some()
            || replay.is_some()
            || config
                .scene
                .layers
                .iter()
                .any(|l| l.enabled && matches!(l.source, crate::scene::LayerSource::Waveform));
        let combined_audio = match (&config.source, &config.audio, output_active) {
            (Source::Device(_), Audio::Device(id), true) => Some(id.as_str()),
            _ => None,
        };
        let mut args = input_args(config, combined_audio);
        let audio_map = if !output_active || matches!(config.audio, Audio::None) {
            None
        } else if combined_audio.is_some() {
            Some("0:a:0")
        } else {
            Some("1:a:0")
        };
        #[cfg(windows)]
        let mut audio = None;
        if output_active {
            match &config.audio {
                Audio::None => {}
                Audio::Tone => args.extend(
                    [
                        "-re",
                        "-f",
                        "lavfi",
                        "-i",
                        "sine=frequency=440:sample_rate=48000",
                    ]
                    .map(String::from),
                ),
                Audio::Device(_) if combined_audio.is_some() => {}
                Audio::Device(id) => args.extend([
                    "-use_wallclock_as_timestamps".into(),
                    "1".into(),
                    "-isync".into(),
                    "0".into(),
                    "-thread_queue_size".into(),
                    "32".into(),
                    "-f".into(),
                    "dshow".into(),
                    "-audio_buffer_size".into(),
                    "50".into(),
                    "-i".into(),
                    format!("audio={id}"),
                ]),
                Audio::System => {
                    #[cfg(windows)]
                    {
                        let bridge = system_audio::Bridge::start()?;
                        args.extend([
                            "-use_wallclock_as_timestamps".into(),
                            "1".into(),
                            "-isync".into(),
                            "0".into(),
                            "-probesize".into(),
                            "32".into(),
                            "-analyzeduration".into(),
                            "0".into(),
                            "-thread_queue_size".into(),
                            "32".into(),
                            "-f".into(),
                            "f32le".into(),
                            "-ar".into(),
                            bridge.rate.to_string(),
                            "-ac".into(),
                            bridge.channels.to_string(),
                            "-i".into(),
                            format!("tcp://127.0.0.1:{}", bridge.port),
                        ]);
                        audio = Some(bridge);
                    }
                }
            }
        }
        let timer_overlay = if config
            .scene
            .layers
            .iter()
            .any(|l| l.enabled && matches!(l.source, crate::scene::LayerSource::Timer))
        {
            Some(crate::timer::Overlay::start(clock)?)
        } else {
            None
        };
        #[cfg(windows)]
        let mut microphone_audio = None;
        let mut next_input = if audio_map == Some("1:a:0") { 2 } else { 1 };
        let mic_map = if output_active {
            if let Some(id) = &config.microphone {
                let index = next_input;
                next_input += 1;
                if id == "synthetic-tone" {
                    args.extend(
                        [
                            "-re",
                            "-f",
                            "lavfi",
                            "-i",
                            "sine=frequency=997:sample_rate=48000",
                        ]
                        .map(String::from),
                    );
                } else {
                    #[cfg(windows)]
                    {
                        let bridge = system_audio::Bridge::start_microphone(id.clone())?;
                        args.extend([
                            "-use_wallclock_as_timestamps".into(),
                            "1".into(),
                            "-isync".into(),
                            "0".into(),
                            "-probesize".into(),
                            "32".into(),
                            "-analyzeduration".into(),
                            "0".into(),
                            "-thread_queue_size".into(),
                            "32".into(),
                            "-f".into(),
                            "f32le".into(),
                            "-ar".into(),
                            bridge.rate.to_string(),
                            "-ac".into(),
                            bridge.channels.to_string(),
                            "-i".into(),
                            format!("tcp://127.0.0.1:{}", bridge.port),
                        ]);
                        microphone_audio = Some(bridge);
                    }
                }
                Some(format!("{index}:a:0"))
            } else {
                None
            }
        } else {
            None
        };
        let mut browsers = Vec::new();
        let mut indexed_layers = Vec::new();
        for layer in config.scene.layers.iter().filter(|l| l.enabled) {
            match &layer.source {
                crate::scene::LayerSource::Image { path } => {
                    args.extend(["-i".into(), path.to_string_lossy().into_owned()])
                }
                crate::scene::LayerSource::Waveform | crate::scene::LayerSource::Timer => {
                    indexed_layers.push((0, layer));
                    continue;
                }
                crate::scene::LayerSource::Browser { location } => {
                    let browser =
                        crate::browser::BrowserOverlay::start(location, layer.width, layer.height)?;
                    args.extend([
                        "-thread_queue_size".into(),
                        "8".into(),
                        "-probesize".into(),
                        "32".into(),
                        "-analyzeduration".into(),
                        "0".into(),
                        "-f".into(),
                        "rawvideo".into(),
                        "-pixel_format".into(),
                        "rgba".into(),
                        "-video_size".into(),
                        format!("{}x{}", layer.width, layer.height),
                        "-framerate".into(),
                        if matches!(layer.source, crate::scene::LayerSource::Timer) {
                            "30"
                        } else {
                            "15"
                        }
                        .into(),
                        "-i".into(),
                        format!("tcp://127.0.0.1:{}", browser.port),
                    ]);
                    browsers.push(browser);
                }
            }
            indexed_layers.push((next_input, layer));
            next_input += 1;
        }
        let program = if replay.is_some() {
            Some(crate::program::Insert::start()?)
        } else {
            None
        };
        let program_inputs = program.as_ref().map(|p| {
            args.extend([
                "-thread_queue_size".into(),
                "2".into(),
                "-probesize".into(),
                "32".into(),
                "-analyzeduration".into(),
                "0".into(),
                "-f".into(),
                "rawvideo".into(),
                "-pixel_format".into(),
                "rgba".into(),
                "-video_size".into(),
                "1280x720".into(),
                "-framerate".into(),
                "30".into(),
                "-i".into(),
                format!("tcp://127.0.0.1:{}", p.video_port),
            ]);
            args.extend([
                "-thread_queue_size".into(),
                "2".into(),
                "-probesize".into(),
                "32".into(),
                "-analyzeduration".into(),
                "0".into(),
                "-f".into(),
                "f32le".into(),
                "-ar".into(),
                "48000".into(),
                "-ac".into(),
                "4".into(),
                "-i".into(),
                format!("tcp://127.0.0.1:{}", p.audio_port),
            ]);
            (next_input, next_input + 1)
        });
        let mixed = mic_map.as_deref().map(|mic| {
            crate::audio_mix::route(
                audio_map,
                mic,
                &config.tuning,
                &audio_filter(config),
                crate::audio_mix::Outputs {
                    wave: indexed_layers
                        .iter()
                        .any(|(_, l)| matches!(l.source, crate::scene::LayerSource::Waveform)),
                    record,
                    stream: target.is_some(),
                    buffer: replay.is_some(),
                    program: program_inputs.is_some(),
                },
            )
        });
        let scene_audio = if let Some(m) = &mixed {
            m.wave.as_deref()
        } else {
            audio_map
        };
        let live_audio = if let Some(m) = &mixed {
            m.live.as_deref()
        } else {
            audio_map
        };
        let buffer_audio = mixed
            .as_ref()
            .and_then(|m| m.buffer.as_deref())
            .or(audio_map);
        let mut graph = crate::scene::filter_graph(
            &indexed_layers,
            (config.width, config.height),
            config.preview.format(),
            record,
            target.is_some(),
            replay.is_some(),
            crate::scene::GraphInputs {
                audio: scene_audio,
                timer: timer_overlay
                    .as_ref()
                    .map(|t| (t.path.as_path(), t.font.as_path())),
                program: program_inputs.map(|p| p.0),
            },
        );
        if let Some(m) = &mixed {
            graph = format!("{}{}", m.graph, graph);
        }
        let mut record_audio = if let Some(m) = &mixed {
            m.record.clone()
        } else {
            audio_map.map(str::to_string)
        };
        let mut stream_audio = if let Some(m) = &mixed {
            m.stream.clone()
        } else {
            audio_map.map(str::to_string)
        };
        if let (Some((_, input)), Some(live)) = (program_inputs, live_audio) {
            if record || target.is_some() {
                graph.push_str(&format!(";[{input}:a]asplit=2[clip4][gain4];[clip4]pan=stereo|c0=c0|c1=c1[clip];[gain4]pan=stereo|c0=c2|c1=c3[gain];[{live}]{},aformat=sample_fmts=flt:sample_rates=48000:channel_layouts=stereo[liveaudio];[liveaudio][gain]amultiply[ducked];[ducked][clip]amix=inputs=2:normalize=0[programaudio]",if mixed.is_some() {"anull".into()} else {audio_filter(config)}));
                if record && target.is_some() {
                    graph.push_str(";[programaudio]asplit=2[a_record][a_stream]");
                    record_audio = Some("[a_record]".into());
                    stream_audio = Some("[a_stream]".into());
                } else if record {
                    record_audio = Some("[programaudio]".into());
                } else {
                    stream_audio = Some("[programaudio]".into());
                }
            } else {
                graph.push_str(&format!(";[{input}:a]anullsink"));
            }
        } else if let Some((_, input)) = program_inputs {
            graph.push_str(&format!(";[{input}:a]anullsink"));
        }
        args.extend([
            "-filter_complex_threads".into(),
            thread::available_parallelism()
                .map_or(1, |n| n.get().min(4))
                .to_string(),
            "-filter_complex".into(),
            graph,
        ]);
        if let Some(path) = &path {
            args.extend(record_args_with_video(
                config,
                path,
                record_audio.as_deref(),
                "[v_record]",
            ));
        }
        if let Some(target) = &target {
            args.extend(stream_args(config, target, stream_audio.as_deref()));
        }
        if let Some(buffer) = &replay {
            // Independent two-second GOPs allow stream-copy exports at segment boundaries.
            let mut a = vec![
                "-map".into(),
                "[v_buffer]".into(),
                "-r".into(),
                config.fps.to_string(),
                "-c:v".into(),
            ];
            match config.encoder {
                Encoder::Software => a.extend(
                    [
                        "libx264",
                        "-preset",
                        "veryfast",
                        "-tune",
                        "zerolatency",
                        "-sc_threshold",
                        "0",
                    ]
                    .map(String::from),
                ),
                Encoder::Amd => a.extend(["h264_amf", "-quality", "speed"].map(String::from)),
                Encoder::Nvidia => a.extend(["h264_nvenc", "-preset", "p4"].map(String::from)),
                Encoder::Intel => a.extend(["h264_qsv", "-preset", "veryfast"].map(String::from)),
            }
            a.extend([
                "-b:v".into(),
                "8000k".into(),
                "-maxrate".into(),
                "8000k".into(),
                "-bufsize".into(),
                "16000k".into(),
                "-g".into(),
                (config.fps * 2).to_string(),
                "-bf".into(),
                "0".into(),
                "-force_key_frames".into(),
                "expr:gte(t,n_forced*2)".into(),
            ]);
            if let Some(audio) = buffer_audio {
                a.extend([
                    "-map".into(),
                    audio.into(),
                    "-c:a".into(),
                    "aac".into(),
                    "-b:a".into(),
                    config.tuning.audio_rate(192),
                    "-ar".into(),
                    "48000".into(),
                ]);
                if !audio.starts_with('[') {
                    a.extend(["-af".into(), audio_filter(config)]);
                }
            } else {
                a.push("-an".into());
            }
            a.extend(buffer.muxer_args());
            args.extend(a);
        }
        let (preview_width, preview_height, _) = config.preview.format();
        args.extend(
            [
                "-map",
                "[v_preview]",
                "-fps_mode",
                "passthrough",
                "-an",
                "-pix_fmt",
                "rgba",
                "-threads",
                "1",
                "-c:v",
                "rawvideo",
                "-f",
                "rawvideo",
                "pipe:1",
            ]
            .map(String::from),
        );
        let mut child = command()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("FFmpegを起動できません: {e}"))?;
        let mut stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let received = Arc::new(AtomicBool::new(false));
        let seen = received.clone();
        let output_error = Arc::new(Mutex::new(None));
        let failure = output_error.clone();
        let virtual_camera = config.virtual_camera;
        #[cfg(windows)]
        let virtual_camera_mirror = config.virtual_camera_mirror;
        let reader = thread::spawn(move || {
            #[cfg(windows)]
            let camera = if virtual_camera {
                match crate::virtual_camera::Output::start(
                    camera_state.clone(),
                    virtual_camera_mirror,
                ) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        *failure.lock().unwrap() = Some(e);
                        None
                    }
                }
            } else {
                None
            };
            #[cfg(not(windows))]
            if virtual_camera {
                *failure.lock().unwrap() = Some("仮想カメラは現在Windows版のみ対応です".into());
            }
            loop {
                let mut buffer = vec![0; (preview_width * preview_height * 4) as usize];
                if stdout.read_exact(&mut buffer).is_err() {
                    break;
                }
                #[cfg(windows)]
                if let Some(camera) = &camera {
                    camera.send(&buffer, preview_width, preview_height);
                }
                let frame = PreviewFrame {
                    width: preview_width,
                    height: preview_height,
                    rgba: buffer,
                };
                crate::replaylab::publish_frame(&recognition_frames, &frame);
                *latest.lock().unwrap() = Some(frame);
                seen.store(true, Ordering::Release);
            }
            camera_state.store(0, Ordering::Relaxed);
            recognition_frames.clear();
        });
        let log: Log = Arc::new(Mutex::new(VecDeque::new()));
        let log_shared = log.clone();
        let scrub = target.clone();
        let logger = thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(Result::ok)
            {
                let mut log = log_shared.lock().unwrap();
                if log.len() >= 40 {
                    log.pop_front();
                }
                let line = if let Some(target) = &scrub {
                    target.redact(&line)
                } else {
                    line
                };
                log.push_back(line.chars().take(1500).collect());
            }
        });
        Ok(Self {
            program,
            program_playing: false,
            timer_overlay,
            replay,
            folder: config.folder.clone(),
            child,
            browsers,
            target,
            reader: Some(reader),
            logger: Some(logger),
            log,
            received,
            output_error,
            started: Instant::now(),
            announced: false,
            path,
            #[cfg(windows)]
            audio,
            #[cfg(windows)]
            microphone_audio,
            microphone_warned: false,
        })
    }
    fn diagnostics(&self) -> String {
        self.log
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn finish_threads(&mut self) {
        self.program.take();
        self.timer_overlay.take();
        self.browsers.clear();
        #[cfg(windows)]
        {
            self.audio.take();
            self.microphone_audio.take();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(logger) = self.logger.take() {
            let _ = logger.join();
        }
    }
    fn stop(&mut self) -> Result<(), String> {
        if let Some(mut stdin) = self.child.stdin.take() {
            let _ = stdin.write_all(b"q\n");
        }
        if let Some(p) = &self.program {
            p.request_stop();
        }
        for browser in &self.browsers {
            browser.request_stop();
        }
        #[cfg(windows)]
        if let Some(audio) = &self.audio {
            audio.request_stop();
        }
        #[cfg(windows)]
        if let Some(audio) = &self.microphone_audio {
            audio.request_stop();
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut forced = false;
        let status = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Err(_) => break None,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(30)),
                Ok(None) => {
                    forced = true;
                    break None;
                }
            }
        };
        if status.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        self.finish_threads();
        if std::env::var_os("STREAMSMASH_DEBUG").is_some() {
            eprintln!("{}", self.diagnostics());
        }
        if forced || !status.is_some_and(|s| s.success()) {
            return Err(format!(
                "入力の停止でエラーが発生しました。録画ファイルを確認してください。\n{}",
                self.diagnostics()
            ));
        }
        if let Some(path) = &self.path {
            if std::fs::metadata(path).map_or(true, |m| m.len() < 1024) {
                return Err("録画データが保存されませんでした".into());
            }
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.stop();
        } else {
            self.finish_threads();
        }
    }
}

#[cfg(windows)]
mod system_audio {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::net::TcpListener;
    pub fn input_devices() -> Result<Vec<Device>, String> {
        let devices = cpal::default_host()
            .input_devices()
            .map_err(|e| e.to_string())?;
        Ok(devices
            .enumerate()
            .filter_map(|(index, d)| {
                d.name().ok().map(|name| Device {
                    id: serde_json::to_string(&(index, &name)).unwrap(),
                    name,
                })
            })
            .collect())
    }
    pub struct Bridge {
        pub rate: u32,
        pub channels: u16,
        pub port: u16,
        error: Arc<Mutex<Option<String>>>,
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }
    impl Bridge {
        pub fn request_stop(&self) {
            self.stop.store(true, Ordering::Relaxed);
        }
        pub fn error(&self) -> Option<String> {
            self.error.lock().unwrap().clone()
        }
        pub fn start() -> Result<Self, String> {
            Self::start_device(None)
        }
        pub fn start_microphone(id: String) -> Result<Self, String> {
            Self::start_device(Some(id))
        }
        fn start_device(microphone: Option<String>) -> Result<Self, String> {
            let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            let port = listener.local_addr().map_err(|e| e.to_string())?.port();
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let error = Arc::new(Mutex::new(None));
            let failure = error.clone();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let worker = thread::spawn(move || {
                let result = (|| -> Result<(), String> {
                    let host = cpal::default_host();
                    let (device, config) = if let Some(id) = microphone {
                        let device = if id == "default" {
                            host.default_input_device().ok_or(
                                "既定のマイクがありません。Windowsの入力設定を確認してください",
                            )?
                        } else {
                            let (index, name): (usize, String) = serde_json::from_str(&id)
                                .map_err(|_| "マイクを選び直してください")?;
                            let device = host
                                .input_devices()
                                .map_err(|e| e.to_string())?
                                .nth(index)
                                .ok_or(
                                    "マイクが見つかりません。デバイス更新後に選び直してください",
                                )?;
                            if device.name().ok().as_deref() != Some(name.as_str()) {
                                return Err(
                                    "マイク一覧が変わりました。デバイス更新後に選び直してください"
                                        .into(),
                                );
                            }
                            device
                        };
                        let config = device
                            .default_input_config()
                            .map_err(|e| format!("マイクの入力形式を取得できません：{e}"))?;
                        (device, config)
                    } else {
                        let device = host
                            .default_output_device()
                            .ok_or("既定の再生デバイスがありません")?;
                        let config = device.default_output_config().map_err(|e| e.to_string())?;
                        (device, config)
                    };
                    let rate = config.sample_rate().0;
                    let channels = config.channels();
                    let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(64);
                    let callback_error = failure.clone();
                    let err = move |e: cpal::StreamError| {
                        *callback_error.lock().unwrap() =
                            Some(format!("WASAPIデバイスのエラー: {e}"));
                    };
                    let stream = match config.sample_format() {
                        cpal::SampleFormat::F32 => device.build_input_stream(&config.config(), move |data: &[f32], _| { let _ = tx.try_send(data.to_vec()); },err,None),
                        cpal::SampleFormat::I16 => device.build_input_stream(&config.config(), move |data: &[i16], _| { let _ = tx.try_send(data.iter().map(|s| *s as f32 / 32768.0).collect()); },err,None),
                        cpal::SampleFormat::I32 => device.build_input_stream(&config.config(), move |data: &[i32], _| { let _ = tx.try_send(data.iter().map(|s| *s as f32 / 2147483648.0).collect()); },err,None),
                        _ => return Err("この音声機器のサンプル形式には未対応です。別の入力を選んでください".into()),
                    }.map_err(|e| format!("音声機器を開始できません。Windowsのマイク権限や機器の使用状況を確認してください: {e}"))?;
                    stream.play().map_err(|e| e.to_string())?;
                    ready_tx
                        .send(Ok((rate, channels)))
                        .map_err(|e| e.to_string())?;
                    let mut socket = loop {
                        if stopping.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        match listener.accept() {
                            Ok((socket, _)) => break socket,
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(10))
                            }
                            Err(e) => return Err(e.to_string()),
                        }
                    };
                    socket.set_nonblocking(false).map_err(|e| e.to_string())?;
                    socket
                        .set_write_timeout(Some(Duration::from_millis(100)))
                        .map_err(|e| e.to_string())?;
                    socket.set_nodelay(true).map_err(|e| e.to_string())?;
                    // Capture may have been running while FFmpeg opened the video input.
                    // Do not replay that pre-connection backlog as current system audio.
                    while rx.try_recv().is_ok() {}
                    let mut clock = crate::audio_mix::PacketClock::new(rate);
                    let mut pending = VecDeque::new();
                    let mut next = Instant::now();
                    while !stopping.load(Ordering::Relaxed) {
                        let samples = clock.frames() * channels as usize;
                        for data in rx.try_iter() {
                            pending.extend(data);
                        }
                        // Bounded latency. Audio underflow is filled with silence to keep the muxer moving.
                        while pending.len() > samples * 5 {
                            pending.drain(..samples);
                        }
                        let mut bytes = Vec::with_capacity(samples * 4);
                        for _ in 0..samples {
                            bytes.extend_from_slice(
                                &pending.pop_front().unwrap_or(0.0_f32).to_le_bytes(),
                            );
                        }
                        crate::program::write_interruptible(&mut socket, &bytes, &stopping)
                            .map_err(|e| format!("FFmpegへの音声転送に失敗: {e}"))?;
                        next += Duration::from_millis(10);
                        if let Some(wait) = next.checked_duration_since(Instant::now()) {
                            thread::sleep(wait);
                        }
                    }
                    drop(stream);
                    Ok(())
                })();
                if let Err(e) = result {
                    if !stopping.load(Ordering::Relaxed) {
                        *failure.lock().unwrap() = Some(e.clone());
                    }
                    let _ = ready_tx.send(Err(e));
                }
            });
            match ready_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok((rate, channels))) => Ok(Self {
                    rate,
                    channels,
                    port,
                    error,
                    stop,
                    worker: Some(worker),
                }),
                result => {
                    stop.store(true, Ordering::Relaxed);
                    Err(format!("音声入力の初期化に失敗: {result:?}"))
                }
            }
        }
    }
    impl Drop for Bridge {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires FFmpeg; uses an intentionally missing DLL, never opens a camera"]
    fn camera_failure_keeps_recording_and_preview_running() {
        struct Override(Option<std::ffi::OsString>);
        impl Drop for Override {
            fn drop(&mut self) {
                if let Some(p) = self.0.take() {
                    std::env::set_var("STREAMSMASH_CAMERA_DLL", p);
                } else {
                    std::env::remove_var("STREAMSMASH_CAMERA_DLL");
                }
            }
        }
        let root = PathBuf::from(
            std::env::var_os("STREAMSMASH_CAMERA_FAILURE_TEST_DIR")
                .expect("set STREAMSMASH_CAMERA_FAILURE_TEST_DIR"),
        );
        let _override = Override(std::env::var_os("STREAMSMASH_CAMERA_DLL"));
        std::env::set_var(
            "STREAMSMASH_CAMERA_DLL",
            root.join("intentionally-missing-camera.dll"),
        );
        let controller = Controller::new();
        let mut config = test_config(RecordingFormat::AacMp4, 0);
        config.source = Source::Synthetic;
        config.audio = Audio::Tone;
        config.folder = root;
        config.preview = PreviewQuality::Economy;
        config.virtual_camera = true;
        controller.commands.send(Control::Record(config)).unwrap();
        let mut warning = false;
        let mut started = false;
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline && !(warning && started) {
            match controller.events.recv_timeout(Duration::from_millis(500)) {
                Ok(Event::Warning(e)) => {
                    assert!(e.contains("仮想カメラ"));
                    warning = true;
                }
                Ok(Event::Started { .. }) => started = true,
                Ok(Event::Error(e)) => panic!("camera failure must not stop capture: {e}"),
                _ => {}
            }
        }
        assert!(warning && started);
        assert_eq!(controller.camera_state.load(Ordering::Relaxed), 3);
        let before = controller.latest.lock().unwrap().clone().unwrap();
        thread::sleep(Duration::from_millis(500));
        let after = controller.latest.lock().unwrap().clone().unwrap();
        assert_ne!(before.rgba, after.rgba);
        controller.commands.send(Control::Stop).unwrap();
        loop {
            match controller
                .events
                .recv_timeout(Duration::from_secs(12))
                .unwrap()
            {
                Event::Stopped { saved: Some(p) } => {
                    assert!(std::fs::metadata(p).unwrap().len() > 1024);
                    break;
                }
                Event::Error(e) => panic!("{e}"),
                _ => {}
            }
        }
    }

    #[test]
    #[ignore = "requires FFmpeg; verifies live -> ordered AV highlights -> live without restarting recording"]
    fn highlights_switch_program_and_keep_capture_running() {
        let root = PathBuf::from(
            std::env::var_os("STREAMSMASH_MATCH_TEST_DIR").expect("set STREAMSMASH_MATCH_TEST_DIR"),
        );
        std::fs::create_dir_all(&root).unwrap();
        let mut clips = vec![];
        for (color, hz) in [("red", 880), ("blue", 1320)] {
            let path = root.join(format!("{color}.mp4"));
            assert!(command()
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("color={color}:s=640x360:r=30"),
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("sine=frequency={hz}:sample_rate=48000"),
                    "-t",
                    "2",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-pix_fmt",
                    "yuv420p",
                    "-c:a",
                    "aac"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success());
            assert!((1.9..2.1).contains(&crate::program::mp4_duration(&path).unwrap()));
            clips.push(path);
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        struct StreamReceiver(Child);
        impl Drop for StreamReceiver {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let stream_path = root.join("program-stream.flv");
        let mut receiver = StreamReceiver(
            command()
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-listen",
                    "1",
                    "-i",
                    &format!("rtmp://127.0.0.1:{port}/live/local-fixture"),
                    "-c",
                    "copy",
                ])
                .arg(&stream_path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        thread::sleep(Duration::from_millis(300));
        let controller = Controller::new();
        let mut config = test_config(RecordingFormat::AacMp4, 0);
        config.source = Source::Synthetic;
        config.audio = Audio::Tone;
        config.folder = root.clone();
        config.replay_seconds = Some(10);
        config.preview = PreviewQuality::Economy;
        config.width = 1280;
        config.height = 720;
        controller
            .commands
            .send(Control::Stream {
                config,
                target: crate::streaming::StreamTarget::local_test(port),
                record: true,
            })
            .unwrap();
        let mut starts = 0;
        let mut path = None;
        let deadline = Instant::now() + Duration::from_secs(20);
        while path.is_none() && Instant::now() < deadline {
            match controller.events.recv_timeout(Duration::from_secs(1)) {
                Ok(Event::Started { path: p, .. }) => {
                    starts += 1;
                    path = p;
                }
                Ok(Event::Error(e)) => panic!("{e}"),
                _ => {}
            }
        }
        assert!(path.is_some(), "capture did not start");
        thread::sleep(Duration::from_secs(1));
        controller
            .commands
            .send(Control::PlayHighlights(clips))
            .unwrap();
        let mut colors = vec![];
        let mut finished = false;
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            while let Ok(e) = controller.events.try_recv() {
                match e {
                    Event::HighlightsFinished => finished = true,
                    Event::Started { .. } => starts += 1,
                    Event::Error(e) => panic!("{e}"),
                    Event::Warning(e) => println!("warning: {e}"),
                    _ => {}
                }
            }
            if let Some(f) = controller.latest.lock().unwrap().take() {
                let solid = |r: u8, b: u8| {
                    f.rgba
                        .chunks_exact(4)
                        .enumerate()
                        .filter(|(i, _)| {
                            let x = *i as u32 % f.width;
                            let y = *i as u32 / f.width;
                            x > f.width / 4
                                && x < f.width * 3 / 4
                                && y > f.height / 4
                                && y < f.height * 3 / 4
                        })
                        .step_by(997)
                        .all(|(_, p)| {
                            (i16::from(p[0]) - i16::from(r)).abs() < 25
                                && p[1] < 25
                                && (i16::from(p[2]) - i16::from(b)).abs() < 25
                        })
                };
                let color = if solid(255, 0) {
                    "red"
                } else if solid(0, 255) {
                    "blue"
                } else {
                    "live"
                };
                if color == "red" || color == "blue" {
                    let at = ((20 * f.width + 17) * 4) as usize;
                    let accent = &f.rgba[at..at + 3];
                    assert!(
                        accent[0] > 210 && accent[1] > 30 && accent[2] > 50,
                        "replay identity missing from program preview: {accent:?}"
                    );
                    let screenshot = root.join(format!("program-{color}-preview.png"));
                    if !screenshot.exists() {
                        image::RgbaImage::from_raw(f.width, f.height, f.rgba.clone())
                            .unwrap()
                            .save(screenshot)
                            .unwrap();
                    }
                }
                if colors.last() != Some(&color) {
                    colors.push(color);
                }
            }
            if finished && colors.last() == Some(&"live") {
                break;
            }
            thread::sleep(Duration::from_millis(30));
        }
        thread::sleep(Duration::from_secs(1));
        let (reply, result) = mpsc::channel();
        controller
            .commands
            .send(Control::SaveReplayRemote(reply))
            .unwrap();
        let ring = result
            .recv_timeout(Duration::from_secs(15))
            .unwrap()
            .unwrap();
        println!("live ring={}", ring.display());
        controller.commands.send(Control::Stop).unwrap();
        loop {
            match controller
                .events
                .recv_timeout(Duration::from_secs(12))
                .unwrap()
            {
                Event::Stopped { .. } => break,
                Event::Error(e) => panic!("{e}"),
                _ => {}
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while receiver.0.try_wait().unwrap().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(std::fs::metadata(&stream_path).unwrap().len() > 10000);
        println!("colors={colors:?}, recording={:?}", path);
        assert_eq!(starts, 1, "capture/stream must not restart");
        assert!(finished, "playlist must end");
        assert!(
            colors.windows(3).any(|c| c == ["red", "blue", "live"]),
            "wrong program sequence: {colors:?}"
        );
    }

    #[test]
    fn parses_names_and_unambiguous_ids() {
        let devices = parse_devices("[dshow] \"USB capture\" (video)\n[dshow] Alternative name \"@device_path\"\n[dshow] \"HDMI 音声\" (audio)\n[dshow] Alternative name \"@audio\"\n");
        assert_eq!(
            devices.video,
            vec![Device {
                name: "USB capture".into(),
                id: "@device_path".into()
            }]
        );
        assert_eq!(devices.audio[0].name, "HDMI 音声");
        assert_eq!(devices.audio[0].id, "@audio");
    }
    #[test]
    fn filenames_and_devices_are_arguments_not_shell_commands() {
        let config = Config {
            scene: crate::scene::Scene::default(),
            replay_seconds: None,
            virtual_camera: false,
            virtual_camera_mirror: false,
            recording_format: RecordingFormat::OpusMkv,
            tuning: Default::default(),
            audio_offset_ms: 0,
            preview: PreviewQuality::FullHd,
            source: Source::Device("name with spaces & quotes".into()),
            audio: Audio::None,
            microphone: None,
            encoder: Encoder::Software,
            width: 1280,
            height: 720,
            fps: 30,
            folder: "recordings".into(),
        };
        let args = input_args(&config, None);
        assert_eq!(args.last().unwrap(), "video=name with spaces & quotes");
        let path = PathBuf::from("folder with spaces/movie.mp4");
        let args = record_args(&config, &path, None);
        assert_eq!(args.last().unwrap(), "folder with spaces/movie.mp4");
        assert!(args.iter().any(|s| s == "-n"));
        assert!(!args.iter().any(|s| s == "1:a:0"));
    }

    #[test]
    fn combined_devices_preserve_sample_clock_and_audio_stream() {
        let config = test_config(RecordingFormat::OpusMkv, 0);
        let args = input_args(&config, Some("@audio-device"));
        assert!(!args.iter().any(|s| s == "-use_wallclock_as_timestamps"));
        assert_eq!(
            args.last().unwrap(),
            "video=@video-device:audio=@audio-device"
        );
        assert!(args.windows(2).any(|s| s == ["-audio_buffer_size", "50"]));
        let args = record_args(&config, Path::new("sample.mkv"), Some("0:a:0"));
        assert!(args.windows(2).any(|s| s == ["-map", "0:a:0"]));
        assert!(args.iter().any(|s| s == "libopus"));
        assert!(!args.iter().any(|s| s == "1:a:0"));
    }

    #[test]
    fn output_tuning_reaches_record_encoder_and_live_audio_filter() {
        let mut config = test_config(RecordingFormat::OpusMkv, 0);
        config.width = 640;
        config.height = 360;
        config.fps = 24;
        config.tuning.gain_percent = 50;
        config.tuning.limiter = true;
        config.tuning.audio_kbps = 128;
        config.tuning.record_crf = 18;
        assert!(config.validate().is_ok());
        let filter = audio_filter(&config);
        assert!(filter.contains("volume=0.50"));
        assert!(filter.contains("latency=1"));
        let args = record_args(&config, Path::new("fixture.mkv"), Some("0:a:0"));
        assert!(args.windows(2).any(|s| s == ["-b:a", "128k"]));
        assert!(args.windows(2).any(|s| s == ["-crf", "18"]));
        assert!(args.windows(2).any(|s| s == ["-r", "24"]));
        config.encoder = Encoder::Nvidia;
        config.tuning.gpu_record_kbps = 18000;
        let args = record_args(&config, Path::new("fixture.mkv"), Some("0:a:0"));
        assert!(args.windows(2).any(|s| s == ["-b:v", "18000k"]));
    }
    #[test]
    #[ignore = "FFmpeg offline synthetic PCM only; never opens an audio device"]
    fn gain_mute_and_limiter_process_synthetic_pcm() {
        let run = |gain, limiter, amplitude: f32| {
            let mut config = test_config(RecordingFormat::OpusMkv, 0);
            config.tuning.gain_percent = gain;
            config.tuning.limiter = limiter;
            let source = format!("aevalsrc={amplitude}*sin(2*PI*1000*t):s=48000:d=1");
            let output = Command::new(ffmpeg_path())
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    &source,
                    "-af",
                    &audio_filter(&config),
                    "-f",
                    "f32le",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let samples: Vec<f32> = output
                .stdout
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            assert!(samples.len() > 47000);
            let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt();
            let peak = samples.iter().map(|x| x.abs()).fold(0f32, f32::max);
            (rms, peak)
        };
        let base = run(100, false, 0.25);
        let half = run(50, false, 0.25);
        let mute = run(0, false, 0.25);
        let limited = run(100, true, 1.5);
        assert!((half.0 / base.0 - 0.5).abs() < 0.002);
        assert!(mute.1 < 0.00001);
        assert!(limited.1 <= 0.951 && limited.1 > 0.9);
        println!("baseline={base:?}, half={half:?}, mute={mute:?}, limited={limited:?}");
    }

    fn test_config(recording_format: RecordingFormat, audio_offset_ms: i32) -> Config {
        Config {
            scene: crate::scene::Scene::default(),
            replay_seconds: None,
            virtual_camera: false,
            virtual_camera_mirror: false,
            recording_format,
            tuning: Default::default(),
            audio_offset_ms,
            preview: PreviewQuality::FullHd,
            source: Source::Device("@video-device".into()),
            audio: Audio::Device("@audio-device".into()),
            microphone: None,
            encoder: Encoder::Software,
            width: 1280,
            height: 720,
            fps: 30,
            folder: "recordings".into(),
        }
    }

    #[test]
    #[ignore = "requires FFmpeg; records synthetic frames and tests live replay destination changes"]
    fn replay_folder_changes_apply_without_restarting_capture() {
        let root = std::env::var_os("STREAMSMASH_FOLDER_TEST_DIR")
            .map(PathBuf::from)
            .expect("set STREAMSMASH_FOLDER_TEST_DIR");
        let controller = Controller::new();
        let mut config = test_config(RecordingFormat::OpusMkv, 0);
        config.source = Source::Synthetic;
        config.audio = Audio::Tone;
        config.folder = root.join("通常録画");
        config.replay_seconds = Some(10);
        std::fs::create_dir_all(&root).unwrap();
        let bridge_path = root.join("test-connection.json");
        let _bridge = crate::replaylab::Bridge::start_with_frames(
            &bridge_path,
            controller.commands.clone(),
            controller.recognition_frames.clone(),
        )
        .unwrap();
        let descriptor: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&bridge_path).unwrap()).unwrap();
        let check_recognition = || {
            use std::net::TcpStream;
            let host = descriptor["url"]
                .as_str()
                .unwrap()
                .strip_prefix("http://")
                .unwrap();
            let token = descriptor["token"].as_str().unwrap();
            let mut socket = TcpStream::connect(host).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(socket,"GET /v1/frame.png HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\n\r\n").unwrap();
            let mut response = Vec::new();
            socket.read_to_end(&mut response).unwrap();
            assert!(
                response.starts_with(b"HTTP/1.1 200"),
                "recognition must stay fresh without a GUI consumer"
            );
        };
        controller
            .commands
            .send(Control::Record(config.clone()))
            .unwrap();
        let mut starts = 0;
        let record_path = loop {
            match controller
                .events
                .recv_timeout(Duration::from_secs(15))
                .unwrap()
            {
                Event::Started {
                    path: Some(path), ..
                } => {
                    starts += 1;
                    break path;
                }
                Event::Error(e) | Event::Warning(e) => panic!("{e}"),
                _ => {}
            }
        };
        thread::sleep(Duration::from_millis(4500));
        check_recognition();
        for (index, name) in ["撃墜クリップ A", "別の保存先 B"].iter().enumerate() {
            let folder = std::path::absolute(root.join(name)).unwrap();
            controller
                .commands
                .send(Control::SetReplayFolder(folder.clone()))
                .unwrap();
            loop {
                match controller
                    .events
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                {
                    Event::ReplayFolderChanged(path) => {
                        assert_eq!(path, folder);
                        break;
                    }
                    Event::Starting | Event::Started { .. } => {
                        panic!("capture restarted while changing destination")
                    }
                    Event::Error(e) | Event::Warning(e) => panic!("{e}"),
                    _ => {}
                }
            }
            let (reply, rx) = mpsc::channel();
            controller
                .commands
                .send(if index == 0 {
                    Control::SaveHighlight { seconds: 6, reply }
                } else {
                    Control::SaveReplayRemote(reply)
                })
                .unwrap();
            let saved = rx.recv_timeout(Duration::from_secs(15)).unwrap().unwrap();
            assert_eq!(saved.parent(), Some(folder.as_path()));
            assert!(std::fs::metadata(&saved).unwrap().len() > 1000);
            println!("Replay saved: {}", saved.display());
            check_recognition();
            thread::sleep(Duration::from_millis(100));
        }
        controller.commands.send(Control::Stop).unwrap();
        loop {
            match controller
                .events
                .recv_timeout(Duration::from_secs(15))
                .unwrap()
            {
                Event::Stopped { saved } => {
                    assert_eq!(saved, Some(record_path.clone()));
                    break;
                }
                Event::Started { .. } => starts += 1,
                Event::Error(e) | Event::Warning(e) => panic!("{e}"),
                _ => {}
            }
        }
        assert_eq!(starts, 1);
        assert_eq!(record_path.parent(), Some(config.folder.as_path()));
        assert!(
            controller.recognition_frames.is_empty(),
            "stop must invalidate recognition frames"
        );
        assert!(std::fs::metadata(record_path).unwrap().len() > 1000);
    }

    /// Measures encoded CONTENT, not just stream duration/first packet timestamps.
    /// 25 flash/beep pairs over two minutes exercise priming, mux timestamps and drift.
    #[test]
    #[ignore = "requires FFmpeg with libopus/libx264 plus ffprobe; writes sync fixtures"]
    fn encoded_flash_and_beep_stay_synchronized() {
        use std::process::Output;
        fn checked(mut command: Command) -> Output {
            let output = command.output().expect("start FFmpeg/ffprobe");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            output
        }
        fn first_frame_time(path: &Path, selector: &str) -> f64 {
            let mut cmd = Command::new(
                std::env::var_os("STREAMSMASH_FFPROBE").unwrap_or_else(|| "ffprobe".into()),
            );
            cmd.args([
                "-v",
                "error",
                "-select_streams",
                selector,
                "-read_intervals",
                "0%+0.15",
                "-show_entries",
                "frame=best_effort_timestamp_time",
                "-of",
                "csv=p=0",
            ])
            .arg(path);
            let output = checked(cmd);
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find_map(|l| l.split(',').next()?.trim().parse().ok())
                .expect("frame PTS")
        }
        let dir = std::env::var_os("STREAMSMASH_SYNC_TEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("streamsmash-sync-tests"));
        std::fs::create_dir_all(&dir).unwrap();
        for (format, offset, duration, limiter) in [
            (RecordingFormat::OpusMkv, 0, 125, false),
            (RecordingFormat::AacMp4, 0, 125, false),
            (RecordingFormat::OpusMkv, -250, 15, false),
            (RecordingFormat::OpusMkv, 0, 15, true),
        ] {
            let path = dir.join(format!(
                "flash-beep-{}-{offset}-{limiter}-{}.{}",
                std::process::id(),
                duration,
                format.extension()
            ));
            let mut config = test_config(format, offset);
            if std::env::var("STREAMSMASH_SYNC_GPU").as_deref() == Ok("amd") {
                config.encoder = Encoder::Amd;
            }
            config.tuning.limiter = limiter;
            if limiter {
                config.tuning.gain_percent = 200;
            }
            let mut cmd = command();
            cmd.args(["-hide_banner","-loglevel","error","-f","lavfi","-i"])
                .arg(format!("color=c=black:s=160x90:r=30:d={duration},drawbox=color=white:t=fill:enable='between(mod(t,5),1,1.05)'"))
                .args(["-f","lavfi","-i"])
                .arg(format!("aevalsrc='if(between(mod(t,5),1,1.02),0.5*sin(2*PI*1000*t),0)':s=48000:d={duration}"));
            // Exercise the actual common conversion + replay insert graph,
            // rather than validating only the standalone encoder arguments.
            cmd.args(["-f", "lavfi", "-i"]).arg(format!(
                "color=c=black:s=160x90:r=30:d={duration},format=rgba,colorchannelmixer=aa=0"
            ));
            let graph = crate::scene::filter_graph(
                &[],
                (config.width, config.height),
                (160, 90, 30),
                true,
                false,
                true,
                crate::scene::GraphInputs {
                    program: Some(2),
                    audio: Some("1:a:0"),
                    timer: None,
                },
            );
            cmd.args(["-filter_complex_threads", "4", "-filter_complex", &graph]);
            let mut args = record_args_with_video(&config, &path, Some("1:a:0"), "[v_record]");
            let file = args.pop().unwrap();
            args.extend(["-threads".into(), "2".into(), file]);
            cmd.args(args);
            cmd.args([
                "-map",
                "[v_preview]",
                "-an",
                "-f",
                "null",
                "-",
                "-map",
                "[v_buffer]",
                "-an",
                "-f",
                "null",
                "-",
            ]);
            checked(cmd);
            let mut cmd = command();
            cmd.args(["-v", "error", "-i"]).arg(&path).args([
                "-map",
                "0:v:0",
                "-vf",
                "scale=1:1:flags=area,format=gray",
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "pipe:1",
            ]);
            let frames = checked(cmd).stdout;
            let video_start = first_frame_time(&path, "v:0");
            let mut flashes = Vec::new();
            for (i, pixel) in frames.iter().enumerate() {
                if *pixel > 128 && (i == 0 || frames[i - 1] <= 128) {
                    flashes.push(video_start + i as f64 / 30.0);
                }
            }
            let mut cmd = command();
            cmd.args(["-v", "error", "-i"]).arg(&path).args([
                "-map", "0:a:0", "-ac", "1", "-ar", "48000", "-f", "f32le", "pipe:1",
            ]);
            let bytes = checked(cmd).stdout;
            let audio_start = first_frame_time(&path, "a:0");
            let mut beeps = Vec::new();
            let mut last_strong = None;
            for (i, bytes) in bytes.chunks_exact(4).enumerate() {
                if f32::from_le_bytes(bytes.try_into().unwrap()).abs() > 0.1 {
                    if last_strong.is_none_or(|last| i - last > 4800) {
                        beeps.push(audio_start + i as f64 / 48000.0);
                    }
                    last_strong = Some(i);
                }
            }
            assert_eq!(
                flashes.len(),
                beeps.len(),
                "flash/beep count: {}",
                path.display()
            );
            assert!(flashes.len() >= if duration > 100 { 25 } else { 3 });
            let errors: Vec<_> = flashes
                .iter()
                .zip(&beeps)
                .map(|(v, a)| a - v - f64::from(offset) / 1000.0)
                .collect();
            let max = errors.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
            let drift = (errors.last().unwrap() - errors.first().unwrap()).abs();
            println!(
                "{:?} offset={offset}ms limiter={limiter} pairs={} max_error={:.2}ms drift={:.2}ms",
                format,
                flashes.len(),
                max * 1000.0,
                drift * 1000.0
            );
            assert!(
                max < 0.050,
                "A/V content offset exceeds 50ms: {max}, {}",
                path.display()
            );
            assert!(drift < 0.020, "A/V content drift exceeds 20ms: {drift}");
        }
    }
    #[test]
    #[ignore = "requires FFmpeg, Edge/Chrome and loopback sockets; no GUI or desktop capture"]
    fn studio_composes_records_streams_and_saves_replay() {
        use crate::scene::{Layer, LayerSource};
        let directory = std::env::var_os("STREAMSMASH_STUDIO_TEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("streamsmash-studio-tests"));
        std::fs::create_dir_all(&directory).unwrap();
        let asset = directory.join("opaque-red.png");
        image::RgbaImage::from_pixel(200, 100, image::Rgba([255, 0, 0, 255]))
            .save(&asset)
            .unwrap();
        let alpha_asset = directory.join("half-red.png");
        image::RgbaImage::from_pixel(200, 100, image::Rgba([255, 0, 0, 128]))
            .save(&alpha_asset)
            .unwrap();
        let black_asset = directory.join("black.png");
        image::RgbaImage::from_pixel(1920, 1080, image::Rgba([0, 0, 0, 255]))
            .save(&black_asset)
            .unwrap();
        let browser_asset = directory.join("transparent-overlay.html");
        std::fs::write(&browser_asset,"<style>body{margin:0;background:transparent}div{width:100px;height:100px;background:lime}</style><div></div>").unwrap();
        let mut config = test_config(RecordingFormat::OpusMkv, 0);
        config.source = Source::Synthetic;
        config.audio = Audio::Tone;
        if std::env::var("STREAMSMASH_TEST_ENCODER").as_deref() == Ok("amd") {
            config.encoder = Encoder::Amd;
        }
        if std::env::var_os("STREAMSMASH_TEST_MIC").is_some() {
            config.microphone = Some("synthetic-tone".into());
            config.tuning.limiter = true;
        }
        config.preview = PreviewQuality::Economy;
        config.folder = directory.clone();
        config.replay_seconds = Some(10);
        config.virtual_camera = std::env::var_os("STREAMSMASH_CAMERA_DLL").is_some();
        let layer = |name: &str, source, x, y, width, height| Layer {
            name: name.into(),
            enabled: true,
            locked: true,
            source,
            x,
            y,
            width,
            height,
            opacity: 1.0,
        };
        config.scene.layers = vec![
            layer("red", LayerSource::Image { path: asset }, 0, 0, 200, 100),
            layer(
                "HTML",
                LayerSource::Browser {
                    location: browser_asset.to_string_lossy().into_owned(),
                },
                300,
                0,
                200,
                100,
            ),
            layer("RTA", LayerSource::Timer, 600, 0, 700, 100),
            layer("wave", LayerSource::Waveform, 0, 850, 1000, 150),
        ];
        config.scene.layers.insert(
            0,
            layer(
                "background",
                LayerSource::Image { path: black_asset },
                0,
                0,
                1920,
                1080,
            ),
        );
        config.scene.layers.push(layer(
            "alpha",
            LayerSource::Image { path: alpha_asset },
            1600,
            800,
            200,
            100,
        ));
        if std::env::var_os("STREAMSMASH_TEST_HTML").is_none() {
            config
                .scene
                .layers
                .retain(|l| !matches!(l.source, LayerSource::Browser { .. }));
        }
        if std::env::var_os("STREAMSMASH_TEST_NO_WAVE").is_some() {
            config
                .scene
                .layers
                .retain(|l| !matches!(l.source, LayerSource::Waveform));
        }
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let received_path = directory.join(format!(
            "local-rtmp-{}.flv",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        struct Receiver(Child);
        impl Drop for Receiver {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut receiver = Receiver(
            command()
                .args(["-hide_banner", "-loglevel", "error", "-listen", "1", "-i"])
                .arg(format!("rtmp://127.0.0.1:{port}/live/local-fixture"))
                .args(["-c", "copy", "-y"])
                .arg(&received_path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        thread::sleep(Duration::from_millis(300));
        let latest = Arc::new(Mutex::new(None));
        let clock = crate::timer::RaceTimer::default();
        clock.apply(crate::timer::TimerAction::Start);
        let mut session = Session::start(
            &config,
            true,
            Some(crate::streaming::StreamTarget::local_test(port)),
            latest.clone(),
            clock,
            Arc::new(AtomicU8::new(0)),
            crate::replaylab::Frames::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(35);
        while !session.received.load(Ordering::Acquire) && Instant::now() < deadline {
            if let Some(status) = session.child.try_wait().unwrap() {
                panic!("early exit {status}: {}", session.diagnostics());
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(
            session.received.load(Ordering::Acquire),
            "{}",
            session.diagnostics()
        );
        let timer_region = |frame: &PreviewFrame| -> Vec<u8> {
            let mut out = Vec::new();
            for y in 0..34 {
                let start = ((y * frame.width + 200) * 4) as usize;
                out.extend_from_slice(&frame.rgba[start..start + 230 * 4]);
            }
            out
        };
        let mut middle_timer = None;
        for second in 0..18 {
            thread::sleep(Duration::from_secs(1));
            if let Some(status) = session.child.try_wait().unwrap() {
                panic!("live ffmpeg exit {status}: {}", session.diagnostics());
            }
            if second == 8 {
                middle_timer = Some(timer_region(latest.lock().unwrap().as_ref().unwrap()));
            }
            if let Some(error) = session.browsers.iter().find_map(|b| b.error()) {
                panic!("live overlay error: {error}");
            }
        }

        assert_ne!(
            middle_timer.unwrap(),
            timer_region(latest.lock().unwrap().as_ref().unwrap()),
            "timer stopped updating"
        );
        let pixel = {
            let frame = latest.lock().unwrap();
            let f = frame.as_ref().unwrap();
            let at = ((10 * f.width + 20) * 4) as usize;
            f.rgba[at..at + 4].to_vec()
        };
        assert!(
            pixel[0] > 220 && pixel[1] < 30 && pixel[2] < 30,
            "image missing from preview {pixel:?}"
        );
        {
            let f = {
                let frame = latest.lock().unwrap();
                let f = frame.as_ref().unwrap();
                PreviewFrame {
                    width: f.width,
                    height: f.height,
                    rgba: f.rgba.clone(),
                }
            };
            image::save_buffer(
                directory.join("composed-preview.png"),
                &f.rgba,
                f.width,
                f.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
            let pixel = |x: u32, y: u32| {
                let at = ((y * f.width + x) * 4) as usize;
                &f.rgba[at..at + 4]
            };
            let half = pixel(550, 280);
            assert!(
                (110..145).contains(&half[0]) && half[1] < 10,
                "PNG alpha incorrect: {half:?}"
            );
            if std::env::var_os("STREAMSMASH_TEST_HTML").is_some() {
                let green = pixel(110, 10);
                assert!(green[1] > 220 && green[0] < 30, "HTML absent: {green:?}");
                let transparent = pixel(155, 10);
                assert!(
                    transparent[0] < 10 && transparent[1] < 10 && transparent[2] < 10,
                    "HTML alpha lost: {transparent:?}"
                );
            }
            let wave = f.rgba.chunks_exact(4).enumerate().any(|(i, p)| {
                let y = i as u32 / f.width;
                y > 280 && y < 333 && p[1] > 130 && p[2] > 130 && p[0] < 100
            });
            println!(
                "wave region max green {}",
                f.rgba
                    .chunks_exact(4)
                    .enumerate()
                    .filter(|(i, _)| {
                        let y = *i as u32 / f.width;
                        y > 280 && y < 333
                    })
                    .map(|(_, p)| p[1])
                    .max()
                    .unwrap()
            );
            assert!(wave, "actual-audio waveform absent");
            let timer = f.rgba.chunks_exact(4).enumerate().any(|(i, p)| {
                let x = i as u32 % f.width;
                let y = i as u32 / f.width;
                (200..430).contains(&x) && y < 34 && p[0] > 230 && p[1] > 230 && p[2] > 230
            });
            assert!(timer, "RTA pixels absent");
        }
        assert!(
            session.output_error.lock().unwrap().is_none(),
            "virtual camera sender failed"
        );
        assert!(
            session.browsers.iter().all(|b| b.error().is_none()),
            "overlay input failed: {:?}",
            session
                .browsers
                .iter()
                .map(|b| b.error())
                .collect::<Vec<_>>()
        );
        let snapshot = session
            .replay
            .as_ref()
            .unwrap()
            .snapshot(&directory)
            .unwrap();
        let replay = snapshot.export().unwrap();
        let probe = std::env::var_os("STREAMSMASH_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                ffmpeg_path().with_file_name(if cfg!(windows) {
                    "ffprobe.exe"
                } else {
                    "ffprobe"
                })
            });
        let duration = Command::new(probe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=nw=1:nk=1",
            ])
            .arg(&replay)
            .output()
            .unwrap();
        assert!(duration.status.success());
        let duration: f64 = String::from_utf8(duration.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!((8.0..10.6).contains(&duration), "ring duration {duration}");
        let path = session.path.clone().unwrap();
        let stopped = session.stop();
        let end = Instant::now() + Duration::from_secs(8);
        while receiver.0.try_wait().unwrap().is_none() && Instant::now() < end {
            thread::sleep(Duration::from_millis(50));
        }
        if receiver.0.try_wait().unwrap().is_none() {
            let _ = receiver.0.kill();
        }
        let _ = receiver.0.wait();
        stopped.unwrap();
        for file in [&path, &replay, &received_path] {
            let output = command()
                .args(["-v", "error", "-i"])
                .arg(file)
                .args([
                    "-frames:v",
                    "1",
                    "-vf",
                    "format=rgb24,crop=1:1:20:10",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}: {}",
                file.display(),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout.len(), 3);
            assert!(
                output.stdout[0] > 210 && output.stdout[1] < 45,
                "red missing from {}: {:?}",
                file.display(),
                output.stdout
            );
            let audio = command()
                .args(["-v", "error", "-i"])
                .arg(file)
                .args(["-t", "0.1", "-map", "0:a:0", "-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                audio.status.success() && audio.stdout.len() > 1000,
                "audio missing: {}",
                file.display()
            );
        }
        println!(
            "Studio OK: image + native RTA + actual-audio waveform; preview/record/RTMP/replay (HTML optional): {}",
            directory.display()
        );
    }
    #[test]
    #[ignore = "requires FFmpeg, OpenSSL and loopback; rejects an untrusted local TLS certificate"]
    fn rtmps_rejects_untrusted_certificate() {
        let openssl = std::env::var_os("STREAMSMASH_OPENSSL").unwrap_or_else(|| "openssl".into());
        let dir = std::env::temp_dir().join(format!(
            "streamsmash-tls-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let cert = dir.join("cert.pem");
        let key = dir.join("key.pem");
        let result = Command::new(&openssl)
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=localhost",
                "-addext",
                "subjectAltName=IP:127.0.0.1",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .output()
            .unwrap();
        assert!(result.status.success());
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let mut server = Command::new(&openssl)
            .args(["s_server", "-accept"])
            .arg(format!("127.0.0.1:{port}"))
            .arg("-cert")
            .arg(&cert)
            .arg("-key")
            .arg(&key)
            .arg("-quiet")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        thread::sleep(Duration::from_millis(250));
        let target = crate::streaming::StreamTarget::new(
            &format!("rtmps://127.0.0.1:{port}/live"),
            "local-fixture".into(),
            2500,
        )
        .unwrap();
        let mut cmd = command();
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:size=1280x720:rate=30",
            "-filter_complex",
            "[0:v]null[v_stream]",
        ]);
        cmd.args(stream_args(
            &test_config(RecordingFormat::OpusMkv, 0),
            &target,
            None,
        ));
        let output = cmd.output();
        let _ = server.kill();
        let _ = server.wait();
        let output = output.unwrap();
        let log = String::from_utf8_lossy(&output.stderr).to_lowercase();
        assert!(
            !output.status.success(),
            "untrusted TLS unexpectedly accepted"
        );
        assert!(
            log.contains("certificate") || log.contains("cert verify"),
            "expected certificate rejection: {}",
            target.redact(&log)
        );
        let _ = std::fs::remove_file(cert);
        let _ = std::fs::remove_file(key);
        let _ = std::fs::remove_dir(dir);
        println!("RTMPS OK: untrusted local certificate rejected");
    }
}
