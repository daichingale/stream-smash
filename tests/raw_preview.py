import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parents[1]
engine = repo / "target" / "debug" / ("stream-smash.exe" if os.name == "nt" else "stream-smash")
with tempfile.TemporaryDirectory(prefix="stream smash preview ") as directory:
    with socket.create_server(("127.0.0.1", 0)) as listener:
        root = Path(directory)
        with (root / "engine.log").open("wb") as log:
            process = subprocess.Popen([
                str(engine), "record", "--test-pattern", "--size", "1280x720", "--fps", "60",
                "--seconds", "9", "--preview-port", str(listener.getsockname()[1]),
                "--preview-size", "640x360", "--preview-fps", "60", "--output-dir", str(root),
            ], stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
            try:
                listener.settimeout(5)
                connection, _ = listener.accept()
                with connection, connection.makefile("rb") as frames:
                    connection.settimeout(3)
                    frame_bytes = 640 * 360 * 4
                    a = frames.read(frame_bytes)
                    b = frames.read(frame_bytes)
                    assert len(a) == len(b) == frame_bytes
                    assert a != b
                    for _ in range(150):
                        assert len(frames.read(frame_bytes)) == frame_bytes
                    started = time.monotonic()
                    for _ in range(120):
                        assert len(frames.read(frame_bytes)) == frame_bytes
                    fps = 120 / (time.monotonic() - started)
                    assert fps > 48, fps
                    time.sleep(1)
                # Close the consumer mid-recording. All recording frames must still arrive.
                assert process.wait(timeout=18) == 0
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
        recording, = root.glob("*.mkv")
        count = subprocess.check_output([
            "ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0",
            "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0", str(recording),
        ], text=True).strip()
        assert count == "540", (count, (root / "engine.log").read_text(errors="replace"))
        print(f"Received {fps:.1f} fps at 640x360; all {count} frames recorded after preview disconnect.")
