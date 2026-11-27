"""Exercise camera composition, end-of-camera recovery, and audio controls."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler, HTTPServer
import os
import re
import subprocess
import tempfile
import threading
import time

repo = Path(__file__).resolve().parents[1]
engine = repo / "target" / "debug" / ("stream-smash.exe" if os.name == "nt" else "stream-smash")

def run(*args):
    result = subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True, timeout=20)
    if result.returncode:
        raise AssertionError(result.stderr.decode(errors="replace"))
    return result.stdout

def fixture(path, color, duration, tone=False):
    audio = ["-f", "lavfi", "-i", "sine=frequency=440"] if tone else []
    run("ffmpeg", "-v", "error", "-f", "lavfi", "-i", f"color={color}:s=320x180:r=30",
        *audio, "-t", duration, "-c:v", "ffv1", str(path))

def pixel(path, time, x, y):
    return run("ffmpeg", "-v", "error", "-ss", time, "-i", str(path), "-an", "-vf",
               f"format=rgb24,crop=1:1:{x}:{y}", "-frames:v", "1", "-f", "rawvideo", "-")

with tempfile.TemporaryDirectory(prefix="smash camera ") as folder:
    folder = Path(folder)
    main, camera = folder / "main.mkv", folder / "camera.mkv"
    fixture(main, "blue", "2.5", tone=True)
    fixture(camera, "lime", "1.0")
    output = folder / "out"
    log = run(str(engine), "record", "--input", str(main), "--camera-file", str(camera),
              "--size", "320x180", "--fps", "30", "--audio-volume", "0", "--audio-meter",
              "--output-dir", str(output))
    result, = output.glob("*.mkv")
    early = pixel(result, "0.5", 280, 150)
    late = pixel(result, "1.9", 280, 150)
    background = pixel(result, "0.5", 10, 10)
    assert len(early) == len(late) == len(background) == 3
    assert early[1] > 220 and early[2] < 30, early
    assert min(late[2], background[2]) > 220, (late, background)
    samples = run("ffmpeg", "-v", "error", "-i", str(result), "-map", "0:a:0", "-f", "s16le", "-")
    assert samples and not any(samples), "The mute setting left audible samples"
    readings = re.findall(rb"Audio level: ([-\d.]+)", log)
    assert readings and set(readings) == {b"-60.0"}, log
    invalid = subprocess.run([str(engine), "record", "--test-pattern", "--camera-url",
                              "http://203.0.113.1/video"],
                             stdin=subprocess.DEVNULL, capture_output=True, timeout=5)
    assert invalid.returncode and b"private IPv4 camera URL" in invalid.stderr

    jpeg = run("ffmpeg", "-v", "error", "-i", str(camera), "-frames:v", "1", "-c:v", "mjpeg", "-f", "image2pipe", "-")

    class CameraFeed(BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(200)
            self.send_header("Content-Type", "multipart/x-mixed-replace; boundary=frame")
            self.end_headers()
            packet = (
                b"--frame\r\nContent-Type: image/jpeg\r\nContent-Length: "
                + str(len(jpeg)).encode() + b"\r\n\r\n" + jpeg + b"\r\n"
            )
            try:
                for _ in range(28):
                    self.wfile.write(packet)
                    time.sleep(.035)
            except (BrokenPipeError, ConnectionResetError):
                pass

    with HTTPServer(("127.0.0.1", 0), CameraFeed) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            network_output = folder / "network"
            url = f"http://127.0.0.1:{server.server_port}/video"
            run(str(engine), "record", "--input", str(main), "--camera-url", url,
                "--size", "320x180", "--fps", "30", "--output-dir", str(network_output))
            result, = network_output.glob("*.mkv")
            assert pixel(result, "0.5", 280, 150)[1] > 220
        finally:
            server.shutdown()
print("Inset composition, camera EOF recovery, mute and audio meter verified.")
