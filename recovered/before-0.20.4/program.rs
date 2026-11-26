//! Continuous transparent video + four-channel audio insert. Capture and replay ring stay live.
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const W: u32 = 1280;
const H: u32 = 720;
struct Plan {
    list: PathBuf,
    ends: Option<Vec<f64>>,
    count: usize,
    start: Instant,
}
impl Drop for Plan {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.list);
    }
}
type Shared = Arc<Mutex<Option<Arc<Plan>>>>;
pub(crate) struct Insert {
    pub video_port: u16,
    pub audio_port: u16,
    plan: Shared,
    overlay: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}
impl Insert {
    pub fn start() -> Result<Self, String> {
        let video = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let audio = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let video_port = video.local_addr().unwrap().port();
        let audio_port = audio.local_addr().unwrap().port();
        let plan: Shared = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let overlay = Arc::new(AtomicBool::new(true));
        let epoch = Arc::new(Mutex::new(None));
        let mut workers = vec![];
        for (listener, is_video) in [(video, true), (audio, false)] {
            listener.set_nonblocking(true).map_err(|e| e.to_string())?;
            let shared = plan.clone();
            let stopping = stop.clone();
            let failure = error.clone();
            let clock = epoch.clone();
            let style = overlay.clone();
            workers.push(thread::spawn(move || {
                if let Err(e) = feed(
                    listener, is_video, &shared, &stopping, &failure, &clock, &style,
                ) {
                    if !stopping.load(Ordering::Relaxed) {
                        *failure.lock().unwrap() = Some(e);
                    }
                }
            }));
        }
        Ok(Self {
            video_port,
            audio_port,
            plan,
            overlay,
            stop,
            workers,
            error,
        })
    }
    pub fn play(&self, paths: Vec<PathBuf>, lead_ms: u32) -> Result<(), String> {
        if paths.is_empty() {
            return Err("この試合の撃墜クリップがありません".into());
        }
        let count = paths.len();
        let ends = paths
            .iter()
            .map(|p| mp4_duration(p))
            .collect::<Option<Vec<_>>>()
            .map(|mut times| {
                let mut sum = 0.;
                for t in &mut times {
                    sum += *t;
                    *t = sum;
                }
                times
            });
        let mut text = String::new();
        for path in paths {
            let p = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
            let p = p
                .to_str()
                .ok_or("動画のパスを読み取れません")?
                .replace('\\', "/");
            if p.contains(['\n', '\r']) {
                return Err("動画のパスに改行があります".into());
            }
            text.push_str(&format!("file '{}'\n", p.replace('\'', "'\\''")));
        }
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let list =
            std::env::temp_dir().join(format!("streamsmash-play-{}-{id}.txt", std::process::id()));
        std::fs::write(&list, text).map_err(|e| e.to_string())?;
        *self.plan.lock().unwrap() = Some(Arc::new(Plan {
            list,
            count,
            ends,
            start: Instant::now() + Duration::from_millis(u64::from(lead_ms.clamp(1000, 120000))),
        }));
        Ok(())
    }
    pub fn set_overlay(&self, enabled: bool) {
        self.overlay.store(enabled, Ordering::Relaxed);
    }
    pub fn cancel(&self) {
        *self.plan.lock().unwrap() = None;
    }
    pub fn playing(&self) -> bool {
        self.plan.lock().unwrap().is_some()
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.cancel();
    }
}
impl Drop for Insert {
    fn drop(&mut self) {
        self.request_stop();
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}
// Exported replays are ordinary MP4 files. Read mvhd without another process.
// Unknown/unsupported duration keeps playback working and displays total count.
pub(crate) fn mp4_duration(path: &Path) -> Option<f64> {
    use std::io::{Seek, SeekFrom};
    fn atom(file: &mut std::fs::File, mut at: u64, end: u64, name: &[u8; 4]) -> Option<(u64, u64)> {
        for _ in 0..10000 {
            if at.checked_add(8)? > end {
                return None;
            }
            file.seek(SeekFrom::Start(at)).ok()?;
            let mut h = [0u8; 8];
            file.read_exact(&mut h).ok()?;
            let mut size = u32::from_be_bytes(h[..4].try_into().ok()?) as u64;
            let mut header = 8;
            if size == 1 {
                let mut large = [0u8; 8];
                file.read_exact(&mut large).ok()?;
                size = u64::from_be_bytes(large);
                header = 16;
            } else if size == 0 {
                size = end - at;
            }
            let next = at.checked_add(size)?;
            if size < header || next > end {
                return None;
            }
            if &h[4..8] == name {
                return Some((at + header, next));
            }
            at = next;
        }
        None
    }
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let (start, end) = atom(&mut file, 0, len, b"moov")?;
    let (start, end) = atom(&mut file, start, end, b"mvhd")?;
    if end - start < 20 {
        return None;
    }
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut h = [0u8; 32];
    file.read_exact(&mut h[..(end - start).min(32) as usize])
        .ok()?;
    let (rate, time) = match h[0] {
        0 => (
            u32::from_be_bytes(h[12..16].try_into().ok()?),
            u32::from_be_bytes(h[16..20].try_into().ok()?) as u64,
        ),
        1 if end - start >= 32 => (
            u32::from_be_bytes(h[20..24].try_into().ok()?),
            u64::from_be_bytes(h[24..32].try_into().ok()?),
        ),
        _ => return None,
    };
    if rate == 0 {
        return None;
    }
    let seconds = time as f64 / f64::from(rate);
    (seconds > 0. && seconds <= 86400.).then_some(seconds)
}

struct Decoder(Child);
impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn decoder(list: &Path, video: bool) -> Result<Decoder, String> {
    let mut command = crate::recorder::command();
    command
        .args([
            "-v", "error", "-nostdin", "-f", "concat", "-safe", "0", "-i",
        ])
        .arg(list);
    if video {
        command.args(["-an","-vf",&format!("fps=30,scale={W}:{H}:force_original_aspect_ratio=decrease,pad={W}:{H}:(ow-iw)/2:(oh-ih)/2,setsar=1"),"-pix_fmt","rgba","-threads","1","-f","rawvideo","pipe:1"]);
    } else {
        command.args(["-vn", "-ar", "48000", "-ac", "2", "-f", "f32le", "pipe:1"]);
    }
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map(Decoder)
        .map_err(|e| e.to_string())
}
fn feed(
    listener: TcpListener,
    video: bool,
    shared: &Shared,
    stop: &AtomicBool,
    error: &Mutex<Option<String>>,
    epoch: &Mutex<Option<Instant>>,
    overlay: &AtomicBool,
) -> Result<(), String> {
    let mut socket = loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match listener.accept() {
            Ok((s, _)) => break s,
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
    let mut frame_number = 0u64;
    let mut current: Option<Arc<Plan>> = None;
    let mut decode: Option<Decoder> = None;
    let mut data = vec![
        0u8;
        if video {
            (W * H * 4) as usize
        } else {
            1600 * 2 * 4
        }
    ];
    // Both demuxers start at PTS zero, even though FFmpeg connects sequentially.
    // Catch up the later socket to the first socket's clock without skipping samples.
    let mut next = *epoch.lock().unwrap().get_or_insert_with(Instant::now);
    while !stop.load(Ordering::Relaxed) {
        let plan = shared.lock().unwrap().clone();
        if plan.as_ref().map(Arc::as_ptr) != current.as_ref().map(Arc::as_ptr) {
            decode = None;
            frame_number = 0;
            current = plan;
            if let Some(p) = &current {
                decode = Some(decoder(&p.list, video)?);
            }
        }
        let mut playing = false;
        data.fill(0);
        if let Some(p) = &current {
            if next >= p.start {
                playing = true;
                if decode
                    .as_mut()
                    .is_none_or(|d| d.0.stdout.as_mut().unwrap().read_exact(&mut data).is_err())
                {
                    data.fill(0);
                    if video {
                        let success = decode
                            .as_mut()
                            .and_then(|d| d.0.wait().ok())
                            .is_some_and(|s| s.success());
                        if !success {
                            *error.lock().unwrap() = Some(
                                "リプレイの読み込みが終了しました。ライブ映像へ戻しました".into(),
                            );
                        }
                        let mut active = shared.lock().unwrap();
                        if active.as_ref().is_some_and(|a| Arc::ptr_eq(a, p)) {
                            *active = None;
                        }
                        playing = false;
                    }
                }
            }
        }
        if video {
            if playing {
                if overlay.load(Ordering::Relaxed) {
                    if let Some(p) = &current {
                        crate::replay_overlay::paint(
                            &mut data,
                            frame_number as f64 / 30.,
                            p.ends.as_deref(),
                            p.count,
                        );
                    }
                }
                frame_number += 1;
            }
            write_interruptible(&mut socket, &data, stop).map_err(|e| e.to_string())?;
        } else {
            // Stereo replay followed by stereo live-gain. Both run on the same 48 kHz clock.
            let gain = if playing { 0f32 } else { 1f32 }.to_le_bytes();
            let mut output = Vec::with_capacity(1600 * 16);
            for sample in data.chunks_exact(8) {
                output.extend_from_slice(sample);
                output.extend_from_slice(&gain);
                output.extend_from_slice(&gain);
            }
            write_interruptible(&mut socket, &output, stop).map_err(|e| e.to_string())?;
        }
        next += Duration::from_nanos(1_000_000_000 / 30);
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }
    Ok(())
}

// A busy encoder/capture device may temporarily stop reading. Preserve the byte
// offset across timeouts; closing this continuous input would end live capture.
pub(crate) fn write_interruptible(
    socket: &mut impl Write,
    mut bytes: &[u8],
    stop: &AtomicBool,
) -> std::io::Result<()> {
    while !bytes.is_empty() && !stop.load(Ordering::Relaxed) {
        match socket.write(bytes) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(n) => bytes = &bytes[n..],
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    #[test]
    fn audio_transfer_keeps_every_pcm_byte_across_timeouts() {
        struct SlowEncoder {
            bytes: Vec<u8>,
            until: Instant,
            attempts: usize,
        }
        impl Write for SlowEncoder {
            fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
                self.attempts += 1;
                if self.bytes.len() >= 7 && Instant::now() < self.until {
                    thread::sleep(Duration::from_millis(25));
                    return Err(if self.attempts.is_multiple_of(2) {
                        std::io::ErrorKind::TimedOut
                    } else {
                        std::io::ErrorKind::WouldBlock
                    }
                    .into());
                }
                let count = data.len().min(if self.bytes.is_empty() { 7 } else { 257 });
                self.bytes.extend_from_slice(&data[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        // Deterministic partial writes + a two-second encoder stall; operating
        // systems may buffer whole loopback writes without actually timing out.
        let pcm: Vec<u8> = (0..200_000u32)
            .flat_map(|i| (i as f32 / 200_000.0).to_le_bytes())
            .collect();
        let expected = pcm.clone();
        let worker = thread::spawn(move || {
            let mut encoder = SlowEncoder {
                bytes: vec![],
                until: Instant::now() + Duration::from_secs(2),
                attempts: 0,
            };
            write_interruptible(&mut encoder, &pcm, &AtomicBool::new(false)).unwrap();
            encoder.bytes
        });
        thread::sleep(Duration::from_millis(1200));
        assert!(
            !worker.is_finished(),
            "a temporary audio stall must not end the input"
        );
        assert_eq!(worker.join().unwrap(), expected);
    }
    #[test]
    fn paused_consumer_does_not_disconnect_the_live_insert() {
        let insert = Insert::start().unwrap();
        let mut consumer = TcpStream::connect(("127.0.0.1", insert.video_port)).unwrap();
        consumer
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut frame = vec![0; (W * H * 4) as usize];
        consumer.read_exact(&mut frame).unwrap();
        thread::sleep(Duration::from_secs(3));
        for _ in 0..30 {
            consumer
                .read_exact(&mut frame)
                .expect("temporary backpressure must not close live input");
        }
        assert!(insert.error().is_none());
        // Drop must also interrupt a blocked writer without an encoder to drain it.
        let start = Instant::now();
        drop(insert);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
