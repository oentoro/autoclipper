#!/usr/bin/env python3
"""
Face censor — pixelate semua wajah terdeteksi di setiap frame video.

Reuse detector dari smart_crop.py (InsightFace > MediaPipe > YuNet > Haar
cascade). Beda dengan smart_crop.py: deteksi jalan di SETIAP frame (bukan
sample ~4fps) karena ini fitur sensor/privasi — ketepatan lebih penting
dari performa.
"""

import sys
import os
import json
import subprocess
import argparse

sys.path.insert(0, os.path.dirname(__file__))
from smart_crop import (
    emit_progress,
    emit_status,
    _load_insightface,
    _detect_insightface,
    _load_mediapipe,
    _detect_mediapipe,
    _load_yunet,
    _detect_yunet,
    _detect_cascade,
)

try:
    import cv2
except ImportError as _e:
    os.write(2, (f"Error: {_e}\nFace Censor membutuhkan opencv-python.\n"
                 "Jalankan: python3 -m pip install opencv-python\n").encode("utf-8"))
    os.write(1, (json.dumps({"error": str(_e)}) + "\n").encode("utf-8"))
    sys.exit(1)


def pixelate_region(frame, bbox, padding: float = 0.15):
    """
    Pixelate area bbox (x, y, w, h) di frame, in-place. Bbox diperbesar
    dengan padding (fraksi dari w/h) di tiap sisi biar tepi wajah (dagu,
    rambut depan) ikut ter-cover, lalu di-clamp ke batas frame.
    Return frame yang sama (dimodifikasi in-place) untuk memudahkan testing.
    """
    h, w = frame.shape[:2]
    bx, by, bw, bh = bbox
    pad_x = int(bw * padding)
    pad_y = int(bh * padding)
    x1 = max(0, bx - pad_x)
    y1 = max(0, by - pad_y)
    x2 = min(w, bx + bw + pad_x)
    y2 = min(h, by + bh + pad_y)
    if x2 <= x1 or y2 <= y1:
        return frame

    roi = frame[y1:y2, x1:x2]
    rh, rw = roi.shape[:2]
    block = max(6, min(rw, rh) // 8)
    small_w = max(1, rw // block)
    small_h = max(1, rh // block)
    small = cv2.resize(roi, (small_w, small_h), interpolation=cv2.INTER_LINEAR)
    mosaic = cv2.resize(small, (rw, rh), interpolation=cv2.INTER_NEAREST)
    frame[y1:y2, x1:x2] = mosaic
    return frame
