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


def detect_face_cx(frame, prev_frame, cascade):
    """
    Return horizontal center-x of the active speaker, or None.

    Priority:
    1. Mouth motion (absdiff between consecutive sampled frames) — detects the
       face whose jaw/lips are moving, i.e. who is currently speaking.
       Works even when that face is slightly blurred (bokeh) because jaw movement
       still produces intensity changes across frames.
    2. Sharpness fallback — used when no clear speaker signal exists (silence,
       first frame, or only one face detected).
    """
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
        return cv2.Laplacian(gray[fy:fy + fh, fx:fx + fw], cv2.CV_64F).var()

    if len(faces) == 1:
        x, y, fw, fh = faces[0]
        return x + fw // 2

    # ── Mouth-motion scoring ──────────────────────────────────────────────────
    if prev_frame is not None:
        prev_gray = cv2.cvtColor(prev_frame, cv2.COLOR_BGR2GRAY)

        def mouth_motion(face):
            fx, fy, fw, fh = face
            # Mouth region: lower 30% of the face bounding box
            my = fy + int(fh * 0.65)
            mh = max(1, int(fh * 0.30))
            curr = gray[my:my + mh, fx:fx + fw]
            prev = prev_gray[my:my + mh, fx:fx + fw]
            if curr.size == 0:
                return 0.0
            return float(cv2.absdiff(curr, prev).mean())

        motions = [(face, mouth_motion(face)) for face in faces]
        top  = max(motions, key=lambda t: t[1])
        rest = [m for _, m in motions if _ is not top[0]]
        avg_rest = sum(rest) / len(rest) if rest else 0.0

        # Clear speaker: top motion > 1 px mean AND > 2× the average of others
        if top[1] > 1.0 and top[1] > avg_rest * 2.0:
            x, y, fw, fh = top[0]
            return x + fw // 2

    # ── Sharpness fallback (silence or first frame) ───────────────────────────
    x, y, fw, fh = max(faces, key=sharpness)
    return x + fw // 2


def smooth_adaptive(arr, fps, src_w):
    """
    EMA smoothing with two modes:
    - Slow (0.5s time constant): stable tracking, suppresses camera jitter.
    - Fast (0.08s time constant): near-instant on focus pulls (face position
      jumps > 15% of frame width), so the crop follows the newly focused subject
      within 2-3 frames instead of waiting 2 seconds.
    """
    if len(arr) == 0:
        return arr

    result = np.empty_like(arr)
    result[0] = arr[0]

    alpha_slow = 1.0 - np.exp(-1.0 / max(1.0, fps * 0.5))
    alpha_fast = 1.0 - np.exp(-1.0 / max(1.0, fps * 0.08))
    focus_pull_threshold = src_w * 0.15  # 15% of frame width

    for i in range(1, len(arr)):
        delta = abs(arr[i] - result[i - 1])
        alpha = alpha_fast if delta > focus_pull_threshold else alpha_slow
        result[i] = alpha * arr[i] + (1.0 - alpha) * result[i - 1]

    return result.astype(int)


def analyze_faces(video_path, crop_w, src_w, fps):
    """
    First pass: sample frames to detect face positions.
    Returns a list of smoothed crop_x values (one per actual frame).
    """
    cascade_path = cv2.data.haarcascades + "haarcascade_frontalface_default.xml"
    cascade = cv2.CascadeClassifier(cascade_path)

    # Sample ~4fps for fast focus/speaker-change detection
    every = max(1, int(fps / 4))
    default_cx = src_w // 2
    last_cx = default_cx
    prev_frame = None  # previous sampled frame for mouth-motion comparison

    raw_cx = []
    cap = cv2.VideoCapture(video_path)
    idx = 0

    while True:
        if idx % every == 0:
            ret, frame = cap.read()
            if not ret:
                break
            detected = detect_face_cx(frame, prev_frame, cascade)
            if detected is not None:
                last_cx = detected
            prev_frame = frame
        else:
            if not cap.grab():
                break
        raw_cx.append(last_cx)
        idx += 1

    cap.release()

    if not raw_cx:
        return [max(0, default_cx - crop_w // 2)]

    arr = np.array(raw_cx, dtype=float)
    smoothed = smooth_adaptive(arr, fps, src_w)

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
