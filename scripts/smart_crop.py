#!/usr/bin/env python3
"""
Smart vertical crop — tracks the speaker's face horizontally.

Detection strategy (in order of preference):
  1. YuNet (cv2.FaceDetectorYN) — handles frontal, 3/4 profile, and full
     profile (0°–90°).  Model is ~340 KB and auto-downloaded once to
     ~/.cache/autoclipper/ on first run.
  2. Multi-cascade fallback — haarcascade_frontalface_default.xml +
     haarcascade_profileface.xml (applied twice: original + horizontally
     flipped).  Bundled with opencv-python; no download needed.

Speaker selection (when multiple faces are visible):
  - Mouth-motion scoring via inter-frame pixel diff on the mouth region.
  - Sharpness fallback for frames with no speech.
"""

import sys
import os
import subprocess
import argparse
import urllib.request

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


# ── YuNet model management ────────────────────────────────────────────────────

_YUNET_URL = (
    "https://github.com/opencv/opencv_zoo/raw/main/models/"
    "face_detection_yunet/face_detection_yunet_2023mar.onnx"
)
_YUNET_FILENAME = "face_detection_yunet_2023mar.onnx"


def _yunet_model_path() -> str:
    cache = os.path.expanduser("~/.cache/autoclipper")
    return os.path.join(cache, _YUNET_FILENAME)


def _download_yunet() -> bool:
    path = _yunet_model_path()
    if os.path.exists(path):
        return True
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        print("[smart_crop] Mengunduh model face detection YuNet (~340 KB)...", file=sys.stderr)
        urllib.request.urlretrieve(_YUNET_URL, path + ".tmp")
        os.rename(path + ".tmp", path)
        print("[smart_crop] Model YuNet berhasil diunduh.", file=sys.stderr)
        return True
    except Exception as e:
        print(f"[smart_crop] Download YuNet gagal ({e}), pakai cascade fallback.", file=sys.stderr)
        if os.path.exists(path + ".tmp"):
            try:
                os.remove(path + ".tmp")
            except OSError:
                pass
        return False


def _load_yunet(frame_w: int, frame_h: int):
    """Return a cv2.FaceDetectorYN instance or None if unavailable."""
    if not hasattr(cv2, "FaceDetectorYN"):
        return None
    if not _download_yunet():
        return None
    try:
        det = cv2.FaceDetectorYN.create(
            _yunet_model_path(), "",
            (frame_w, frame_h),
            score_threshold=0.55,
            nms_threshold=0.3,
            top_k=100,
        )
        return det
    except Exception as e:
        print(f"[smart_crop] YuNet load gagal ({e}), pakai cascade fallback.", file=sys.stderr)
        return None


# ── Non-max suppression ───────────────────────────────────────────────────────

def _nms(boxes: list, iou_thr: float = 0.35) -> list:
    """Remove duplicate detections; keep largest area first."""
    if len(boxes) <= 1:
        return boxes
    b = np.array(boxes, dtype=float)
    x1, y1 = b[:, 0], b[:, 1]
    x2, y2 = b[:, 0] + b[:, 2], b[:, 1] + b[:, 3]
    areas = (x2 - x1) * (y2 - y1)
    order = areas.argsort()[::-1]
    keep = []
    while len(order):
        i = order[0]
        keep.append(int(i))
        xx1 = np.maximum(x1[i], x1[order[1:]])
        yy1 = np.maximum(y1[i], y1[order[1:]])
        xx2 = np.minimum(x2[i], x2[order[1:]])
        yy2 = np.minimum(y2[i], y2[order[1:]])
        inter = np.maximum(0, xx2 - xx1) * np.maximum(0, yy2 - yy1)
        iou = inter / (areas[i] + areas[order[1:]] - inter + 1e-6)
        order = order[1:][iou < iou_thr]
    return [boxes[k] for k in keep]


# ── Face detection ────────────────────────────────────────────────────────────

def _detect_yunet(frame, detector) -> list[dict]:
    """
    Detect faces with YuNet.
    Returns list of dicts: {cx, bbox:(x,y,w,h), mouth:(x,y,w,h), score}.
    YuNet output columns: x,y,w,h, re_x,re_y, le_x,le_y, nose_x,nose_y,
                          rcm_x,rcm_y, lcm_x,lcm_y, score
    """
    h, w = frame.shape[:2]
    detector.setInputSize((w, h))
    _, faces = detector.detect(frame)
    if faces is None or len(faces) == 0:
        return []

    result = []
    for f in faces:
        bx, by, bw, bh = int(f[0]), int(f[1]), int(f[2]), int(f[3])
        nose_x   = int(f[8])
        rcm_x, rcm_y = int(f[10]), int(f[11])
        lcm_x, lcm_y = int(f[12]), int(f[13])
        score = float(f[14])

        # Mouth bounding box from landmarks, expanded a little
        mx = max(0, min(rcm_x, lcm_x) - 4)
        my = max(0, min(rcm_y, lcm_y) - 6)
        mw = abs(lcm_x - rcm_x) + 8
        mh = max(12, abs(lcm_y - rcm_y) + 20)

        # Use nose x as the focal point — works correctly for profile faces
        # (where bbox center is off-center relative to the actual face).
        cx = nose_x if (0 < nose_x < w) else bx + bw // 2

        result.append({
            "cx": cx,
            "bbox": (bx, by, bw, bh),
            "mouth": (mx, my, mw, mh),
            "score": score,
        })
    return result


def _detect_cascade(gray, cascade_front, cascade_profile) -> list[dict]:
    """
    Detect faces with Haar cascades: frontal + profile (both directions).
    Returns list of dicts: {cx, bbox:(x,y,w,h), mouth:(x,y,w,h), score}.
    """
    h, w = gray.shape
    min_dim = max(30, min(w, h) // 10)
    kw = dict(scaleFactor=1.1, minNeighbors=4, minSize=(min_dim, min_dim))

    raw: list[tuple] = []

    # Frontal
    for (x, y, fw, fh) in cascade_front.detectMultiScale(gray, **kw):
        raw.append((int(x), int(y), int(fw), int(fh)))

    # Profile (looking left in image)
    if cascade_profile is not None:
        for (x, y, fw, fh) in cascade_profile.detectMultiScale(gray, **kw):
            raw.append((int(x), int(y), int(fw), int(fh)))

        # Profile (looking right): flip image, detect, mirror x back
        flipped = cv2.flip(gray, 1)
        for (x, y, fw, fh) in cascade_profile.detectMultiScale(flipped, **kw):
            raw.append((w - int(x) - int(fw), int(y), int(fw), int(fh)))

    deduped = _nms(raw)

    result = []
    for (x, y, fw, fh) in deduped:
        # Mouth region: lower 30% of face bbox
        my = y + int(fh * 0.65)
        mh = max(8, int(fh * 0.30))
        result.append({
            "cx": x + fw // 2,
            "bbox": (x, y, fw, fh),
            "mouth": (x, my, fw, mh),
            "score": 1.0,
        })
    return result


# ── Speaker selection ─────────────────────────────────────────────────────────

def _mouth_motion(gray_curr, gray_prev, mouth: tuple) -> float:
    mx, my, mw, mh = mouth
    if mw <= 0 or mh <= 0:
        return 0.0
    c = gray_curr[my:my + mh, mx:mx + mw]
    p = gray_prev[my:my + mh, mx:mx + mw]
    if c.size == 0 or c.shape != p.shape:
        return 0.0
    return float(cv2.absdiff(c, p).mean())


def _sharpness(gray, bbox: tuple) -> float:
    x, y, w, h = bbox
    roi = gray[y:y + h, x:x + w]
    if roi.size == 0:
        return 0.0
    return float(cv2.Laplacian(roi, cv2.CV_64F).var())


def pick_speaker_cx(faces: list[dict], gray_curr, gray_prev) -> int | None:
    """
    Choose the speaker face from a list of detections and return its center x.
    Priority: clear mouth-motion winner → sharpest face.
    """
    if not faces:
        return None
    if len(faces) == 1:
        return faces[0]["cx"]

    # Score by mouth motion when we have a previous frame
    if gray_prev is not None:
        scored = [
            (f, _mouth_motion(gray_curr, gray_prev, f["mouth"]))
            for f in faces
        ]
        best, best_score = max(scored, key=lambda t: t[1])
        rest_avg = (sum(s for _, s in scored) - best_score) / (len(scored) - 1)
        if best_score > 1.0 and best_score > rest_avg * 2.0:
            return best["cx"]

    # Fallback: sharpest face (usually the in-focus subject)
    best = max(faces, key=lambda f: _sharpness(gray_curr, f["bbox"]))
    return best["cx"]


# ── Analysis pass ─────────────────────────────────────────────────────────────

def analyze_faces(video_path: str, crop_w: int, src_w: int, fps: float) -> list[int]:
    """
    Sample frames to detect face positions.
    Returns a list of smoothed crop_x values (one per actual video frame).
    """
    # Open once to read a test frame for YuNet input-size init
    cap_tmp = cv2.VideoCapture(video_path)
    ret, test_frame = cap_tmp.read()
    cap_tmp.release()
    fh_px = test_frame.shape[0] if ret else 720
    fw_px = test_frame.shape[1] if ret else 1280

    # Try YuNet first, fall back to multi-cascade
    yunet = _load_yunet(fw_px, fh_px)
    if yunet is not None:
        print("[smart_crop] Detector: YuNet (frontal + profile)", file=sys.stderr)
    else:
        print("[smart_crop] Detector: Haar cascade (frontal + profile)", file=sys.stderr)

    cascade_front   = cv2.CascadeClassifier(cv2.data.haarcascades + "haarcascade_frontalface_default.xml")
    cascade_profile = cv2.CascadeClassifier(cv2.data.haarcascades + "haarcascade_profileface.xml")

    every = max(1, int(fps / 4))   # sample ~4 fps
    sample_fps = fps / every
    # Minimum sampled frames before allowing a face switch (~2.5 s at ~4 fps sampling)
    min_lock_samples = max(8, int(round(sample_fps * 2.5)))
    # cx must differ by >20% of frame width to be treated as a different face
    switch_dist = src_w * 0.20

    default_cx = src_w // 2
    last_cx    = default_cx
    prev_gray  = None
    raw_cx: list[int] = []

    locked_cx: int | None = None   # face center we are currently tracking
    lock_age:  int        = 0      # sampled frames spent on the current lock

    cap = cv2.VideoCapture(video_path)
    idx = 0

    while True:
        if idx % every == 0:
            ret, frame = cap.read()
            if not ret:
                break
            gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)

            if yunet is not None:
                faces = _detect_yunet(frame, yunet)
            else:
                faces = _detect_cascade(gray, cascade_front, cascade_profile)

            cx = pick_speaker_cx(faces, gray, prev_gray)
            if cx is not None:
                if locked_cx is None:
                    # First detection — establish lock immediately
                    locked_cx = cx
                    lock_age  = 0
                elif abs(cx - locked_cx) <= switch_dist:
                    # Same face region (speaker may have shifted slightly)
                    locked_cx = cx
                    lock_age += 1
                else:
                    # Different face region detected
                    if lock_age >= min_lock_samples:
                        # Lock held long enough — switch to new face
                        locked_cx = cx
                        lock_age  = 0
                    else:
                        # Too soon — stay on current face, ignore the switch
                        lock_age += 1
                last_cx = locked_cx
            else:
                # No face this sample — age the lock but hold position
                lock_age += 1

            prev_gray = gray
        else:
            if not cap.grab():
                break

        raw_cx.append(last_cx)
        idx += 1

    cap.release()

    if not raw_cx:
        return [max(0, default_cx - crop_w // 2)]

    arr = np.array(raw_cx, dtype=float)
    smoothed = _smooth_adaptive(arr, fps, src_w)
    min_cx = crop_w // 2
    max_cx = src_w - crop_w // 2
    return [int(np.clip(cx, min_cx, max_cx)) - crop_w // 2 for cx in smoothed]


# ── EMA smoothing ─────────────────────────────────────────────────────────────

def _smooth_adaptive(arr: np.ndarray, fps: float, src_w: int) -> np.ndarray:
    """
    EMA with two time constants:
    - Slow (1.5 s): smooth cinematic tracking for normal movement.
    - Fast (0.7 s): follows large speaker cuts (>25% frame width jump).
    Raising both constants from the original 0.5 s / 0.08 s makes the
    crop pan feel gradual rather than snapping.
    """
    if len(arr) == 0:
        return arr
    result = np.empty_like(arr)
    result[0] = arr[0]
    alpha_slow = 1.0 - np.exp(-1.0 / max(1.0, fps * 1.5))
    alpha_fast = 1.0 - np.exp(-1.0 / max(1.0, fps * 0.7))
    threshold = src_w * 0.25
    for i in range(1, len(arr)):
        alpha = alpha_fast if abs(arr[i] - result[i - 1]) > threshold else alpha_slow
        result[i] = alpha * arr[i] + (1.0 - alpha) * result[i - 1]
    return result.astype(int)


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Smart face-tracking crop")
    parser.add_argument("input",   help="Input video path")
    parser.add_argument("output",  help="Output video path")
    parser.add_argument("--ratio", default="9:16", help="Target aspect ratio (e.g. 9:16)")
    args = parser.parse_args()

    ffmpeg = os.environ.get("AUTOCLIPPER_FFMPEG", "ffmpeg")

    cap = cv2.VideoCapture(args.input)
    if not cap.isOpened():
        print(f"Error: tidak dapat membuka video: {args.input}", file=sys.stderr)
        sys.exit(1)

    src_w  = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    src_h  = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fps    = cap.get(cv2.CAP_PROP_FPS) or 30.0
    cap.release()

    aw, ah = [int(x) for x in args.ratio.split(":")]
    crop_w = min(src_w, int(src_h * aw / ah)) & ~1
    crop_h = src_h & ~1

    print(f"[smart_crop] {src_w}x{src_h} → {crop_w}x{crop_h} ({args.ratio})", file=sys.stderr)
    print("[smart_crop] Mendeteksi posisi pembicara...", file=sys.stderr)

    crop_x_list = analyze_faces(args.input, crop_w, src_w, fps)

    print(f"[smart_crop] Menerapkan crop ke {len(crop_x_list)} frame...", file=sys.stderr)

    ffmpeg_cmd = [
        ffmpeg, "-y",
        "-f", "rawvideo",
        "-pixel_format", "bgr24",
        "-video_size", f"{crop_w}x{crop_h}",
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
