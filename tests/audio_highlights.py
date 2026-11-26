import json
import os
from pathlib import Path
import subprocess
import time

repo = Path(__file__).resolve().parents[1]
engine = repo / "target" / "debug" / ("stream-smash.exe" if os.name == "nt" else "stream-smash")
work = repo / "target" / "verify-audio-highlights" / str(time.time_ns())
work.mkdir(parents=True)
fixture = work / "audio-with-two-peaks.mkv"
subprocess.run([
    "ffmpeg", "-hide_banner", "-loglevel", "error",
    "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30",
    "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
    "-af", "volume='if(between(t,3.5,4.8)+between(t,7,8),3,0.04)':eval=frame",
    "-t", "9", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "pcm_s16le", str(fixture),
], check=True)

output = work / "capture"
result = subprocess.run([
    str(engine), "record", "--input", str(fixture), "--size", "320x180", "--fps", "30",
    "--highlights", "10", "--audio-peaks", "--output-dir", str(output),
], stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=25, check=True)
assert result.stdout.count("Audio highlight saved: ") == 1, result
assert "lavfi.astats." not in result.stderr, result
clip, = output.glob("*-Highlight-*-Audio.mkv")
clip_info = json.loads(subprocess.check_output([
    "ffprobe", "-v", "error", "-show_entries", "format=duration:stream=codec_type",
    "-of", "json", str(clip),
], text=True))
assert {stream["codec_type"] for stream in clip_info["streams"]} == {"audio", "video"}, clip_info
assert float(clip_info["format"]["duration"]) >= 4, clip_info
print("Automatically saved the first audio rise and suppressed a second peak during the cooldown.")
