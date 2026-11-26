from pathlib import Path
import json
import os
import struct
import subprocess
import tempfile
import zlib

repo = Path(__file__).resolve().parents[1]
engine = repo / "target" / "debug" / ("stream-smash.exe" if os.name == "nt" else "stream-smash")


def png_chunk(name, body):
    return struct.pack(">I", len(body)) + name + body + struct.pack(">I", zlib.crc32(name + body))


def pixel(path, x, y, seconds=0):
    return subprocess.check_output([
        "ffmpeg", "-v", "error", "-ss", str(seconds), "-i", str(path),
        "-vf", f"format=rgb24,crop=1:1:{x}:{y}", "-frames:v", "1",
        "-pix_fmt", "rgb24", "-f", "rawvideo", "-",
    ])


with tempfile.TemporaryDirectory(prefix="smash overlay ") as temp:
    root = Path(temp)
    overlay = root / "枠 overlay.png"
    bitmap = (b"\0" + bytes((255, 0, 0, 255)) * 16 + bytes((255, 0, 0, 0)) * 16) * 18
    overlay.write_bytes(
        b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 32, 18, 8, 6, 0, 0, 0))
        + png_chunk(b"IDAT", zlib.compress(bitmap)) + png_chunk(b"IEND", b"")
    )
    fixture = root / "base.mkv"
    subprocess.run([
        "ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=blue:size=320x180:rate=30",
        "-t", "1.5", "-c:v", "ffv1", str(fixture),
    ], check=True)
    image = root / "monitor.jpg"
    output = root / "recording"
    subprocess.run([
        str(engine), "record", "--input", str(fixture), "--output-dir", str(output),
        "--size", "320x180", "--fps", "30", "--overlay", str(overlay),
        "--overlay-opacity", "75", "--preview-image", str(image),
        "--preview-size", "160x90", "--preview-fps", "24", "--preview-quality", "95",
    ], stdin=subprocess.DEVNULL, check=True, timeout=20)
    recording, = output.glob("*.mkv")
    for media, width, height, seconds in [(image, 160, 90, 0), (recording, 320, 180, 1)]:
        r, g, b = pixel(media, width // 4, height // 2, seconds)
        assert 150 <= r <= 220 and g < 35 and 30 <= b <= 100, (media, r, g, b)
        r, g, b = pixel(media, width * 3 // 4, height // 2, seconds)
        assert r < 35 and g < 35 and b > 210, (media, r, g, b)
        info = json.loads(subprocess.check_output([
            "ffprobe", "-v", "error", "-select_streams", "v:0",
            "-show_entries", "stream=width,height", "-of", "json", str(media),
        ]))
        assert info["streams"][0] == {"width": width, "height": height}
    only = root / "preview-only.jpg"
    subprocess.run([
        str(engine), "preview", "--input", str(fixture), "--size", "320x180", "--fps", "30",
        "--overlay", str(overlay), "--preview-image", str(only),
    ], stdin=subprocess.DEVNULL, check=True, timeout=20)
    assert pixel(only, 80, 90)[0] > 220
print("Overlay transparency and opacity verified in preview-only, live preview, and recording.")
