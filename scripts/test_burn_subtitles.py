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
