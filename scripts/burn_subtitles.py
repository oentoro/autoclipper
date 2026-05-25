#!/usr/bin/env python3
"""
burn_subtitles.py <input_video> <entries_json_path> <output_video>
                  [--font-size N] [--font /path/to/font.ttf]
                  [--style '{"textColor":"#fff","outlineColor":"#000",...}']
"""
import sys, json, subprocess, os, argparse
from PIL import Image, ImageDraw, ImageFont
import platform as _platform

def _find_bin(name):
    _system = _platform.system()
    if _system == "Windows":
        candidates = [
            f"C:\\ffmpeg\\bin\\{name}.exe",
            f"C:\\ProgramData\\chocolatey\\bin\\{name}.exe",
        ]
    elif _system == "Darwin":
        candidates = [f"/opt/homebrew/bin/{name}", f"/usr/local/bin/{name}", f"/usr/bin/{name}"]
    else:
        candidates = [f"/usr/bin/{name}", f"/usr/local/bin/{name}"]
    for p in candidates:
        if os.path.exists(p):
            return p
    return f"{name}.exe" if _system == "Windows" else name

FFMPEG  = os.environ.get("AUTOCLIPPER_FFMPEG")  or _find_bin("ffmpeg")
FFPROBE = os.environ.get("AUTOCLIPPER_FFPROBE") or _find_bin("ffprobe")

_system = _platform.system()
if _system == "Darwin":
    FONT_CANDIDATES = [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
    ]
elif _system == "Windows":
    FONT_CANDIDATES = [
        "C:/Windows/Fonts/arialbd.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/calibrib.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
    ]
else:
    FONT_CANDIDATES = [
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSansBold.ttf",
    ]

def find_font(size, preferred_path=None):
    if preferred_path and os.path.exists(preferred_path):
        try:
            return ImageFont.truetype(preferred_path, size)
        except Exception:
            pass
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

def hex_to_rgb(hex_color):
    hex_color = hex_color.lstrip("#")
    return (int(hex_color[0:2], 16), int(hex_color[2:4], 16), int(hex_color[4:6], 16))

def draw_subtitle(img, text, font, line_h, style):
    w, h = img.size

    # Transparent overlay so box can have alpha
    overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)

    max_w = int(w * 0.88)
    lines = wrap_text(text, font, max_w, draw)
    total_h = len(lines) * line_h

    pos = style.get("position", "bottom")
    if pos == "top":
        y = int(h * 0.06)
    elif pos == "center":
        y = (h - total_h) // 2
    else:
        y = h - int(h * 0.10) - total_h

    text_rgb    = hex_to_rgb(style.get("textColor",    "#ffffff"))
    outline_rgb = hex_to_rgb(style.get("outlineColor", "#000000"))
    outline_w   = int(style.get("outlineWidth", 2))
    box_enabled = style.get("boxEnabled", False)
    box_rgb     = hex_to_rgb(style.get("boxColor", "#000000"))
    box_alpha   = int(style.get("boxOpacity", 70) / 100 * 255)
    all_caps    = style.get("allCaps", False)

    for line in lines:
        if all_caps:
            line = line.upper()
        tw = int(draw.textlength(line, font=font))
        x = (w - tw) // 2

        if box_enabled and box_alpha > 0:
            pad_x, pad_y = 14, 5
            draw.rectangle(
                [x - pad_x, y - pad_y, x + tw + pad_x, y + line_h - 4],
                fill=(*box_rgb, box_alpha),
            )

        draw.text(
            (x, y), line, font=font,
            fill=(*text_rgb, 255),
            stroke_width=outline_w,
            stroke_fill=(*outline_rgb, 255) if outline_w > 0 else None,
        )
        y += line_h

    composited = Image.alpha_composite(img.convert("RGBA"), overlay)
    return composited.convert("RGB")

def burn(input_path, entries, output_path, font_size=0, font_path=None, style=None):
    if style is None:
        style = {}
    w, h, fps = get_video_info(input_path)
    actual_size = font_size if font_size > 0 else max(26, h // 22)
    font   = find_font(actual_size, font_path)
    line_h = actual_size + 8
    frame_bytes = w * h * 3

    decode = subprocess.Popen(
        [FFMPEG, "-i", input_path, "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
    )
    encode = subprocess.Popen(
        [FFMPEG, "-y",
         "-f", "rawvideo", "-pix_fmt", "rgb24",
         "-s", f"{w}x{h}", "-r", f"{fps:.6f}", "-i", "pipe:0",
         "-i", input_path,
         "-map", "0:v", "-map", "1:a",
         "-c:v", "libx264", "-preset", "fast", "-crf", "23",
         "-c:a", "copy", output_path],
        stdin=subprocess.PIPE, stderr=subprocess.DEVNULL
    )

    frame_num = 0
    try:
        while True:
            chunk = decode.stdout.read(frame_bytes)
            if len(chunk) < frame_bytes:
                break
            t    = frame_num / fps
            text = get_text_at(t, entries)
            if text:
                img = Image.frombytes("RGB", (w, h), chunk)
                img = draw_subtitle(img, text, font, line_h, style)
                encode.stdin.write(img.tobytes())
            else:
                encode.stdin.write(chunk)
            frame_num += 1
    finally:
        decode.stdout.close()
        decode.wait()
        encode.stdin.close()
        encode.wait()

    return frame_num

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("entries_json")
    parser.add_argument("output")
    parser.add_argument("--font-size", type=int, default=0, help="0 = auto")
    parser.add_argument("--font", type=str, default="", help="Path to font file")
    parser.add_argument("--style", type=str, default="{}", help="JSON subtitle style options")
    args = parser.parse_args()

    if not os.path.exists(args.input):
        print(json.dumps({"error": f"Input video tidak ditemukan: {args.input}"}))
        sys.exit(1)

    with open(args.entries_json) as f:
        entries = json.load(f)

    try:
        style = json.loads(args.style)
    except Exception:
        style = {}

    frames = burn(
        args.input, entries, args.output,
        font_size=args.font_size,
        font_path=args.font if args.font else None,
        style=style,
    )
    print(json.dumps({"success": True, "frames": frames}))
