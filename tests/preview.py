from pathlib import Path
import os
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parents[1]
engine = root / "target" / "debug" / ("stream-smash.exe" if os.name == "nt" else "stream-smash")
with tempfile.TemporaryDirectory(prefix="stream smash preview ") as temp:
    folder = Path(temp)
    frame = folder / "live frame.jpg"
    logs = folder / "preview.log"
    with logs.open("wb") as log:
        process = subprocess.Popen([
            str(engine), "preview", "--test-pattern", "--size", "1280x720",
            "--fps", "30", "--seconds", "10", "--output-dir", str(folder / "recording"),
            "--preview-image", str(frame),
        ], stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT)
        try:
            images = set()
            deadline = time.monotonic() + 5
            while len(images) < 3 and time.monotonic() < deadline and process.poll() is None:
                try:
                    data = frame.read_bytes()
                except OSError:
                    time.sleep(0.05)
                    continue
                assert data.startswith(b"\xff\xd8") and data.endswith(b"\xff\xd9")
                images.add(data)
                time.sleep(0.05)
            assert len(images) >= 3, logs.read_text(errors="replace")
        finally:
            if process.poll() is None:
                process.stdin.write(b"q\n")
                process.stdin.flush()
            assert process.wait(timeout=15) == 0, logs.read_text(errors="replace")
    assert not (folder / "recording").exists()
    dimensions = subprocess.check_output([
        "ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height",
        "-of", "csv=s=x:p=0", str(frame),
    ], text=True).strip()
    assert dimensions == "640x360", dimensions
print("Preview-only displayed changing 640x360 images, accepted stop, and created no recording.")
