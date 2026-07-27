#!/usr/bin/env python3
"""Self-check for pixelate_region (assert-based, no framework)."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
import numpy as np
from face_censor import pixelate_region  # noqa: E402


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


if __name__ == "__main__":
    test_pixelate_reduces_variance()
    test_pixelate_clamps_to_frame_edges()
    test_padding_expands_processed_area()
    print("OK: pixelate_region self-check passed")
