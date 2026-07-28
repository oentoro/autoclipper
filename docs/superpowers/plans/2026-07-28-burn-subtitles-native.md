# Burn Subtitles Native Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite subtitle burning to use a single native ffmpeg pass (PNG overlay per unique text, timed via `overlay=...:enable=`) instead of a per-frame Python/PIL/raw-pipe loop, cutting burn time roughly 3-4x for typical clips, while keeping the old per-frame path as an automatic fallback.

**Architecture:** New function `burn_native()` in `scripts/burn_subtitles.py` renders each unique subtitle text (and the title, if any) to a transparent PNG once, using the exact same PIL rendering logic already in the file (no CJK/word-wrap/box-style changes). It then builds one `ffmpeg filter_complex` graph that overlays each PNG onto the source video during its time window, and runs ONE ffmpeg process (hardware encoder when available, falling back to `libx264`). If `burn_native()` raises for any reason, `__main__` falls back to the existing `burn()` function unchanged. The Rust/frontend integration is untouched — CLI argument contract and JSON output format stay identical.

**Tech Stack:** Python 3, Pillow (PIL), ffmpeg (filter_complex, hardware encoders: h264_videotoolbox/nvenc/qsv/amf), subprocess.

## Global Constraints

- `burn()` (existing per-frame implementation) MUST remain unmodified and reachable as a fallback — no deletion, no behavior change.
- Zero changes to `src-tauri/src/commands.rs` or any frontend file — CLI args in, JSON `{"success":true,"frames":N}` / `{"error":...}` out, `PROGRESS:N` lines on stderr, must stay byte-for-byte compatible with what `exec_burn_subs` already parses.
- No change to font selection, CJK detection, word-wrap, or box/outline/color styling logic — only *how* rendered pixels reach the output video changes, not *how* they're computed.
- Any failure in the native path (encoder unavailable, ffmpeg non-zero exit, PNG render error, etc.) must fall back to `burn()` automatically — never surface a native-path-specific error to the user without having tried the fallback first.

---

## Task 1: `render_overlay_image()` — transparent-canvas renderer + parity test

**Files:**
- Modify: `scripts/burn_subtitles.py` (add function after `draw_subtitle`, i.e. after line 376 in the current file)
- Test: `scripts/test_burn_subtitles.py` (new)

**Interfaces:**
- Produces: `render_overlay_image(text: str, font, line_h: int, style: dict, w: int, h: int) -> PIL.Image.Image` — returns an RGBA image of size `(w, h)`, transparent everywhere except the rendered text (and box, if `boxEnabled` is on). Used by Task 3's `burn_native()`.
- Consumes: existing `_build_subtitle_overlay`, `_compute_subtitle_layout`, `hex_to_rgb` (all already in `scripts/burn_subtitles.py`, unchanged).

- [ ] **Step 1: Write the failing test**

Create `scripts/test_burn_subtitles.py`:

```python
#!/usr/bin/env python3
"""Self-check for burn_subtitles.py (assert-based, no framework)."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from PIL import Image, ImageFont
from burn_subtitles import render_overlay_image, draw_subtitle  # noqa: E402


def _default_font(size=32):
    return ImageFont.load_default()


def test_overlay_matches_draw_subtitle_no_box():
    """render_overlay_image (transparent) composited onto a black frame must
    match draw_subtitle's direct-draw output pixel-for-pixel, for the same
    text/font/style with box disabled."""
    w, h = 320, 240
    font = _default_font()
    style = {"textColor": "#ffffff", "outlineColor": "#000000", "outlineWidth": 2,
              "boxEnabled": False, "position": "bottom"}
    line_h = 24

    direct = Image.new("RGB", (w, h), (0, 0, 0))
    direct = draw_subtitle(direct, "Halo dunia", font, line_h, style)

    overlay = render_overlay_image("Halo dunia", font, line_h, style, w, h)
    composited = Image.alpha_composite(
        Image.new("RGBA", (w, h), (0, 0, 0, 255)), overlay
    ).convert("RGB")

    assert list(direct.getdata()) == list(composited.getdata())


def test_overlay_matches_draw_subtitle_with_box():
    """Same parity check with box rendering enabled."""
    w, h = 320, 240
    font = _default_font()
    style = {"textColor": "#ffffff", "outlineColor": "#000000", "outlineWidth": 2,
              "boxEnabled": True, "boxColor": "#000000", "boxOpacity": 70,
              "position": "bottom"}
    line_h = 24

    direct = Image.new("RGB", (w, h), (0, 0, 0))
    direct = draw_subtitle(direct, "Halo dunia", font, line_h, style)

    overlay = render_overlay_image("Halo dunia", font, line_h, style, w, h)
    composited = Image.alpha_composite(
        Image.new("RGBA", (w, h), (0, 0, 0, 255)), overlay
    ).convert("RGB")

    assert list(direct.getdata()) == list(composited.getdata())


def test_overlay_is_transparent_outside_text():
    """Pixels far from any text must be fully transparent (alpha=0)."""
    w, h = 320, 240
    font = _default_font()
    style = {"textColor": "#ffffff", "outlineColor": "#000000", "outlineWidth": 2,
              "boxEnabled": False, "position": "bottom"}
    overlay = render_overlay_image("Halo", font, 24, style, w, h)
    assert overlay.getpixel((5, 5))[3] == 0


if __name__ == "__main__":
    test_overlay_matches_draw_subtitle_no_box()
    test_overlay_matches_draw_subtitle_with_box()
    test_overlay_is_transparent_outside_text()
    print("OK: burn_subtitles native-overlay self-check passed")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 scripts/test_burn_subtitles.py`
Expected: `ImportError: cannot import name 'render_overlay_image' from 'burn_subtitles'`

- [ ] **Step 3: Implement `render_overlay_image()`**

In `scripts/burn_subtitles.py`, add this function immediately after `draw_subtitle` (after the line `return img` that ends `draw_subtitle`, i.e. right before `def draw_title(img, text, font, style):`):

```python
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 scripts/test_burn_subtitles.py`
Expected: `OK: burn_subtitles native-overlay self-check passed`

- [ ] **Step 5: Commit**

```bash
git add scripts/burn_subtitles.py scripts/test_burn_subtitles.py
git commit -m "feat: render_overlay_image untuk transparent PNG overlay subtitle"
```

---

## Task 2: Encoder selection — `_pick_encoder()` + `_bitrate_for()`

**Files:**
- Modify: `scripts/burn_subtitles.py` (add functions near the top, after the `FFMPEG`/`FFPROBE` bin-detection block, i.e. after line 28 `FFPROBE = ...`)
- Test: `scripts/test_burn_subtitles.py` (append)

**Interfaces:**
- Produces: `_pick_encoder() -> tuple[str, bool]` (encoder name, `is_hardware`); `_bitrate_for(w: int, h: int) -> str` (e.g. `"8M"`). Both consumed by Task 3's `burn_native()`.

- [ ] **Step 1: Write the failing test — append to `scripts/test_burn_subtitles.py`**

Add these functions (before the `if __name__ == "__main__":` block) and update that block:

```python
def test_bitrate_for_resolution():
    from burn_subtitles import _bitrate_for
    assert _bitrate_for(1280, 720) == "5M"
    assert _bitrate_for(1920, 1080) == "8M"
    assert _bitrate_for(1080, 1920) == "8M"   # vertical, same pixel count as 1080p
    assert _bitrate_for(3840, 2160) == "16M"


def test_pick_encoder_returns_valid_tuple():
    from burn_subtitles import _pick_encoder
    name, is_hw = _pick_encoder()
    assert isinstance(name, str) and len(name) > 0
    assert isinstance(is_hw, bool)
    # Must always have a usable fallback even with no hardware encoder present
    if not is_hw:
        assert name == "libx264"
```

Update the `if __name__ == "__main__":` block at the bottom of the file to also run these:

```python
if __name__ == "__main__":
    test_overlay_matches_draw_subtitle_no_box()
    test_overlay_matches_draw_subtitle_with_box()
    test_overlay_is_transparent_outside_text()
    test_bitrate_for_resolution()
    test_pick_encoder_returns_valid_tuple()
    print("OK: burn_subtitles native-overlay self-check passed")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 scripts/test_burn_subtitles.py`
Expected: `ImportError: cannot import name '_bitrate_for' from 'burn_subtitles'`

- [ ] **Step 3: Implement `_pick_encoder()` and `_bitrate_for()`**

In `scripts/burn_subtitles.py`, add after line 28 (`FFPROBE = os.environ.get("AUTOCLIPPER_FFPROBE") or _find_bin("ffprobe")`):

```python
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 scripts/test_burn_subtitles.py`
Expected: `OK: burn_subtitles native-overlay self-check passed`

- [ ] **Step 5: Commit**

```bash
git add scripts/burn_subtitles.py scripts/test_burn_subtitles.py
git commit -m "feat: deteksi hardware encoder + heuristik bitrate"
```

---

## Task 3: `burn_native()` — core native ffmpeg overlay pipeline

**Files:**
- Modify: `scripts/burn_subtitles.py` (add function after `burn()`, i.e. after line 513 `return frame_num` that ends `burn()`, before `if __name__ == "__main__":`)

**Interfaces:**
- Consumes: `get_video_info` (existing), `find_font` (existing), `render_overlay_image` (Task 1), `_pick_encoder`, `_bitrate_for` (Task 2), `emit_progress`, `emit_status` (existing).
- Produces: `burn_native(input_path, entries, output_path, font_size=0, font_path=None, style=None, title="", title_font_size=0, title_color="#ffffff") -> int` — same signature and return type (frame count) as `burn()`, consumed by Task 4's `__main__` dispatch.

- [ ] **Step 1: Add required imports**

At the top of `scripts/burn_subtitles.py`, change:
```python
import sys, json, subprocess, os, argparse, bisect
```
to:
```python
import sys, json, subprocess, os, argparse, bisect, tempfile, shutil
```

- [ ] **Step 2: Implement `burn_native()`**

Add to `scripts/burn_subtitles.py`, right before `if __name__ == "__main__":`:

```python
def _run_ffmpeg_with_progress(cmd, total_duration_sec):
    """Run ffmpeg with -progress pipe:1, translating out_time= lines into
    our own PROGRESS:N convention on stderr (same format the per-frame
    burn() path already emits, so the Rust caller needs no changes)."""
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
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
    stderr_output = proc.stderr.read() if proc.stderr else ""
    return proc.returncode, stderr_output


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
```

- [ ] **Step 3: Write the smoke test — append to `scripts/test_burn_subtitles.py`**

Add before the `if __name__ == "__main__":` block:

```python
def test_burn_native_end_to_end():
    """Synthetic tiny video + a couple of entries → burn_native succeeds,
    produces a non-trivial output file. Requires ffmpeg on PATH."""
    import subprocess as sp
    import tempfile as tf
    from burn_subtitles import burn_native

    workdir = tf.mkdtemp(prefix="autoclipper_test_")
    src = os.path.join(workdir, "src.mp4")
    out = os.path.join(workdir, "out.mp4")
    sp.run([
        "ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
        "-f", "lavfi", "-i", "testsrc=duration=3:size=320x240:rate=10",
        "-pix_fmt", "yuv420p", src,
    ], check=True)

    entries = [
        {"start": 0.0, "end": 1.2, "text": "Halo"},
        {"start": 1.5, "end": 2.5, "text": "Dunia"},
    ]
    frames = burn_native(src, entries, out, style={"boxEnabled": False})
    assert frames > 0
    assert os.path.exists(out)
    assert os.path.getsize(out) > 1000
```

Update the `if __name__ == "__main__":` block again:

```python
if __name__ == "__main__":
    test_overlay_matches_draw_subtitle_no_box()
    test_overlay_matches_draw_subtitle_with_box()
    test_overlay_is_transparent_outside_text()
    test_bitrate_for_resolution()
    test_pick_encoder_returns_valid_tuple()
    test_burn_native_end_to_end()
    print("OK: burn_subtitles native-overlay self-check passed")
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 scripts/test_burn_subtitles.py`
Expected: `OK: burn_subtitles native-overlay self-check passed`

If `test_burn_native_end_to_end` fails with an ffmpeg filter-graph error mentioning a PNG input being referenced but only usable once: this means ffmpeg's build does not allow reusing a demuxer input stream label across two overlay nodes when the SAME text repeats at two different (non-contiguous) time windows. If this happens: change `text_to_png` handling in `burn_native()` so each ENTRY gets its own `-i` even when text repeats (drop the `if text in text_to_png: continue` dedup at the input-building stage, keep it only for the PNG-render step to avoid redundant renders, then reference each entry's own copied/duplicated `-i` slot). Document whichever behavior was observed in the task report.

- [ ] **Step 5: Commit**

```bash
git add scripts/burn_subtitles.py scripts/test_burn_subtitles.py
git commit -m "feat: burn_native() — native ffmpeg overlay pipeline"
```

---

## Task 4: `__main__` integration — try native, fallback to legacy

**Files:**
- Modify: `scripts/burn_subtitles.py` (the `if __name__ == "__main__":` block at the end of the file, currently calling `burn(...)` directly around line 541)
- Test: `scripts/test_burn_subtitles.py` (append)

**Interfaces:**
- Consumes: `burn_native` (Task 3), `burn` (existing, unchanged).
- Produces: CLI behavior — `python3 burn_subtitles.py <input> <entries.json> <output> [...]` tries `burn_native` first, falls back to `burn` on any exception, same JSON output contract either way.

- [ ] **Step 1: Write the failing test — append to `scripts/test_burn_subtitles.py`**

Add before the `if __name__ == "__main__":` block:

```python
def test_main_falls_back_to_legacy_burn_on_native_failure():
    """If burn_native raises, the dispatch logic must call burn() instead
    and still succeed — verified by calling the extracted dispatch function
    directly with a monkeypatched burn_native that always raises."""
    import burn_subtitles as bs

    calls = {"native": 0, "legacy": 0}

    def fake_native(*a, **kw):
        calls["native"] += 1
        raise RuntimeError("simulated native failure")

    def fake_legacy(*a, **kw):
        calls["legacy"] += 1
        return 42

    original_native, original_legacy = bs.burn_native, bs.burn
    bs.burn_native, bs.burn = fake_native, fake_legacy
    try:
        frames = bs.burn_with_fallback("in.mp4", [], "out.mp4")
    finally:
        bs.burn_native, bs.burn = original_native, original_legacy

    assert calls["native"] == 1
    assert calls["legacy"] == 1
    assert frames == 42
```

Update the `if __name__ == "__main__":` block again:

```python
if __name__ == "__main__":
    test_overlay_matches_draw_subtitle_no_box()
    test_overlay_matches_draw_subtitle_with_box()
    test_overlay_is_transparent_outside_text()
    test_bitrate_for_resolution()
    test_pick_encoder_returns_valid_tuple()
    test_burn_native_end_to_end()
    test_main_falls_back_to_legacy_burn_on_native_failure()
    print("OK: burn_subtitles native-overlay self-check passed")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `python3 scripts/test_burn_subtitles.py`
Expected: `AttributeError: module 'burn_subtitles' has no attribute 'burn_with_fallback'`

- [ ] **Step 3: Extract dispatch logic into `burn_with_fallback()`, wire up `__main__`**

In `scripts/burn_subtitles.py`, add this function right before `if __name__ == "__main__":` (after `burn_native`):

```python
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
```

Then find this existing block near the bottom of the file:
```python
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
```
and change `burn(` to `burn_with_fallback(`:
```python
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `python3 scripts/test_burn_subtitles.py`
Expected: `OK: burn_subtitles native-overlay self-check passed`

- [ ] **Step 5: Commit**

```bash
git add scripts/burn_subtitles.py scripts/test_burn_subtitles.py
git commit -m "feat: dispatch burn_native dengan fallback otomatis ke burn() lama"
```

---

## Task 5: End-to-end benchmark verification

**Files:** None modified — verification only.

**Interfaces:**
- Consumes: `scripts/burn_subtitles.py` CLI (as invoked by `commands.rs`'s `exec_burn_subs`, but run directly here for measurement).

- [ ] **Step 1: Reproduce the original benchmark scenario**

Run (creates a scratch 30s 1080x1920 test video + 12 subtitle entries, same as the manual benchmark done during brainstorming):

```bash
mkdir -p /tmp/burn_bench
ffmpeg -y -hide_banner -loglevel error -f lavfi -i "testsrc=duration=30:size=1080x1920:rate=30" -f lavfi -i "sine=frequency=440:duration=30" -pix_fmt yuv420p -c:v libx264 -preset fast -crf 23 -c:a aac /tmp/burn_bench/sample.mp4
python3 -c "
import json
entries = []
t = 0.0
i = 0
while t < 29:
    entries.append({'start': t, 'end': t + 2.2, 'text': f'Ini contoh subtitle nomor {i} buat testing benchmark encoder'})
    t += 2.5
    i += 1
json.dump(entries, open('/tmp/burn_bench/entries.json', 'w'))
"
```

- [ ] **Step 2: Time the new dispatch path (native, with automatic fallback wiring intact)**

Run:
```bash
cd /Users/oentoro/Projects/autoclipper
time python3 scripts/burn_subtitles.py /tmp/burn_bench/sample.mp4 /tmp/burn_bench/entries.json /tmp/burn_bench/out_native.mp4
```
Expected: JSON `{"success": true, "frames": 900}` on stdout, `[burn] encoder: ...` status line and `PROGRESS:N` lines on stderr, wall time well under the old 8.9s baseline (target: close to the ~2.5-4s range demonstrated during feasibility testing, though exact number depends on machine).

- [ ] **Step 3: Verify visual output**

Run: `ffprobe -v error -show_entries stream=width,height,duration,codec_name -of default=noprint_wrappers=1 /tmp/burn_bench/out_native.mp4`
Expected: `width=1080`, `height=1920`, `duration≈30.0`, `codec_name=h264`. Then open `/tmp/burn_bench/out_native.mp4` (e.g. `open /tmp/burn_bench/out_native.mp4` on macOS) and visually confirm subtitles appear at the correct times with the expected styling (white text, black outline, bottom position — the default style used when `--style` isn't passed).

- [ ] **Step 4: Record the result**

Write the measured wall-clock time and any visual QA notes (subtitle timing correct? styling matches expectation? file size reasonable vs the ~3.1MB the old libx264 path produced for the same clip?) into the task report. If timing or visual output is materially wrong, treat as a regression and return to the relevant earlier task rather than closing this one.

- [ ] **Step 5: Clean up scratch files**

```bash
rm -rf /tmp/burn_bench
```

No commit for this task (verification only, no source changes).
