#!/usr/bin/env python3
"""
Smart vertical crop — tracks the speaker's face horizontally.
Uses OpenCV Haar cascade (bundled with opencv-python, no model download).
Pipe frames directly into FFmpeg to avoid double-encode quality loss.
"""

import sys
import os
import subprocess
import argparse

try:
    import cv2
    import numpy as np
except ImportError as _e:
    print(
        f"Error: {_e}\n"
        "Smart Crop membutuhkan opencv-python.\n"
        "Jalankan: python3 -m pip install opencv-python --break-system-packages",
        file=sys.stderr,
    )
    sys.exit(1)


def detect_face_cx(frame, cascade):
    """Return horizontal center-x of the sharpest (most in-focus) face, or None."""
    gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
    h, w = gray.shape
    min_dim = max(30, min(w, h) // 10)
    faces = cascade.detectMultiScale(
        gray, scaleFactor=1.1, minNeighbors=4,
        minSize=(min_dim, min_dim),
    )
    if len(faces) == 0:
        return None

    def sharpness(face):
        fx, fy, fw, fh = face
        roi = gray[fy:fy + fh, fx:fx + fw]
        return cv2.Laplacian(roi, cv2.CV_64F).var()

    x, y, fw, fh = max(faces, key=sharpness)
    return x + fw // 2


def analyze_faces(video_path, crop_w, src_w, fps):
    """
    First pass: sample frames to detect face positions.
    Returns a list of smoothed crop_x values (one per actual frame).
    """
    cascade_path = cv2.data.haarcascades + "haarcascade_frontalface_default.xml"
    cascade = cv2.CascadeClassifier(cascade_path)

    # Sample ~2 frames per second for face detection
    every = max(1, int(fps / 2))
    default_cx = src_w // 2
    last_cx = default_cx

    raw_cx = []
    cap = cv2.VideoCapture(video_path)
    idx = 0

    while True:
        if idx % every == 0:
            ret, frame = cap.read()
            if not ret:
                break
            detected = detect_face_cx(frame, cascade)
            if detected is not None:
                last_cx = detected
        else:
            if not cap.grab():
                break
        raw_cx.append(last_cx)
        idx += 1

    cap.release()

    if not raw_cx:
        return [max(0, default_cx - crop_w // 2)]

    # Smooth over ~2-second window to prevent jitter
    window = max(3, int(fps * 2))
    arr = np.array(raw_cx, dtype=float)
    kernel = np.ones(window) / window
    smoothed = np.convolve(arr, kernel, mode="same").astype(int)

    # Clamp face center to valid range, then compute top-left crop x
    min_cx = crop_w // 2
    max_cx = src_w - crop_w // 2
    crop_x = [int(np.clip(cx, min_cx, max_cx)) - crop_w // 2 for cx in smoothed]
    return crop_x


def main():
    parser = argparse.ArgumentParser(description="Smart face-tracking crop")
    parser.add_argument("input", help="Input video path")
    parser.add_argument("output", help="Output video path")
    parser.add_argument("--ratio", default="9:16", help="Target aspect ratio (e.g. 9:16)")
    args = parser.parse_args()

    ffmpeg = os.environ.get("AUTOCLIPPER_FFMPEG", "ffmpeg")

    # --- Read video metadata ---
    cap = cv2.VideoCapture(args.input)
    if not cap.isOpened():
        print(f"Error: tidak dapat membuka video: {args.input}", file=sys.stderr)
        sys.exit(1)

    src_w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    src_h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fps = cap.get(cv2.CAP_PROP_FPS) or 30.0
    cap.release()

    # --- Compute target crop dimensions ---
    aw, ah = [int(x) for x in args.ratio.split(":")]
    crop_w = min(src_w, int(src_h * aw / ah)) & ~1  # must be even
    crop_h = src_h & ~1

    print(f"[smart_crop] {src_w}x{src_h} → {crop_w}x{crop_h} ({args.ratio})", file=sys.stderr)
    print("[smart_crop] Mendeteksi posisi pembicara...", file=sys.stderr)

    crop_x_list = analyze_faces(args.input, crop_w, src_w, fps)

    print(f"[smart_crop] Menerapkan crop ke {len(crop_x_list)} frame...", file=sys.stderr)

    # --- Second pass: pipe cropped frames into FFmpeg (single re-encode) ---
    ffmpeg_cmd = [
        ffmpeg, "-y",
        "-f", "rawvideo",
        "-pixel_format", "bgr24",
        "-video_size", f"{crop_w}x{crop_h}",
        "-framerate", str(fps),
        "-i", "pipe:0",
        "-i", args.input,          # source for audio
        "-map", "0:v:0",
        "-map", "1:a:0?",          # optional audio (? = don't fail if absent)
        "-c:v", "libx264", "-preset", "fast", "-crf", "23",
        "-c:a", "aac", "-b:a", "128k",
        "-shortest",
        args.output,
    ]

    proc = subprocess.Popen(ffmpeg_cmd, stdin=subprocess.PIPE, stderr=subprocess.PIPE)

    cap = cv2.VideoCapture(args.input)
    frame_idx = 0
    last_x = crop_x_list[-1] if crop_x_list else 0

    while True:
        ret, frame = cap.read()
        if not ret:
            break
        x = crop_x_list[frame_idx] if frame_idx < len(crop_x_list) else last_x
        cropped = frame[:crop_h, x : x + crop_w]
        # Guard against undersized frames at end of file
        if cropped.shape[0] != crop_h or cropped.shape[1] != crop_w:
            cropped = cv2.resize(cropped, (crop_w, crop_h))
        try:
            proc.stdin.write(cropped.tobytes())
        except BrokenPipeError:
            break
        frame_idx += 1

    cap.release()
    try:
        proc.stdin.close()
    except Exception:
        pass

    _, stderr_data = proc.communicate()
    if proc.returncode != 0:
        print(stderr_data.decode(errors="replace"), file=sys.stderr)
        sys.exit(1)

    print("[smart_crop] Selesai.", file=sys.stderr)


if __name__ == "__main__":
    main()
