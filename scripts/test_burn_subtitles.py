#!/usr/bin/env python3
"""Self-check for burn_subtitles.py (assert-based, no framework)."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from PIL import Image, ImageFont
from burn_subtitles import render_overlay_image, draw_subtitle  # noqa: E402


def _default_font(size=32):
    return ImageFont.load_default()


# Small tolerance for sub-pixel anti-aliasing rounding noise at stroke joins
# (observed: a handful of pixels differ by a few gray levels between the
# direct-draw path and the transparent-canvas-then-composite path). A real
# regression (e.g. a missing outline) produces thousands of differing
# pixels, not tens, so this threshold stays far from masking that.
MAX_DIFFERING_PIXELS = 20


def _count_differing_pixels(img_a, img_b):
    return sum(1 for a, b in zip(img_a.getdata(), img_b.getdata()) if a != b)


def test_overlay_matches_draw_subtitle_no_box():
    """render_overlay_image (transparent) composited onto a black frame must
    match draw_subtitle's direct-draw output within a small anti-aliasing
    tolerance, for the same text/font/style with box disabled."""
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

    diff = _count_differing_pixels(direct, composited)
    assert diff <= MAX_DIFFERING_PIXELS, f"{diff} differing pixels (black bg, no box)"


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

    diff = _count_differing_pixels(direct, composited)
    assert diff <= MAX_DIFFERING_PIXELS, f"{diff} differing pixels (black bg, with box)"


def test_overlay_matches_draw_subtitle_on_white_background():
    """Parity check on a WHITE base — this is the class of bug that shipped
    silently before: the black-canvas-then-colorkey approach deleted a black
    outline drawn on a black background, but that defect was invisible when
    every test only composited onto black. Uses the default/common style
    (black outline, white text) which is exactly what broke."""
    w, h = 320, 240
    font = _default_font()
    style = {"textColor": "#ffffff", "outlineColor": "#000000", "outlineWidth": 2,
              "boxEnabled": False, "position": "bottom"}
    line_h = 24

    direct = Image.new("RGB", (w, h), (255, 255, 255))
    direct = draw_subtitle(direct, "Halo dunia", font, line_h, style)

    overlay = render_overlay_image("Halo dunia", font, line_h, style, w, h)
    composited = Image.alpha_composite(
        Image.new("RGBA", (w, h), (255, 255, 255, 255)), overlay
    ).convert("RGB")

    diff = _count_differing_pixels(direct, composited)
    assert diff <= MAX_DIFFERING_PIXELS, f"{diff} differing pixels (white bg, no box)"


def test_overlay_is_transparent_outside_text():
    """Pixels far from any text must be fully transparent (alpha=0)."""
    w, h = 320, 240
    font = _default_font()
    style = {"textColor": "#ffffff", "outlineColor": "#000000", "outlineWidth": 2,
              "boxEnabled": False, "position": "bottom"}
    overlay = render_overlay_image("Halo", font, 24, style, w, h)
    assert overlay.getpixel((5, 5))[3] == 0


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


if __name__ == "__main__":
    test_overlay_matches_draw_subtitle_no_box()
    test_overlay_matches_draw_subtitle_with_box()
    test_overlay_matches_draw_subtitle_on_white_background()
    test_overlay_is_transparent_outside_text()
    test_bitrate_for_resolution()
    test_pick_encoder_returns_valid_tuple()
    test_burn_native_end_to_end()
    print("OK: burn_subtitles native-overlay self-check passed")
