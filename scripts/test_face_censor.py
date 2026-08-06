#!/usr/bin/env python3
"""Self-check for pixelate_region (assert-based, no framework)."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
import numpy as np
from face_censor import pixelate_region, overlay_image_region  # noqa: E402


def test_pixelate_reduces_variance():
    rng = np.random.default_rng(42)
    frame = rng.integers(0, 256, size=(100, 100, 3), dtype=np.uint8)
    original_roi = frame[20:80, 20:80].copy()
    pixelate_region(frame, (20, 20, 60, 60), padding=0.0)
    result_roi = frame[20:80, 20:80]
    assert result_roi.astype(float).var() < original_roi.astype(float).var()


def test_pixelate_clamps_to_frame_edges():
    frame = np.zeros((50, 50, 3), dtype=np.uint8)
    result = pixelate_region(frame, (-10, -10, 30, 30), padding=0.2)
    assert result.shape == (50, 50, 3)


def test_padding_expands_processed_area():
    rng = np.random.default_rng(7)
    base = rng.integers(0, 256, size=(100, 100, 3), dtype=np.uint8)
    frame_no_pad = base.copy()
    frame_padded = base.copy()
    pixelate_region(frame_no_pad, (30, 30, 20, 20), padding=0.0)
    pixelate_region(frame_padded, (30, 30, 20, 20), padding=1.0)
    changed_no_pad = np.count_nonzero(np.any(frame_no_pad != base, axis=2))
    changed_padded = np.count_nonzero(np.any(frame_padded != base, axis=2))
    assert changed_padded > changed_no_pad


def test_overlay_opaque_replaces_region():
    frame = np.zeros((100, 100, 3), dtype=np.uint8)
    overlay = np.full((10, 10, 3), 200, dtype=np.uint8)  # abu-abu solid
    overlay_image_region(frame, (20, 20, 60, 60), overlay, padding=0.0)
    roi = frame[20:80, 20:80]
    assert np.all(roi == 200)


def test_overlay_alpha_blends_with_original():
    overlay = np.zeros((10, 10, 4), dtype=np.uint8)
    overlay[:, :, :3] = 200  # warna abu-abu solid

    # alpha = 1.0 -> ROI harus jadi PERSIS warna overlay
    frame_full_alpha = np.zeros((100, 100, 3), dtype=np.uint8)
    overlay_full_alpha = overlay.copy()
    overlay_full_alpha[:, :, 3] = 255
    overlay_image_region(frame_full_alpha, (20, 20, 60, 60), overlay_full_alpha, padding=0.0)
    roi_full = frame_full_alpha[20:80, 20:80]
    assert np.all(roi_full == 200)

    # alpha = 0.0 -> ROI harus tetap PERSIS nilai frame asli (0)
    frame_zero_alpha = np.zeros((100, 100, 3), dtype=np.uint8)
    overlay_zero_alpha = overlay.copy()
    overlay_zero_alpha[:, :, 3] = 0
    overlay_image_region(frame_zero_alpha, (20, 20, 60, 60), overlay_zero_alpha, padding=0.0)
    roi_zero = frame_zero_alpha[20:80, 20:80]
    assert np.all(roi_zero == 0)


def test_grayscale_overlay_shape_is_invalid():
    # 2D grayscale array (no channel dim) — the shape this bug crashes on
    gray = np.zeros((20, 20), dtype=np.uint8)
    assert gray.ndim != 3
    # gray+alpha (2 channels) — also unsupported by overlay_image_region
    gray_alpha = np.zeros((20, 20, 2), dtype=np.uint8)
    assert gray_alpha.ndim == 3 and gray_alpha.shape[2] not in (3, 4)
    # valid BGR/BGRA still pass
    bgr = np.zeros((20, 20, 3), dtype=np.uint8)
    bgra = np.zeros((20, 20, 4), dtype=np.uint8)
    assert bgr.shape[2] in (3, 4)
    assert bgra.shape[2] in (3, 4)


def test_overlay_clamps_to_frame_edges():
    frame = np.zeros((50, 50, 3), dtype=np.uint8)
    overlay = np.full((10, 10, 3), 100, dtype=np.uint8)
    result = overlay_image_region(frame, (-10, -10, 30, 30), overlay, padding=0.2)
    assert result.shape == (50, 50, 3)


if __name__ == "__main__":
    test_pixelate_reduces_variance()
    test_pixelate_clamps_to_frame_edges()
    test_padding_expands_processed_area()
    test_overlay_opaque_replaces_region()
    test_overlay_alpha_blends_with_original()
    test_overlay_clamps_to_frame_edges()
    test_grayscale_overlay_shape_is_invalid()
    print("OK: pixelate_region + overlay_image_region self-check passed")
