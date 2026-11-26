use std::{
    net::SocketAddr,
    process::Stdio,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::Duration,
};

use crate::session::ffmpeg;

pub fn validate_url(url: &str) -> Result<(), &'static str> {
    let address = url
        .strip_prefix("rtmps://")
        .or_else(|| url.strip_prefix("rtmp://"));
    let has_destination = address
        .and_then(|address| address.split_once('/'))
        .is_some_and(|(host, path)| !host.is_empty() && !path.is_empty());
    if !has_destination || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Use an RTMP(S) URL with a host and a stream path");
    }
    Ok(())
}

pub fn relay(
    address: SocketAddr,
    destination: &str,
    kbps: u32,
    done: &AtomicBool,
    retries: &AtomicU64,
) {
    let fifo_packets = (kbps * 1000 / (8 * 188 * 2)).max(4096);
    let source =
        format!("udp://{address}?fifo_size={fifo_packets}&overrun_nonfatal=1&timeout=2500000");
    // A restarted relay can join just after an IDR. Read far enough to reach the next one at higher bitrates.
    let probe_bytes = (kbps * 400).max(4_194_304).to_string();
    while !done.load(Ordering::Relaxed) {
        let mut command = ffmpeg();
        command.args([
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-probesize",
            &probe_bytes,
            "-analyzeduration",
            "2500000",
            "-fflags",
            "+discardcorrupt",
            "-f",
            "mpegts",
            "-i",
            &source,
            "-c",
            "copy",
            "-avoid_negative_ts",
            "make_zero",
            "-rw_timeout",
            "3000000",
        ]);
        if destination.starts_with("rtmps://") {
            command.args(["-tls_verify", "1"]);
        }
        let sender = command
            .args([
                "-flush_packets",
                "1",
                "-flvflags",
                "no_duration_filesize",
                "-f",
                "flv",
                destination,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            // FFmpeg includes the destination URL and stream key in diagnostics.
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut sender) = sender else {
            eprintln!("Could not launch the RTMP sender. Recording continues.");
            return;
        };
        loop {
            if done.load(Ordering::Relaxed) {
                let _ = sender.kill();
                let _ = sender.wait();
                return;
            }
            match sender.try_wait() {
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Ok(Some(_)) => break,
                Err(_) => {
                    let _ = sender.kill();
                    let _ = sender.wait();
                    break;
                }
            }
        }
        retries.fetch_add(1, Ordering::Relaxed);
        eprintln!("RTMP output stopped; retrying. Recording continues.");
        thread::sleep(Duration::from_millis(500));
    }
}
