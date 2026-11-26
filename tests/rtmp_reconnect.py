import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import threading
import time

base = Path(__file__).resolve().parents[1]
exe = base / "target" / "debug" / ("stream-smash.exe" if os.name == "nt" else "stream-smash")
root = base / "target" / "verify-media" / str(time.time_ns())
root.mkdir(parents=True)
proxy = socket.create_server(("127.0.0.1", 0))
url = f"rtmp://127.0.0.1:{proxy.getsockname()[1]}/live/stream-smash-fixture"
upstream = None
active = []
proxy_lock = threading.Lock()

def pump(source, destination):
    try:
        while packet := source.recv(65_536):
            destination.sendall(packet)
    except OSError:
        pass
    finally:
        for conn in (source, destination):
            try:
                conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            conn.close()

def proxy_connections():
    while True:
        try:
            client, _ = proxy.accept()
        except OSError:
            break
        with proxy_lock:
            try:
                if upstream is None:
                    client.close()
                    continue
                target = socket.create_connection(upstream, timeout=1)
                target.settimeout(None)
            except OSError:
                client.close()
                continue
            active[:] = [client, target]
            threading.Thread(target=pump, args=(client, target), daemon=True).start()
            threading.Thread(target=pump, args=(target, client), daemon=True).start()

threading.Thread(target=proxy_connections, daemon=True).start()

def receiver(name):
    global upstream
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        address = probe.getsockname()
    listener_url = f"rtmp://127.0.0.1:{address[1]}/live/stream-smash-fixture"
    proc = subprocess.Popen([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-listen", "1", "-i", listener_url,
        "-c", "copy", "-flush_packets", "1", str(root / name)
    ], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    time.sleep(.4)
    with proxy_lock:
        upstream = address
    return proc

def wait_received(proc, name):
    path = root / name
    for _ in range(70):
        if path.is_file() and path.stat().st_size > 4000:
            return
        if proc.poll() is not None:
            raise RuntimeError(proc.stderr.read().decode(errors="replace"))
        time.sleep(.1)
    raise RuntimeError(f"{name} did not receive video")

def stop(proc):
    if proc.poll() is None:
        proc.kill()
    proc.communicate(timeout=5)

first = receiver("before-drop.flv")
second = None
sender = None
sender_log = (root / "sender.log").open("w+", encoding="utf-8")
encoder = sys.argv[1] if len(sys.argv) > 1 else "cpu"
size, fps, bitrate = ("1920x1080", "60", "17000") if "--hd" in sys.argv else ("320x180", "30", "6000")
expected_frames = int(fps) * 15
try:
    time.sleep(.4)
    sender = subprocess.Popen([
        str(exe), "record", "--test-pattern", "--size", size, "--fps", fps,
        "--encoder", encoder, "--bitrate", bitrate,
        "--seconds", "15", "--stream", "--highlights", "10", "--output-dir", str(root)
    ], env={**os.environ, "STREAM_SMASH_RTMP_URL": url},
       stdout=sender_log, stderr=subprocess.STDOUT, stdin=subprocess.PIPE, text=True)
    wait_received(first, "before-drop.flv")
    with proxy_lock:
        upstream = None
        for conn in active:
            try:
                conn.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            conn.close()
    stop(first)
    sender.stdin.write("h\n")
    sender.stdin.flush()
    time.sleep(1.5)
    second = receiver("after-drop.flv")
    wait_received(second, "after-drop.flv")
    sender.wait(timeout=25)
    sender_log.seek(0)
    log = sender_log.read()
    assert sender.returncode == 0 and url not in log, log
    assert "RTMP output stopped; retrying" in log, log
    assert "Highlight saved:" in log, log
    clip, = root.glob("*-Highlight-*.mkv")
    assert clip.stat().st_size > 4000
    recording, = (file for file in root.glob("*.mkv") if "-Highlight-" not in file.name)
    result = subprocess.check_output([
        "ffprobe", "-v", "error", "-count_frames", "-show_entries",
        "stream=codec_type,nb_read_frames:format=duration", "-of", "json", str(recording)
    ], text=True)
    info = json.loads(result)
    video, = [s for s in info["streams"] if s["codec_type"] == "video"]
    assert int(video["nb_read_frames"]) == expected_frames, info
    assert any(s["codec_type"] == "audio" for s in info["streams"]), info
    assert float(info["format"]["duration"]) >= 14.99, info
    print(f"RTMP reception before and after outage; recording preserved all {expected_frames} frames and audio ({info['format']['duration']} s)")
finally:
    proxy.close()
    stop(first)
    if second is not None:
        stop(second)
    if sender is not None:
        try:
            sender.wait(timeout=20)
        finally:
            if sys.exc_info()[0] is not None:
                sender_log.seek(0)
                print(sender_log.read())
            stop(sender)
    sender_log.close()
