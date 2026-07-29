#!/usr/bin/env python3
"""
Smart vertical crop — tracks the speaker's face horizontally.

Detection strategy (in order of preference):
  1. MediaPipe Face Detection — deep-learning model, lowest false-positive
     rate, no separate download required (bundled with the mediapipe package).
     Pip: pip install mediapipe
  2. YuNet (cv2.FaceDetectorYN) — handles frontal + profile (0°–90°).
     Model ~340 KB, auto-downloaded once to ~/.cache/autoclipper/.
  3. Multi-cascade fallback — haarcascade frontal + profile (both directions).
     Bundled with opencv-python; no download needed but highest false-positive
     rate — last resort only.

Speaker selection (when multiple faces are visible):
  - Mouth-motion scoring via inter-frame pixel diff on the mouth region.
  - Sharpness fallback for frames with no speech.
"""

import sys
import os
import json
import subprocess
import argparse
import urllib.request

try:
    import cv2
    import numpy as np
except ImportError as _e:
    os.write(2, (f"Error: {_e}\nSmart Crop membutuhkan opencv-python.\n"
                 "Jalankan: python3 -m pip install opencv-python\n").encode("utf-8"))
    os.write(1, (json.dumps({"error": str(_e)}) + "\n").encode("utf-8"))
    sys.exit(1)


def emit_progress(pct: int) -> None:
    try:
        os.write(2, f"PROGRESS:{min(100, max(0, pct))}\n".encode("ascii"))
    except OSError:
        pass


def emit_status(msg: str) -> None:
    try:
        os.write(2, (msg + "\n").encode("utf-8", errors="replace"))
    except OSError:
        pass


# ── InsightFace SCRFD (frontal + profil 0°–90°+) ─────────────────────────────

def _select_insightface_providers(available_providers: list) -> tuple:
    """
    Pick (ctx_id, device_label, providers) for InsightFace given the list of
    onnxruntime execution providers available on this machine. Priority:
    CUDA (Nvidia GPU) > CoreML (Apple Neural Engine/GPU) > CPU.
    `providers` is None when relying on ctx_id alone (CUDA/CPU — insightface's
    own ctx_id-to-provider mapping already handles these); it's an explicit
    provider list only for CoreML, which has no device-index concept.
    """
    if "CUDAExecutionProvider" in available_providers:
        return 0, "CUDA GPU", None
    if "CoreMLExecutionProvider" in available_providers:
        return 0, "CoreML", ["CoreMLExecutionProvider", "CPUExecutionProvider"]
    return -1, "CPU", None


def _load_insightface():
    """Return (FaceAnalysis app, device_label) or (None, None) if unavailable."""
    try:
        from insightface.app import FaceAnalysis

        ctx_id, device_label, providers = -1, "CPU", None
        try:
            import onnxruntime as ort
            ctx_id, device_label, providers = _select_insightface_providers(ort.get_available_providers())
        except Exception:
            pass

        kwargs = {"name": "buffalo_sc", "allowed_modules": ["detection"]}
        if providers is not None:
            kwargs["providers"] = providers
        app = FaceAnalysis(**kwargs)
        app.prepare(ctx_id=ctx_id, det_size=(640, 640))
        return app, device_label
    except Exception:
        return None, None


def _detect_insightface(frame, app) -> list[dict]:
    """
    Detect faces with InsightFace SCRFD.
    Keypoint order (buffalo_sc / SCRFD): left_eye, right_eye, nose_tip,
                                          left_mouth, right_mouth.
    Returns same dict format: {cx, bbox:(x,y,w,h), mouth:(x,y,w,h), score}.
    """
    h, w = frame.shape[:2]
    faces = app.get(frame)  # InsightFace uses BGR natively
    if not faces:
        return []

    result = []
    for face in faces:
        x1, y1, x2, y2 = [int(v) for v in face.bbox]
        bx, by = max(0, x1), max(0, y1)
        bw = max(1, x2 - x1)
        bh = max(1, y2 - y1)
        score = float(face.det_score)

        kps = face.kps  # (5, 2): left_eye, right_eye, nose, left_mouth, right_mouth
        if kps is not None and len(kps) >= 5:
            nose_x = int(kps[2][0])
            lm = kps[3]   # left mouth corner
            rm = kps[4]   # right mouth corner

            # Estimate face orientation: ratio of nose offset from bbox center
            # relative to half-width. 0 = fully frontal, ~1 = full profile.
            profile_ratio = abs(nose_x - (bx + bw / 2.0)) / (bw / 2.0 + 1e-6)

            if profile_ratio > 0.30:
                # Profile face: landmark-based mouth region is unreliable (two
                # corners project nearly on top of each other). Use the lower
                # face / jaw region instead — the jaw drop during speech is the
                # dominant visible motion from the side.
                mx = bx
                my = max(0, by + int(bh * 0.55))
                mw = bw
                mh = max(20, int(bh * 0.40))
            else:
                # Frontal / slight tilt: precise landmark-based mouth region.
                mx = max(0, int(min(lm[0], rm[0])) - 8)
                my = max(0, int(min(lm[1], rm[1])) - 8)
                mw = max(16, int(abs(rm[0] - lm[0])) + 16)
                mh = max(14, int(abs(rm[1] - lm[1])) + 22)
        else:
            nose_x = bx + bw // 2
            mx, my, mw, mh = bx, by + int(bh * 0.65), bw, max(8, int(bh * 0.30))

        cx = nose_x if (0 < nose_x < w) else bx + bw // 2
        result.append({
            "cx":    cx,
            "bbox":  (bx, by, bw, bh),
            "mouth": (mx, my, mw, mh),
            "score": score,
        })
    return result


# ── MediaPipe face detection ──────────────────────────────────────────────────

def _load_mediapipe():
    """
    Return an initialised MediaPipe FaceDetection object, or None if the
    package is not installed.  model_selection=1 covers distances > 2 m
    (better for most video content than the short-range model 0).
    """
    try:
        import mediapipe as mp
        detector = mp.solutions.face_detection.FaceDetection(
            model_selection=1,
            min_detection_confidence=0.5,
        )
        return detector
    except Exception:
        return None


def _detect_mediapipe(frame, detector) -> list[dict]:
    """
    Detect faces with MediaPipe and return the same dict format as the other
    detectors: {cx, bbox:(x,y,w,h), mouth:(x,y,w,h), score}.

    MediaPipe keypoints (FaceDetection short/full-range):
      0 right_eye  1 left_eye  2 nose_tip  3 mouth_center
      4 right_ear_tragion  5 left_ear_tragion
    """
    import mediapipe as mp
    h, w = frame.shape[:2]
    rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
    results = detector.process(rgb)
    if not results or not results.detections:
        return []

    faces = []
    for det in results.detections:
        box = det.location_data.relative_bounding_box
        bx = max(0, int(box.xmin * w))
        by = max(0, int(box.ymin * h))
        bw = max(1, int(box.width  * w))
        bh = max(1, int(box.height * h))

        kp = det.location_data.relative_keypoints
        # Use nose tip as focal x (index 2) — accurate even for slight profiles
        nose_x = int(kp[2].x * w) if len(kp) > 2 else bx + bw // 2
        # Mouth region centred on mouth keypoint (index 3)
        if len(kp) > 3:
            m_cx = int(kp[3].x * w)
            m_cy = int(kp[3].y * h)
        else:
            m_cx = bx + bw // 2
            m_cy = by + int(bh * 0.75)
        m_half_w = max(8, bw // 3)
        m_half_h = max(6, bh // 8)
        mx = max(0, m_cx - m_half_w)
        my = max(0, m_cy - m_half_h)

        score = det.score[0] if det.score else 1.0
        cx = nose_x if 0 < nose_x < w else bx + bw // 2

        faces.append({
            "cx":    cx,
            "bbox":  (bx, by, bw, bh),
            "mouth": (mx, my, m_half_w * 2, m_half_h * 2),
            "score": score,
        })
    return faces


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
        emit_status("[smart_crop] Mengunduh model face detection YuNet (~340 KB)...")
        urllib.request.urlretrieve(_YUNET_URL, path + ".tmp")
        os.rename(path + ".tmp", path)
        emit_status("[smart_crop] Model YuNet berhasil diunduh.")
        return True
    except Exception as e:
        emit_status(f"[smart_crop] Download YuNet gagal ({e}), pakai cascade fallback.")
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

    cuda_ok = hasattr(cv2, "cuda") and cv2.cuda.getCudaEnabledDeviceCount() > 0
    backend = cv2.dnn.DNN_BACKEND_CUDA if cuda_ok else cv2.dnn.DNN_BACKEND_DEFAULT
    target  = cv2.dnn.DNN_TARGET_CUDA  if cuda_ok else cv2.dnn.DNN_TARGET_CPU

    try:
        det = cv2.FaceDetectorYN.create(
            _yunet_model_path(), "",
            (frame_w, frame_h),
            score_threshold=0.55,
            nms_threshold=0.3,
            top_k=100,
            backend_id=backend,
            target_id=target,
        )
        return det
    except Exception:
        # CUDA backend gagal (opencv tanpa CUDA) — fallback ke CPU
        try:
            return cv2.FaceDetectorYN.create(
                _yunet_model_path(), "",
                (frame_w, frame_h),
                score_threshold=0.55,
                nms_threshold=0.3,
                top_k=100,
            )
        except Exception as e:
            emit_status(f"[smart_crop] YuNet load gagal ({e}), pakai cascade fallback.")
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
    diff = cv2.absdiff(c, p).astype(np.float32)
    # Gradient-weighted diff: amplifies motion at lip/jaw edges, suppresses
    # flat background noise. x-gradient catches jaw shift (profile view);
    # y-gradient catches lip opening (frontal view).
    gx = cv2.Sobel(c, cv2.CV_32F, 1, 0, ksize=3)
    gy = cv2.Sobel(c, cv2.CV_32F, 0, 1, ksize=3)
    grad_mag = np.sqrt(gx * gx + gy * gy)
    mean_grad = float(grad_mag.mean())
    if mean_grad > 1.0:
        weight = np.clip(grad_mag / (mean_grad + 1e-6), 0.5, 2.5)
        return float((diff * weight).mean())
    return float(diff.mean())


def _sharpness(gray, bbox: tuple) -> float:
    x, y, w, h = bbox
    roi = gray[y:y + h, x:x + w]
    if roi.size == 0:
        return 0.0
    return float(cv2.Laplacian(roi, cv2.CV_64F).var())


def pick_speaker_cx(faces: list[dict], gray_curr, gray_prev,
                     locked_cx: int | None = None) -> int | None:
    """
    Choose the speaker face from a list of detections and return its center x.
    Priority: clear mouth-motion winner → face closest to the current lock
    (continuity) → sharpest face (only when there's no lock yet).
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

    # Ambiguous motion signal (two speakers close together and both quiet
    # or both moving) — stay on whoever is currently locked instead of
    # guessing. The old sharpest-face tiebreak has nothing to do with who's
    # talking and flips noisily frame-to-frame between similarly-sharp
    # faces, which oscillated the crop between the two speakers and made
    # the EMA smoothing converge toward the frame's center.
    if locked_cx is not None:
        return min(faces, key=lambda f: abs(f["cx"] - locked_cx))["cx"]

    best = max(faces, key=lambda f: _sharpness(gray_curr, f["bbox"]))
    return best["cx"]


# ── Analysis pass ─────────────────────────────────────────────────────────────

def analyze_faces(video_path: str, crop_w: int, src_w: int, fps: float,
                  transition: str = "smooth") -> list[int]:
    """
    Sample frames to detect face positions.
    Returns a list of smoothed crop_x values (one per actual video frame).

    transition="smooth"     — slow cinematic pan; holds lock for ~2.5 s.
    transition="aggressive" — snaps instantly; switches face after ~0.5 s.
    """
    # Open once to read a test frame for YuNet input-size init
    cap_tmp = cv2.VideoCapture(video_path)
    ret, test_frame = cap_tmp.read()
    total_frames = max(1, int(cap_tmp.get(cv2.CAP_PROP_FRAME_COUNT)))
    cap_tmp.release()
    fh_px = test_frame.shape[0] if ret else 720
    fw_px = test_frame.shape[1] if ret else 1280

    # Detector priority: InsightFace > MediaPipe > YuNet > Haar cascade
    insight_app, insight_device = _load_insightface()
    mp_detector  = None
    yunet        = None
    cuda_yunet   = False
    if insight_app is not None:
        emit_status(f"[smart_crop] Detector: InsightFace SCRFD — {insight_device} (frontal + profil 0°–90°)")
    else:
        mp_detector = _load_mediapipe()
        if mp_detector is not None:
            emit_status("[smart_crop] Detector: MediaPipe — CPU (deep learning, presisi tinggi)")
        else:
            yunet = _load_yunet(fw_px, fh_px)
            if yunet is not None:
                cuda_yunet = hasattr(cv2, "cuda") and cv2.cuda.getCudaEnabledDeviceCount() > 0
                device_label = "CUDA GPU" if cuda_yunet else "CPU"
                emit_status(f"[smart_crop] Detector: YuNet — {device_label} (frontal + profil)")
            else:
                emit_status("[smart_crop] Detector: Haar cascade — CPU (fallback) — install insightface untuk presisi lebih baik")

    cascade_front   = cv2.CascadeClassifier(cv2.data.haarcascades + "haarcascade_frontalface_default.xml")
    cascade_profile = cv2.CascadeClassifier(cv2.data.haarcascades + "haarcascade_profileface.xml")

    every = max(1, int(fps / 4))   # sample ~4 fps
    sample_fps = fps / every
    if transition == "aggressive":
        # Switch face after ~0.5 s; respond to smaller positional shifts (15 %)
        min_lock_samples = max(2, int(round(sample_fps * 0.5)))
        switch_dist = src_w * 0.15
    else:
        # Smooth: hold lock for ~2.5 s; only switch on large positional shift (20 %)
        min_lock_samples = max(8, int(round(sample_fps * 2.5)))
        switch_dist = src_w * 0.20

    default_cx = src_w // 2
    last_cx    = default_cx
    prev_gray  = None
    raw_cx: list[int] = []

    locked_cx: int | None = None   # face center we are currently tracking
    lock_age:  int        = 0      # sampled frames spent on the current lock

    cap = cv2.VideoCapture(video_path)
    idx = 0
    progress_interval = max(1, total_frames // 55)  # ~55 updates in 0-55% range
    emit_progress(0)

    while True:
        if idx % every == 0:
            ret, frame = cap.read()
            if not ret:
                break
            gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)

            if insight_app is not None:
                faces = _detect_insightface(frame, insight_app)
            elif mp_detector is not None:
                faces = _detect_mediapipe(frame, mp_detector)
            elif yunet is not None:
                faces = _detect_yunet(frame, yunet)
            else:
                faces = _detect_cascade(gray, cascade_front, cascade_profile)

            cx = pick_speaker_cx(faces, gray, prev_gray, locked_cx)
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
                # No face detected — hold last position AND reset lock_age.
                # Resetting ensures that when a face reappears (possibly at a
                # different / center position), it must accumulate min_lock_samples
                # fresh detections before we switch.  Without this reset the
                # pre-gap lock_age would satisfy min_lock_samples immediately,
                # causing the crop to snap to center the instant any face returns.
                lock_age = 0

            prev_gray = gray
        else:
            if not cap.grab():
                break

        raw_cx.append(last_cx)
        idx += 1
        if idx % progress_interval == 0:
            emit_progress(min(55, int(idx / total_frames * 55)))

    cap.release()
    if mp_detector is not None:
        try:
            mp_detector.close()
        except Exception:
            pass

    if not raw_cx:
        return [max(0, default_cx - crop_w // 2)]

    arr = np.array(raw_cx, dtype=float)
    smoothed = _smooth_adaptive(arr, fps, src_w, mode=transition)
    min_cx = crop_w // 2
    max_cx = src_w - crop_w // 2
    return [int(np.clip(cx, min_cx, max_cx)) - crop_w // 2 for cx in smoothed]


# ── EMA smoothing ─────────────────────────────────────────────────────────────

def _smooth_adaptive(arr: np.ndarray, fps: float, src_w: int,
                     mode: str = "smooth") -> np.ndarray:
    """
    EMA with two time constants, selected by mode:

    smooth     — cinematic pan. Slow constant (1.5 s) for normal movement,
                 fast constant (0.7 s) kicks in only for large jumps (>25 %).
                 The camera glides between positions.

    aggressive — hard cut on large jumps (alpha=1, instant), very light
                 smoothing (0.1 s) for micro-jitter within the same face.
                 Threshold is 10 % so even medium re-frames snap immediately.
    """
    if len(arr) == 0:
        return arr
    result = np.empty_like(arr, dtype=float)
    result[0] = arr[0]

    if mode == "aggressive":
        alpha_slow = 1.0 - np.exp(-1.0 / max(1.0, fps * 0.1))  # 0.1 s — micro-jitter only
        alpha_fast = 1.0                                          # instant snap
        threshold  = src_w * 0.10                                 # 10 % triggers snap
    else:  # smooth
        alpha_slow = 1.0 - np.exp(-1.0 / max(1.0, fps * 1.5))
        alpha_fast = 1.0 - np.exp(-1.0 / max(1.0, fps * 0.7))
        threshold  = src_w * 0.25

    for i in range(1, len(arr)):
        alpha = alpha_fast if abs(arr[i] - result[i - 1]) > threshold else alpha_slow
        result[i] = alpha * arr[i] + (1.0 - alpha) * result[i - 1]
    return result.astype(int)


# ── Main ──────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Smart face-tracking crop")
    parser.add_argument("input",   help="Input video path")
    parser.add_argument("output",  help="Output video path")
    parser.add_argument("--ratio",      default="9:16",   help="Target aspect ratio (e.g. 9:16)")
    parser.add_argument("--transition", default="smooth",
                        choices=["smooth", "aggressive"],
                        help="smooth = cinematic pan; aggressive = instant snap")
    args = parser.parse_args()

    ffmpeg = os.environ.get("AUTOCLIPPER_FFMPEG", "ffmpeg")

    cap = cv2.VideoCapture(args.input)
    if not cap.isOpened():
        os.write(1, (json.dumps({"error": f"tidak dapat membuka video: {args.input}"}) + "\n").encode("utf-8"))
        sys.exit(1)

    src_w  = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    src_h  = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fps    = cap.get(cv2.CAP_PROP_FPS) or 30.0
    cap.release()

    aw, ah = [int(x) for x in args.ratio.split(":")]
    crop_w = min(src_w, int(src_h * aw / ah)) & ~1
    crop_h = src_h & ~1

    emit_status(f"[smart_crop] {src_w}x{src_h} → {crop_w}x{crop_h} ({args.ratio})")
    emit_status("[smart_crop] Mendeteksi posisi pembicara...")

    crop_x_list = analyze_faces(args.input, crop_w, src_w, fps, transition=args.transition)

    emit_status(f"[smart_crop] Menerapkan crop ke {len(crop_x_list)} frame...")
    emit_progress(56)

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

    proc = subprocess.Popen(ffmpeg_cmd, stdin=subprocess.PIPE, stderr=subprocess.DEVNULL)

    cap = cv2.VideoCapture(args.input)
    frame_idx = 0
    last_x = crop_x_list[-1] if crop_x_list else 0
    total_render = len(crop_x_list) or 1
    render_interval = max(1, total_render // 40)

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
        if frame_idx % render_interval == 0:
            emit_progress(56 + min(43, int(frame_idx / total_render * 43)))

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
    emit_status("[smart_crop] Selesai.")


if __name__ == "__main__":
    main()
