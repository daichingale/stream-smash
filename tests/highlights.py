import json
import os
from pathlib import Path
import subprocess
import time

repo = Path(__file__).resolve().parents[1]
engine = repo / "target" / "debug" / ("stream-smash.exe" if os.name == "nt" else "stream-smash")
output = repo / "target" / "verify-highlights" / str(time.time_ns())
output.mkdir(parents=True)
log = output / "session.log"


def probe(path):
    return json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-count_frames", "-show_entries",
        "format=duration:stream=codec_type,nb_read_frames", "-of", "json", str(path),
    ], text=True))


def wait_for(predicate, process, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        if process.poll() is not None:
            break
        time.sleep(0.1)
    raise RuntimeError(log.read_text(encoding="utf-8"))


with log.open("wb") as report:
    session = subprocess.Popen([
        str(engine), "record", "--test-pattern", "--size", "320x180", "--fps", "30",
        "--highlights", "10", "--seconds", "28", "--output-dir", str(output),
    ], stdin=subprocess.PIPE, stdout=report, stderr=subprocess.STDOUT)
    try:
        wait_for(lambda: (
            any(output.glob(".*/part-00000006.mkv"))
            and not any(output.glob(".*/part-00000000.mkv"))
        ), session, timeout=18)
        session.stdin.write(b"h\n")
        session.stdin.flush()
        wait_for(lambda: "Highlight saved:" in log.read_text(encoding="utf-8"), session)
        assert session.poll() is None
        clip, = output.glob("*-Highlight-*.mkv")
        clip_info = probe(clip)
        video, = (stream for stream in clip_info["streams"] if stream["codec_type"] == "video")
        assert int(video["nb_read_frames"]) == 300, clip_info
        assert any(stream["codec_type"] == "audio" for stream in clip_info["streams"]), clip_info
        assert 9.9 <= float(clip_info["format"]["duration"]) <= 10.4, clip_info
    finally:
        if session.poll() is None:
            session.stdin.write(b"q\n")
            session.stdin.flush()
        assert session.wait(timeout=12) == 0, log.read_text(encoding="utf-8")

recording, = (file for file in output.glob("*.mkv") if "-Highlight-" not in file.name)
assert float(probe(recording)["format"]["duration"]) > 10
assert not list(output.glob(".*-replay"))
print("Saved a 10-second, 300-frame highlight with audio while recording, and cleared the rolling buffer at stop.")
