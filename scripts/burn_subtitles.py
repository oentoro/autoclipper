#!/usr/bin/env python3
"""
burn_subtitles.py <input_video> <entries_json_path> <output_video>

Membakar subtitle ke frame video menggunakan PIL + FFmpeg pipe.
entries_json: [{"start": 0.0, "end": 2.5, "text": "..."}, ...]
"""
import sys, json, subprocess, os
from PIL import Image, ImageDraw, ImageFont

def _find_bin(name):
    for p in [f"/opt/homebrew/bin/{name}", f"/usr/local/bin/{name}", f"/usr/bin/{name}"]:
        if os.path.exists(p):
            return p
    return name

FFMPEG = os.environ.get("AUTOCLIPPER_FFMPEG") or _find_bin("ffmpeg")
FFPROBE = os.environ.get("AUTOCLIPPER_FFPROBE") or _find_bin("ffprobe")

FONT_CANDIDATES = [
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/Geneva.ttf",
]

def find_font(size):
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                continue
    return ImageFont.load_default()

def get_video_info(path):
    result = subprocess.run(
        [FFPROBE, "-v", "quiet", "-print_format", "json",
         "-show_streams", "-select_streams", "v:0", path],
        capture_output=True, text=True
    )
    stream = json.loads(result.stdout)["streams"][0]
    w = stream["width"]
    h = stream["height"]
    num, den = map(int, stream["r_frame_rate"].split("/"))
    fps = num / den
    return w, h, fps

def get_text_at(t, entries):
    for entry in entries:
        if entry["start"] <= t <= entry["end"]:
            return entry["text"]
    return None

def wrap_text(text, font, max_width, draw):
    words = text.split()
    lines, current = [], []
    for word in words:
        test = " ".join(current + [word])
        w = draw.textlength(test, font=font)
        if w <= max_width:
            current.append(word)
        else:
            if current:
                lines.append(" ".join(current))
            current = [word]
    if current:
        lines.append(" ".join(current))
    return lines or [""]

def draw_subtitle(img, text, font, line_h):
    draw = ImageDraw.Draw(img)
    w, h = img.size
    max_w = int(w * 0.88)
    lines = wrap_text(text, font, max_w, draw)
    total_h = len(lines) * line_h
    y = h - int(h * 0.10) - total_h

    for line in lines:
        tw = draw.textlength(line, font=font)
        x = int((w - tw) / 2)
        # Shadow/outline: draw 8 directions
        for dx, dy in [(-2,-2),(-2,0),(-2,2),(0,-2),(0,2),(2,-2),(2,0),(2,2)]:
            draw.text((x + dx, y + dy), line, font=font, fill=(0, 0, 0, 220))
        draw.text((x, y), line, font=font, fill=(255, 255, 255, 255))
        y += line_h

def burn(input_path, entries, output_path):
    w, h, fps = get_video_info(input_path)
    font_size = max(26, h // 22)
    font = find_font(font_size)
    line_h = font_size + 8
    frame_bytes = w * h * 3

    decode = subprocess.Popen(
        [FFMPEG, "-i", input_path,
         "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
    )

    encode = subprocess.Popen(
        [FFMPEG, "-y",
         "-f", "rawvideo", "-pix_fmt", "rgb24",
         "-s", f"{w}x{h}", "-r", f"{fps:.6f}", "-i", "pipe:0",
         "-i", input_path,
         "-map", "0:v", "-map", "1:a",
         "-c:v", "libx264", "-preset", "fast", "-crf", "23",
         "-c:a", "copy",
         output_path],
        stdin=subprocess.PIPE, stderr=subprocess.DEVNULL
    )

    frame_num = 0
    prev_text = None
    prev_frame_bytes = None  # cache for repeated identical subtitle frames

    try:
        while True:
            chunk = decode.stdout.read(frame_bytes)
            if len(chunk) < frame_bytes:
                break

            t = frame_num / fps
            text = get_text_at(t, entries)

            if text:
                if text == prev_text and prev_frame_bytes:
                    # Same subtitle as previous frame, just update the raw frame with cached overlay position
                    # Can't reuse bytes directly since video content changes, so redraw
                    img = Image.frombytes("RGB", (w, h), chunk)
                    draw_subtitle(img, text, font, line_h)
                    out_bytes = img.tobytes()
                else:
                    img = Image.frombytes("RGB", (w, h), chunk)
                    draw_subtitle(img, text, font, line_h)
                    out_bytes = img.tobytes()
                prev_text = text
                encode.stdin.write(out_bytes)
            else:
                prev_text = None
                encode.stdin.write(chunk)

            frame_num += 1

    finally:
        decode.stdout.close()
        decode.wait()
        encode.stdin.close()
        encode.wait()

    return frame_num

if __name__ == "__main__":
    if len(sys.argv) != 4:
        print(json.dumps({"error": "Usage: burn_subtitles.py <input> <entries_json_file> <output>"}))
        sys.exit(1)

    input_path, entries_file, output_path = sys.argv[1], sys.argv[2], sys.argv[3]

    if not os.path.exists(input_path):
        print(json.dumps({"error": f"Input video tidak ditemukan: {input_path}"}))
        sys.exit(1)

    with open(entries_file) as f:
        entries = json.load(f)

    frames = burn(input_path, entries, output_path)
    print(json.dumps({"success": True, "frames": frames}))
