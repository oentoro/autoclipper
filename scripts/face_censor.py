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


def main():
    parser = argparse.ArgumentParser(description="Face censor — pixelate semua wajah terdeteksi")
    parser.add_argument("input",  help="Input video path")
    parser.add_argument("output", help="Output video path")
    args = parser.parse_args()

    ffmpeg = os.environ.get("AUTOCLIPPER_FFMPEG", "ffmpeg")

    cap = cv2.VideoCapture(args.input)
    if not cap.isOpened():
        os.write(1, (json.dumps({"error": f"tidak dapat membuka video: {args.input}"}) + "\n").encode("utf-8"))
        sys.exit(1)

    src_w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    src_h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fps   = cap.get(cv2.CAP_PROP_FPS) or 30.0
    total_frames = max(1, int(cap.get(cv2.CAP_PROP_FRAME_COUNT)))
    cap.release()

    emit_status(f"[face_censor] {src_w}x{src_h} — mendeteksi & mensensor wajah tiap frame...")
    emit_progress(0)

    insight_app, insight_device = _load_insightface()
    mp_detector = None
    yunet = None
    if insight_app is not None:
        emit_status(f"[face_censor] Detector: InsightFace SCRFD — {insight_device}")
    else:
        mp_detector = _load_mediapipe()
        if mp_detector is not None:
            emit_status("[face_censor] Detector: MediaPipe — CPU")
        else:
            yunet = _load_yunet(src_w, src_h)
            if yunet is not None:
                emit_status("[face_censor] Detector: YuNet — CPU")
            else:
                emit_status("[face_censor] Detector: Haar cascade — CPU (fallback)")

    cascade_front   = cv2.CascadeClassifier(cv2.data.haarcascades + "haarcascade_frontalface_default.xml")
    cascade_profile = cv2.CascadeClassifier(cv2.data.haarcascades + "haarcascade_profileface.xml")

    ffmpeg_cmd = [
        ffmpeg, "-y",
        "-f", "rawvideo",
        "-pixel_format", "bgr24",
        "-video_size", f"{src_w}x{src_h}",
        "-framerate", str(fps),
        "-i", "pipe:0",
        "-i", args.input,
        "-map", "0:v:0",
        "-map", "1:a:0?",
        "-c:v", "libx264", "-preset", "fast", "-crf", "23",
        "-c:a", "aac", "-b:a", "128k",
        "-shortest",
        args.output,
    ]
    proc = subprocess.Popen(ffmpeg_cmd, stdin=subprocess.PIPE, stderr=subprocess.DEVNULL)

    cap = cv2.VideoCapture(args.input)
    frame_idx = 0
    progress_interval = max(1, total_frames // 100)

    while True:
        ret, frame = cap.read()
        if not ret:
            break

        if insight_app is not None:
            faces = _detect_insightface(frame, insight_app)
        elif mp_detector is not None:
            faces = _detect_mediapipe(frame, mp_detector)
        elif yunet is not None:
            faces = _detect_yunet(frame, yunet)
        else:
            gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
            faces = _detect_cascade(gray, cascade_front, cascade_profile)

        for f in faces:
            pixelate_region(frame, f["bbox"], padding=0.15)

        try:
            proc.stdin.write(frame.tobytes())
        except BrokenPipeError:
            break

        frame_idx += 1
        if frame_idx % progress_interval == 0:
            emit_progress(min(99, int(frame_idx / total_frames * 100)))

    cap.release()
    try:
        proc.stdin.close()
    except Exception:
        pass

    proc.wait()
    if proc.returncode != 0:
        os.write(1, (json.dumps({"error": f"FFmpeg render gagal (exit {proc.returncode})"}) + "\n").encode("utf-8"))
        sys.exit(1)

    emit_progress(100)
    emit_status("[face_censor] Selesai.")


if __name__ == "__main__":
    main()
