#!/usr/bin/env python3
"""
burn_subtitles.py <input_video> <entries_json_path> <output_video>
                  [--font-size N] [--font /path/to/font.ttf]
                  [--style '{"textColor":"#fff","outlineColor":"#000",...}']
"""
import sys, json, subprocess, os, argparse, bisect, tempfile, shutil, threading
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


def _bitrate_for(w: int, h: int) -> str:
    """Target bitrate heuristic by pixel count (resolution-independent of
    orientation) — starting point tuned to roughly match libx264 crf 23
    output size; revisit if visual QA finds it too aggressive either way."""
    pixels = w * h
    if pixels <= 1280 * 720:
        return "5M"
    elif pixels <= 1920 * 1080:
        return "8M"
    else:
        return "16M"


def _pick_encoder() -> tuple:
    """
    Return (encoder_name, is_hardware). Checks ffmpeg's compiled encoder
    list once — this confirms the encoder was BUILT into this ffmpeg, not
    that compatible hardware is actually present. Runtime failure (hardware
    missing) is handled by the caller via a libx264 retry.
    """
    system = _platform.system()
    if system == "Darwin":
        candidates = ["h264_videotoolbox"]
    elif system == "Windows":
        candidates = ["h264_nvenc", "h264_qsv", "h264_amf"]
    else:
        candidates = ["h264_nvenc", "h264_vaapi"]

    try:
        result = subprocess.run([FFMPEG, "-hide_banner", "-encoders"],
                                 capture_output=True, text=True, timeout=10)
        available = result.stdout
    except Exception:
        available = ""

    for name in candidates:
        if name in available:
            return name, True
    return "libx264", False

_system = _platform.system()
if _system == "Darwin":
    FONT_CANDIDATES = [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
    ]
    # CJK fonts on macOS.
    # AppleSDGothicNeo covers Korean + Japanese + Chinese → listed first.
    # Hiragino covers Japanese + Chinese but NOT Korean.
    # STHeiti covers Chinese + Japanese but NOT Korean.
    FONT_CANDIDATES_CJK = [
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",      # KO ✓ JA ✓ ZH ✓
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", # KO ✗ JA ✓ ZH ✓
        "/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",         # KO ✗ JA ✓ ZH ✓
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/Library/Fonts/NanumGothic.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    ]
elif _system == "Windows":
    FONT_CANDIDATES = [
        "C:/Windows/Fonts/arialbd.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/calibrib.ttf",
        "C:/Windows/Fonts/segoeui.ttf",
    ]
    FONT_CANDIDATES_CJK = [
        # Japanese
        "C:/Windows/Fonts/meiryo.ttc",
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/msmincho.ttc",
        # Chinese Simplified
        "C:/Windows/Fonts/msyh.ttc",       # Microsoft YaHei
        "C:/Windows/Fonts/simsun.ttc",
        "C:/Windows/Fonts/simhei.ttf",
        # Chinese Traditional
        "C:/Windows/Fonts/msjh.ttc",       # Microsoft JhengHei
        # Korean
        "C:/Windows/Fonts/malgun.ttf",
        "C:/Windows/Fonts/malgunbd.ttf",
        "C:/Windows/Fonts/gulim.ttc",
    ]
else:
    FONT_CANDIDATES = [
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
        "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/freefont/FreeSansBold.ttf",
    ]
    FONT_CANDIDATES_CJK = [
        # Noto CJK — covers Japanese, Chinese, Korean (install: apt install fonts-noto-cjk)
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/opentype/noto/NotoSansCJKkr-Regular.otf",
        "/usr/share/fonts/noto-cjk/NotoSansCJKkr-Regular.otf",
        # Korean — Nanum
        "/usr/share/fonts/truetype/nanum/NanumGothicBold.ttf",
        "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
        "/usr/share/fonts/truetype/unfonts-core/UnDotum.ttf",
    ]


def _has_cjk(text: str) -> bool:
    """Return True if text contains Hangul, CJK, or kana characters."""
    for ch in text:
        cp = ord(ch)
        if (0x1100 <= cp <= 0x11FF    # Hangul Jamo
                or 0x3130 <= cp <= 0x318F  # Hangul Compat Jamo
                or 0xAC00 <= cp <= 0xD7AF  # Hangul Syllables
                or 0x4E00 <= cp <= 0x9FFF  # CJK Unified
                or 0x3040 <= cp <= 0x30FF  # Hiragana + Katakana
                or 0x3400 <= cp <= 0x4DBF  # CJK Ext-A
                or 0x20000 <= cp <= 0x2A6DF):  # CJK Ext-B
            return True
    return False


def _cjk_test_pair(text: str) -> tuple:
    """Return two distinct test chars matching the CJK script found in text.

    Used to detect .notdef boxes: fonts without a script render all missing
    codepoints as an identical placeholder glyph.
    """
    for ch in text:
        cp = ord(ch)
        if 0xAC00 <= cp <= 0xD7AF or 0x1100 <= cp <= 0x11FF or 0x3130 <= cp <= 0x318F:
            return '가', '나'   # Hangul
        if 0x3040 <= cp <= 0x309F:
            return 'あ', 'い'   # Hiragana
        if 0x30A0 <= cp <= 0x30FF:
            return 'ア', 'イ'   # Katakana
        if 0x4E00 <= cp <= 0x9FFF or 0x3400 <= cp <= 0x4DBF:
            return '中', '文'   # CJK Unified
    return '가', '나'


def _font_can_render_cjk(font, sample_pair: tuple = ('가', '나')) -> bool:
    """Check font has real CJK glyphs by comparing two distinct characters.

    .notdef boxes produced for missing codepoints are pixel-identical,
    regardless of which character is requested. If both chars render the
    same pixels the font lacks those glyphs.
    """
    try:
        c1, c2 = sample_pair
        test_path = getattr(font, 'path', None)
        size = 30

        def _render(ch):
            img = Image.new("L", (size * 2, size * 2), 0)
            f = ImageFont.truetype(test_path, size) if test_path else font
            ImageDraw.Draw(img).text((2, 2), ch, font=f, fill=255)
            return bytes(img.tobytes())

        r1, r2 = _render(c1), _render(c2)
        if r1 == r2:
            return False   # identical → .notdef for both
        # Sanity: at least 1% lit pixels so two different blanks don't pass
        lit = sum(1 for b in r1 if b > 20)
        return lit > (size * 2) ** 2 * 0.01
    except Exception:
        return False


def find_font(size, preferred_path=None, text_hint: str = ""):
    need_cjk = _has_cjk(text_hint)
    cjk_pair = _cjk_test_pair(text_hint) if need_cjk else ('가', '나')

    def _try(path: str):
        if not os.path.exists(path):
            return None
        try:
            f = ImageFont.truetype(path, size)
            if need_cjk and not _font_can_render_cjk(f, cjk_pair):
                return None
            return f
        except Exception:
            return None

    # Preferred font — skip if it can't render CJK when needed
    if preferred_path:
        f = _try(preferred_path)
        if f:
            print(f"[burn] font: {preferred_path}", file=sys.stderr)
            return f
        if need_cjk:
            print(f"[burn] '{os.path.basename(preferred_path)}' tidak support CJK, cari font alternatif",
                  file=sys.stderr)

    candidates = (FONT_CANDIDATES_CJK + FONT_CANDIDATES) if need_cjk else FONT_CANDIDATES
    for path in candidates:
        f = _try(path)
        if f:
            print(f"[burn] font: {path}", file=sys.stderr)
            return f

    # Last resort without CJK verification (something is better than .notdef boxes)
    for path in (FONT_CANDIDATES_CJK + FONT_CANDIDATES):
        if os.path.exists(path):
            try:
                f = ImageFont.truetype(path, size)
                print(f"[burn] font (no-verify fallback): {path}", file=sys.stderr)
                return f
            except Exception:
                continue

    print("[burn] font: PIL default", file=sys.stderr)
    return ImageFont.load_default()

def emit_progress(pct: int) -> None:
    try:
        os.write(2, f"PROGRESS:{min(100, max(0, pct))}\n".encode("ascii"))
    except OSError:
        pass

def emit_status(msg: str) -> None:
    try:
        os.write(2, (msg + "\n").encode("utf-8", errors="replace"))
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

def get_text_at(t, entries, _starts):
    if not entries:
        return None
    i = bisect.bisect_right(_starts, t) - 1
    if 0 <= i < len(entries) and entries[i]["end"] >= t:
        return entries[i]["text"]
    return None


def _compute_subtitle_layout(text, font, line_h, style, w, h):
    """Compute wrapped lines and drawing positions. Pure geometry — cacheable by text."""
    _tmp = ImageDraw.Draw(Image.new("RGBA", (2, 2)))
    outline_w = int(style.get("outlineWidth", 2))
    effective_text = text.upper() if style.get("allCaps", False) else text
    margin = max(int(w * 0.10), 24)
    max_w = w - 2 * margin - 2 * outline_w
    lines = wrap_text(effective_text, font, max_w, _tmp)
    total_h = len(lines) * line_h
    pos = style.get("position", "bottom")
    if pos == "top":
        y = int(h * 0.08)
    elif pos == "center":
        y = (h - total_h) // 2
    else:
        y = h - int(h * 0.12) - total_h
    result = []
    for line in lines:
        tw = _measure_text(_tmp, line, font)
        x = (w - tw) // 2
        x = max(margin + outline_w, min(x, w - margin - outline_w - tw))
        result.append((line, x, y, tw))
        y += line_h
    return result


def _build_subtitle_overlay(text, font, line_h, style, w, h):
    """Build RGBA overlay for box-enabled subtitles. Result is cacheable."""
    overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    outline_w = int(style.get("outlineWidth", 2))
    effective_text = text.upper() if style.get("allCaps", False) else text
    margin = max(int(w * 0.10), 24)
    max_w = w - 2 * margin - 2 * outline_w
    lines = wrap_text(effective_text, font, max_w, draw)
    total_h = len(lines) * line_h
    pos = style.get("position", "bottom")
    if pos == "top":
        y = int(h * 0.08)
    elif pos == "center":
        y = (h - total_h) // 2
    else:
        y = h - int(h * 0.12) - total_h
    text_rgb = hex_to_rgb(style.get("textColor", "#ffffff"))
    outline_rgb = hex_to_rgb(style.get("outlineColor", "#000000"))
    box_rgb = hex_to_rgb(style.get("boxColor", "#000000"))
    box_alpha = int(style.get("boxOpacity", 70) / 100 * 255)
    for line in lines:
        tw = _measure_text(draw, line, font)
        x = (w - tw) // 2
        x = max(margin + outline_w, min(x, w - margin - outline_w - tw))
        pad_x, pad_y = 12, 5
        draw.rectangle([max(0, x - pad_x), y - pad_y, min(w, x + tw + pad_x), y + line_h - 4],
                       fill=(*box_rgb, box_alpha))
        draw.text((x, y), line, font=font, fill=(*text_rgb, 255),
                  stroke_width=outline_w,
                  stroke_fill=(*outline_rgb, 255) if outline_w > 0 else None)
        y += line_h
    return overlay

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


def _wrap_paragraph(text, font, max_width, draw):
    """Word-wrap a single paragraph (no newlines) to fit max_width."""
    words = text.split()
    lines, current = [], []
    for word in words:
        test = " ".join(current + [word])
        if _measure_text(draw, test, font) <= max_width:
            current.append(word)
        else:
            if current:
                lines.append(" ".join(current))
            if _measure_text(draw, word, font) > max_width:
                parts = _split_word_by_chars(draw, word, font, max_width)
                lines.extend(parts[:-1])
                current = [parts[-1]] if parts else []
            else:
                current = [word]
    if current:
        lines.append(" ".join(current))
    return lines or [""]


def wrap_text(text, font, max_width, draw):
    """Wrap text preserving explicit \\n line breaks (used for bilingual subtitles)."""
    result = []
    for paragraph in text.split("\n"):
        result.extend(_wrap_paragraph(paragraph, font, max_width, draw))
    return result or [""]

def hex_to_rgb(hex_color):
    hex_color = hex_color.lstrip("#")
    return (int(hex_color[0:2], 16), int(hex_color[2:4], 16), int(hex_color[4:6], 16))

def draw_subtitle(img, text, font, line_h, style):
    w, h = img.size
    box_enabled = style.get("boxEnabled", False)
    box_alpha = int(style.get("boxOpacity", 70) / 100 * 255)
    if box_enabled and box_alpha > 0:
        overlay = _build_subtitle_overlay(text, font, line_h, style, w, h)
        return Image.alpha_composite(img.convert("RGBA"), overlay).convert("RGB")
    else:
        layout = _compute_subtitle_layout(text, font, line_h, style, w, h)
        draw = ImageDraw.Draw(img)
        outline_w = int(style.get("outlineWidth", 2))
        text_rgb = hex_to_rgb(style.get("textColor", "#ffffff"))
        outline_rgb = hex_to_rgb(style.get("outlineColor", "#000000"))
        for line, x, y, _ in layout:
            draw.text((x, y), line, font=font, fill=text_rgb,
                      stroke_width=outline_w,
                      stroke_fill=outline_rgb if outline_w > 0 else None)
        return img

def render_overlay_image(text, font, line_h, style, w, h):
    """
    Render subtitle text onto a transparent RGBA canvas (w x h), reusing the
    exact same layout/box/stroke logic as draw_subtitle — but always
    targeting a transparent canvas instead of compositing onto a video
    frame. Used to pre-render PNG overlays for the native ffmpeg path
    (burn_native), so CJK/word-wrap/box-style behavior never diverges from
    the per-frame path.
    """
    box_enabled = style.get("boxEnabled", False)
    box_alpha = int(style.get("boxOpacity", 70) / 100 * 255)
    if box_enabled and box_alpha > 0:
        return _build_subtitle_overlay(text, font, line_h, style, w, h)

    overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw = ImageDraw.Draw(overlay)
    outline_w = int(style.get("outlineWidth", 2))
    text_rgb = hex_to_rgb(style.get("textColor", "#ffffff"))
    outline_rgb = hex_to_rgb(style.get("outlineColor", "#000000"))
    layout = _compute_subtitle_layout(text, font, line_h, style, w, h)
    for line, x, y, _ in layout:
        draw.text((x, y), line, font=font, fill=(*text_rgb, 255),
                   stroke_width=outline_w,
                   stroke_fill=(*outline_rgb, 255) if outline_w > 0 else None)
    return overlay

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

    # ── Diagnostic: print first 10 entries to stderr ──────────────────────
    if entries:
        import sys as _sys
        n_show = min(10, len(entries))
        _sys.stderr.write(f"[burn] total entries: {len(entries)}, showing first {n_show}:\n")
        for i, e in enumerate(entries[:n_show]):
            _sys.stderr.write(f"  [{i}] start={e['start']:.3f} end={e['end']:.3f} text={e['text'][:80]}\n")
        _sys.stderr.flush()

    w, h, fps, total_frames = get_video_info(input_path)
    actual_size = font_size if font_size > 0 else max(26, h // 22)

    # Scan all subtitle text + title to detect CJK/Hangul — pick font accordingly
    all_text = " ".join(e.get("text", "") for e in entries) + " " + title
    font   = find_font(actual_size, font_path, text_hint=all_text)
    line_h = actual_size + 8
    frame_bytes = w * h * 3
    # Emit roughly one progress update per percent (min every 10 frames)
    progress_interval = max(10, total_frames // 100) if total_frames > 0 else 30

    has_title = bool(title and title.strip())
    actual_title_size = title_font_size if title_font_size > 0 else max(32, h // 18)
    title_font = find_font(actual_title_size, font_path, text_hint=title) if has_title else None
    title_style = {"titleColor": title_color}

    # Precompute subtitle lookup structures
    _starts = [e["start"] for e in entries]
    _box_enabled = bool(style.get("boxEnabled", False))
    _box_alpha = int(style.get("boxOpacity", 70) / 100 * 255)
    _use_alpha_path = _box_enabled and _box_alpha > 0
    _outline_w = int(style.get("outlineWidth", 2))
    _text_rgb = hex_to_rgb(style.get("textColor", "#ffffff"))
    _outline_rgb = hex_to_rgb(style.get("outlineColor", "#000000"))
    _box_rgb = hex_to_rgb(style.get("boxColor", "#000000"))
    _subtitle_cache: dict = {}

    # Precompute title layout once — title text never changes between frames
    _title_layout = None
    _title_text_rgb = hex_to_rgb(title_color)
    if has_title:
        _t_line_h = title_font.size + 8 if hasattr(title_font, 'size') else 40
        _t_style = {"textColor": title_color, "outlineColor": "#000000",
                    "outlineWidth": 2, "allCaps": False, "boxEnabled": False, "position": "top"}
        _title_layout = _compute_subtitle_layout(title, title_font, _t_line_h, _t_style, w, h)

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
            text = get_text_at(t, entries, _starts)
            needs_draw = text or has_title
            if needs_draw:
                img = Image.frombytes("RGB", (w, h), chunk)
                if text:
                    # Cache layout (tiny list of tuples) — NOT the full overlay image
                    # which was ~8MB per unique text and caused RAM blowup.
                    if text not in _subtitle_cache:
                        _subtitle_cache[text] = _compute_subtitle_layout(text, font, line_h, style, w, h)
                    layout = _subtitle_cache[text]
                    if _use_alpha_path:
                        # Rebuild small overlay per frame using cached positions (no wrap_text).
                        overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
                        draw = ImageDraw.Draw(overlay)
                        for line, x, y, tw in layout:
                            pad_x, pad_y = 12, 5
                            draw.rectangle([max(0, x - pad_x), y - pad_y,
                                            min(w, x + tw + pad_x), y + line_h - 4],
                                           fill=(*_box_rgb, _box_alpha))
                            draw.text((x, y), line, font=font, fill=(*_text_rgb, 255),
                                      stroke_width=_outline_w,
                                      stroke_fill=(*_outline_rgb, 255) if _outline_w > 0 else None)
                        img = Image.alpha_composite(img.convert("RGBA"), overlay).convert("RGB")
                    else:
                        draw = ImageDraw.Draw(img)
                        for line, x, y, _ in layout:
                            draw.text((x, y), line, font=font, fill=_text_rgb,
                                      stroke_width=_outline_w,
                                      stroke_fill=_outline_rgb if _outline_w > 0 else None)
                if _title_layout:
                    draw = ImageDraw.Draw(img)
                    for line, x, y, _ in _title_layout:
                        draw.text((x, y), line, font=title_font, fill=_title_text_rgb,
                                  stroke_width=2, stroke_fill=(0, 0, 0))
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


def _run_ffmpeg_with_progress(cmd, total_duration_sec):
    """Run ffmpeg with -progress pipe:1, translating out_time= lines into
    our own PROGRESS:N convention on stderr (same format the per-frame
    burn() path already emits, so the Rust caller needs no changes).
    stderr is drained on a background thread concurrently with stdout to
    avoid a pipe deadlock if ffmpeg ever writes enough to stderr to fill
    the OS pipe buffer before -progress completes."""
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    stderr_chunks = []
    def _drain_stderr():
        if proc.stderr is not None:
            for line in proc.stderr:
                stderr_chunks.append(line)
    stderr_thread = threading.Thread(target=_drain_stderr, daemon=True)
    stderr_thread.start()

    last_pct = 0
    if proc.stdout is not None:
        for line in proc.stdout:
            line = line.strip()
            if line.startswith("out_time=") and total_duration_sec > 0:
                time_str = line.split("=", 1)[1]
                try:
                    h_s, m_s, s_s = time_str.split(":")
                    seconds = int(h_s) * 3600 + int(m_s) * 60 + float(s_s)
                    pct = min(99, int(seconds / total_duration_sec * 100))
                    if pct > last_pct:
                        emit_progress(pct)
                        last_pct = pct
                except ValueError:
                    pass

    proc.wait()
    stderr_thread.join(timeout=5)
    return proc.returncode, "".join(stderr_chunks)


def burn_native(input_path, entries, output_path, font_size=0, font_path=None, style=None,
                 title="", title_font_size=0, title_color="#ffffff"):
    if style is None:
        style = {}

    w, h, fps, total_frames = get_video_info(input_path)
    duration_sec = total_frames / fps if fps > 0 else 0
    actual_size = font_size if font_size > 0 else max(26, h // 22)

    all_text = " ".join(e.get("text", "") for e in entries) + " " + title
    font = find_font(actual_size, font_path, text_hint=all_text)
    line_h = actual_size + 8

    has_title = bool(title and title.strip())
    actual_title_size = title_font_size if title_font_size > 0 else max(32, h // 18)
    title_font = find_font(actual_title_size, font_path, text_hint=title) if has_title else None
    title_line_h = title_font.size + 8 if has_title and hasattr(title_font, 'size') else 40
    title_style = {"textColor": title_color, "outlineColor": "#000000", "outlineWidth": 2,
                   "allCaps": False, "boxEnabled": False, "position": "top"}

    tmp_dir = tempfile.mkdtemp(prefix="autoclipper_burn_")
    try:
        emit_status(f"[burn] native path: {len(entries)} entri, render PNG overlay...")
        emit_progress(0)

        # Render one PNG per unique subtitle text.
        text_to_png = {}
        for e in entries:
            text = e["text"]
            if text in text_to_png:
                continue
            img = render_overlay_image(text, font, line_h, style, w, h)
            png_path = os.path.join(tmp_dir, f"sub_{len(text_to_png)}.png")
            img.save(png_path)
            text_to_png[text] = png_path

        title_png = None
        if has_title:
            title_img = render_overlay_image(title, title_font, title_line_h, title_style, w, h)
            title_png = os.path.join(tmp_dir, "title.png")
            title_img.save(title_png)

        emit_progress(5)

        # Build ffmpeg input list + filter_complex graph.
        ffmpeg_inputs = ["-i", input_path]
        filter_lines = []
        prev_label = "[0:v]"
        input_idx = 1
        overlay_count = sum(1 for _ in entries) + (1 if has_title else 0)
        step = 0
        for e in entries:
            png_path = text_to_png[e["text"]]
            ffmpeg_inputs += ["-i", png_path]
            step += 1
            is_last = (step == overlay_count)
            out_label = "[vout]" if (is_last and not has_title) else f"[v{step}]"
            filter_lines.append(
                f"{prev_label}[{input_idx}:v]overlay=0:0:"
                f"enable='between(t,{e['start']:.3f},{e['end']:.3f})'{out_label}"
            )
            prev_label = out_label
            input_idx += 1

        if has_title:
            ffmpeg_inputs += ["-i", title_png]
            filter_lines.append(
                f"{prev_label}[{input_idx}:v]overlay=0:0:"
                f"enable='between(t,0,{duration_sec:.3f})'[vout]"
            )
            input_idx += 1

        if not filter_lines:
            # No subtitles and no title — nothing to overlay, pass source through untouched.
            filter_lines.append(f"{prev_label}null[vout]")

        filter_script_path = os.path.join(tmp_dir, "filter.txt")
        with open(filter_script_path, "w") as f:
            f.write(";\n".join(filter_lines))

        encoder, is_hw = _pick_encoder()
        bitrate = _bitrate_for(w, h)
        emit_status(f"[burn] encoder: {encoder} ({'hardware' if is_hw else 'software'})")

        def _build_cmd(enc):
            cmd = [FFMPEG, "-y", "-loglevel", "error"] + ffmpeg_inputs + [
                "-filter_complex_script", filter_script_path,
                "-map", "[vout]", "-map", "0:a?",
                "-c:v", enc,
            ]
            cmd += ["-b:v", bitrate] if enc != "libx264" else ["-preset", "fast", "-crf", "23"]
            cmd += ["-c:a", "copy", "-progress", "pipe:1", output_path]
            return cmd

        returncode, stderr_output = _run_ffmpeg_with_progress(_build_cmd(encoder), duration_sec)

        if returncode != 0 and is_hw:
            emit_status(f"[burn] encoder {encoder} gagal, retry pakai libx264")
            returncode, stderr_output = _run_ffmpeg_with_progress(_build_cmd("libx264"), duration_sec)

        if returncode != 0:
            raise RuntimeError(f"ffmpeg gagal (exit {returncode}): {stderr_output[-2000:]}")

        emit_progress(100)
        return total_frames
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)


def burn_with_fallback(input_path, entries, output_path, font_size=0, font_path=None,
                        style=None, title="", title_font_size=0, title_color="#ffffff"):
    """Try the fast native ffmpeg-overlay path; fall back to the per-frame
    PIL path unchanged if native fails for any reason."""
    try:
        return burn_native(input_path, entries, output_path, font_size=font_size,
                            font_path=font_path, style=style, title=title,
                            title_font_size=title_font_size, title_color=title_color)
    except Exception as e:
        emit_status(f"[burn] native path gagal ({e}), fallback ke metode lama")
        return burn(input_path, entries, output_path, font_size=font_size,
                    font_path=font_path, style=style, title=title,
                    title_font_size=title_font_size, title_color=title_color)


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
        frames = burn_with_fallback(
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
