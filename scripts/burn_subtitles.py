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

def emit_progress(pct: int) -> None:
    try:
        os.write(2, f"PROGRESS:{min(100, max(0, pct))}\n".encode("ascii"))
    except OSError:
        pass

def get_video_info(path):
    result = subprocess.run(
        [FFPROBE, "-v", "quiet", "-print_format", "json",
         "-show_streams", "-show_format", "-select_streams", "v:0", path],
        capture_output=True, text=True
    )
    data = json.loads(result.stdout)
    stream = data["streams"][0]
    w = stream["width"]
    h = stream["height"]
    num, den = map(int, stream["r_frame_rate"].split("/"))
    fps = num / den
    try:
        duration = float(stream["duration"])
    except (KeyError, ValueError):
        try:
            duration = float(data.get("format", {}).get("duration") or 0)
        except (ValueError, TypeError):
            duration = 0.0
    total_frames = int(duration * fps) if duration > 0 else 0
    return w, h, fps, total_frames

def get_text_at(t, entries):
    for entry in entries:
        if entry["start"] <= t <= entry["end"]:
            return entry["text"]
    return None

def _measure_text(draw, text, font) -> int:
    """Return rendered pixel width of text."""
    try:
        bbox = draw.textbbox((0, 0), text, font=font)
        return max(0, bbox[2] - bbox[0])
    except AttributeError:
        w, _ = draw.textsize(text, font=font)  # type: ignore[attr-defined]
        return w


def _split_word_by_chars(draw, word, font, max_width) -> list[str]:
    """Split a single word that exceeds max_width, breaking by character."""
    parts, current = [], ""
    for ch in word:
        test = current + ch
        if _measure_text(draw, test, font) <= max_width:
            current = test
        else:
            if current:
                parts.append(current)
            current = ch
    if current:
        parts.append(current)
    return parts or [word]


def wrap_text(text, font, max_width, draw):
    words = text.split()
    lines, current = [], []
    for word in words:
        test = " ".join(current + [word])
        if _measure_text(draw, test, font) <= max_width:
            current.append(word)
        else:
            if current:
                lines.append(" ".join(current))
            # Word itself might be wider than max_width — split by character
            if _measure_text(draw, word, font) > max_width:
                parts = _split_word_by_chars(draw, word, font, max_width)
                lines.extend(parts[:-1])
                current = [parts[-1]] if parts else []
            else:
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

    outline_w   = int(style.get("outlineWidth", 2))
    all_caps    = style.get("allCaps", False)

    # Apply allCaps BEFORE wrapping so measurement reflects actual rendered width.
    # Uppercase letters are ~15–25% wider; wrapping on lowercase then uppercasing
    # causes overflow on the right edge.
    effective_text = text.upper() if all_caps else text

    # margin: 10% each side (min 24px). Subtract stroke bleed (outline_w) from
    # each side so a line at max_w + stroke still stays within the margin zone.
    margin = max(int(w * 0.10), 24)
    max_w  = w - 2 * margin - 2 * outline_w

    lines = wrap_text(effective_text, font, max_w, draw)
    total_h = len(lines) * line_h

    pos = style.get("position", "bottom")
    if pos == "top":
        y = int(h * 0.08)
    elif pos == "center":
        y = (h - total_h) // 2
    else:
        y = h - int(h * 0.12) - total_h

    text_rgb    = hex_to_rgb(style.get("textColor",    "#ffffff"))
    outline_rgb = hex_to_rgb(style.get("outlineColor", "#000000"))
    box_enabled = style.get("boxEnabled", False)
    box_rgb     = hex_to_rgb(style.get("boxColor", "#000000"))
    box_alpha   = int(style.get("boxOpacity", 70) / 100 * 255)

    for line in lines:
        tw = _measure_text(draw, line, font)

        # Center the line, then clamp so neither text nor its stroke crosses margin.
        # stroke_width bleeds outline_w pixels beyond tw on each side, so offset by that.
        x = (w - tw) // 2
        x = max(margin + outline_w, min(x, w - margin - outline_w - tw))

        if box_enabled and box_alpha > 0:
            pad_x, pad_y = 12, 5
            bx1 = max(0, x - pad_x)
            bx2 = min(w, x + tw + pad_x)
            draw.rectangle([bx1, y - pad_y, bx2, y + line_h - 4], fill=(*box_rgb, box_alpha))

        draw.text(
            (x, y), line, font=font,
            fill=(*text_rgb, 255),
            stroke_width=outline_w,
            stroke_fill=(*outline_rgb, 255) if outline_w > 0 else None,
        )
        y += line_h

    composited = Image.alpha_composite(img.convert("RGBA"), overlay)
    return composited.convert("RGB")

def draw_title(img, text, font, style):
    """Draw a persistent title at the top of the frame."""
    w, h = img.size
    line_h = font.size + 8 if hasattr(font, 'size') else 40
    title_style = {
        "textColor": style.get("titleColor", "#ffffff"),
        "outlineColor": "#000000",
        "outlineWidth": 2,
        "allCaps": False,
        "boxEnabled": False,
        "position": "top",
    }
    return draw_subtitle(img, text, font, line_h, title_style)

def burn(input_path, entries, output_path, font_size=0, font_path=None, style=None,
         title="", title_font_size=0, title_color="#ffffff"):
    if style is None:
        style = {}
    w, h, fps, total_frames = get_video_info(input_path)
    actual_size = font_size if font_size > 0 else max(26, h // 22)
    font   = find_font(actual_size, font_path)
    line_h = actual_size + 8
    frame_bytes = w * h * 3
    # Emit roughly one progress update per percent (min every 10 frames)
    progress_interval = max(10, total_frames // 100) if total_frames > 0 else 30

    has_title = bool(title and title.strip())
    actual_title_size = title_font_size if title_font_size > 0 else max(32, h // 18)
    title_font = find_font(actual_title_size, font_path) if has_title else None
    title_style = {"titleColor": title_color}

    decode = subprocess.Popen(
        [FFMPEG, "-i", input_path, "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
    )
    encode = subprocess.Popen(
        [FFMPEG, "-y",
         "-f", "rawvideo", "-pix_fmt", "rgb24",
         "-s", f"{w}x{h}", "-r", f"{fps:.6f}", "-i", "pipe:0",
         "-i", input_path,
         "-map", "0:v", "-map", "1:a?",
         "-c:v", "libx264", "-preset", "fast", "-crf", "23",
         "-c:a", "copy", output_path],
        stdin=subprocess.PIPE, stderr=subprocess.DEVNULL
    )

    frame_num = 0
    emit_progress(0)
    try:
        while True:
            chunk = decode.stdout.read(frame_bytes)
            if len(chunk) < frame_bytes:
                break
            t    = frame_num / fps
            text = get_text_at(t, entries)
            needs_draw = text or has_title
            if needs_draw:
                img = Image.frombytes("RGB", (w, h), chunk)
                if text:
                    img = draw_subtitle(img, text, font, line_h, style)
                if has_title:
                    img = draw_title(img, title, title_font, title_style)
                encode.stdin.write(img.tobytes())
            else:
                encode.stdin.write(chunk)
            frame_num += 1
            if frame_num % progress_interval == 0:
                pct = min(99, int(frame_num / total_frames * 100)) if total_frames > 0 else 50
                emit_progress(pct)
    finally:
        decode.stdout.close()
        decode.wait()
        encode.stdin.close()
        encode.wait()

    emit_progress(100)
    return frame_num

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("entries_json")
    parser.add_argument("output")
    parser.add_argument("--font-size", type=int, default=0, help="0 = auto")
    parser.add_argument("--font", type=str, default="", help="Path to font file")
    parser.add_argument("--style", type=str, default="{}", help="JSON subtitle style options")
    parser.add_argument("--title", type=str, default="", help="Persistent title text at top")
    parser.add_argument("--title-font-size", type=int, default=0, help="0 = auto")
    parser.add_argument("--title-color", type=str, default="#ffffff", help="Title text color")
    args = parser.parse_args()

    if not os.path.exists(args.input):
        os.write(1, (json.dumps({"error": f"Input video tidak ditemukan: {args.input}"}) + "\n").encode("utf-8"))
        sys.exit(1)

    with open(args.entries_json) as f:
        entries = json.load(f)

    try:
        style = json.loads(args.style)
    except Exception:
        style = {}

    try:
        frames = burn(
            args.input, entries, args.output,
            font_size=args.font_size,
            font_path=args.font if args.font else None,
            style=style,
            title=args.title,
            title_font_size=args.title_font_size,
            title_color=args.title_color,
        )
        os.write(1, (json.dumps({"success": True, "frames": frames}) + "\n").encode("utf-8"))
    except Exception as e:
        import traceback
        os.write(1, (json.dumps({"error": str(e), "traceback": traceback.format_exc()}) + "\n").encode("utf-8"))
        sys.exit(1)
