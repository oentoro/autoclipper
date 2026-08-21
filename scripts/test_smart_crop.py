#!/usr/bin/env python3
"""Self-check for pick_speaker_cx continuity bias (assert-based, no framework)."""
import sys
import os
from types import SimpleNamespace

sys.path.insert(0, os.path.dirname(__file__))
from smart_crop import (  # noqa: E402
    pick_speaker_cx,
    _select_insightface_providers,
    _head_bbox_from_landmarks,
)


def face(cx):
    return {"cx": cx, "bbox": (cx - 10, 10, 20, 20), "mouth": (cx - 5, 25, 10, 5), "score": 1.0}


def test_ambiguous_prefers_locked_face():
    faces = [face(100), face(400)]
    # No gray frames -> mouth-motion scoring skipped -> ambiguous fallback.
    assert pick_speaker_cx(faces, None, None, locked_cx=110) == 100
    assert pick_speaker_cx(faces, None, None, locked_cx=390) == 400


def test_no_lock_yet_falls_back_to_sharpest():
    import numpy as np
    sharp = np.zeros((40, 40), dtype=np.uint8)
    sharp[::2, :] = 255  # high-frequency stripes -> high Laplacian variance
    flat = np.zeros((40, 40), dtype=np.uint8)
    gray = np.zeros((60, 460), dtype=np.uint8)
    gray[10:50, 90:130] = sharp
    gray[10:50, 390:430] = flat
    faces = [face(100), face(400)]
    assert pick_speaker_cx(faces, gray, None, locked_cx=None) == 100


def test_single_face_shortcut():
    assert pick_speaker_cx([face(250)], None, None, locked_cx=999) == 250


def test_no_faces():
    assert pick_speaker_cx([], None, None, locked_cx=100) is None


def test_select_insightface_providers_prefers_cuda():
    ctx_id, label, providers = _select_insightface_providers(
        ["CUDAExecutionProvider", "CoreMLExecutionProvider", "CPUExecutionProvider"]
    )
    assert ctx_id == 0
    assert label == "CUDA GPU"
    assert providers is None


def test_select_insightface_providers_falls_back_to_coreml():
    ctx_id, label, providers = _select_insightface_providers(
        ["CoreMLExecutionProvider", "AzureExecutionProvider", "CPUExecutionProvider"]
    )
    assert ctx_id == 0
    assert label == "CoreML"
    assert providers == ["CoreMLExecutionProvider", "CPUExecutionProvider"]


def test_select_insightface_providers_falls_back_to_cpu():
    ctx_id, label, providers = _select_insightface_providers(
        ["AzureExecutionProvider", "CPUExecutionProvider"]
    )
    assert ctx_id == -1
    assert label == "CPU"
    assert providers is None


def _landmarks(overrides: dict) -> list:
    """33 landmark palsu, default invisible; override index tertentu dengan (x, y, visibility)."""
    base = [SimpleNamespace(x=0.5, y=0.5, visibility=0.0) for _ in range(33)]
    for idx, (x, y, vis) in overrides.items():
        base[idx] = SimpleNamespace(x=x, y=y, visibility=vis)
    return base


def test_head_bbox_from_frontal_landmarks():
    lm = _landmarks({
        0:  (0.50, 0.20, 1.0),  # nose
        2:  (0.47, 0.19, 1.0),  # left_eye
        5:  (0.53, 0.19, 1.0),  # right_eye
        7:  (0.45, 0.20, 1.0),  # left_ear
        8:  (0.55, 0.20, 1.0),  # right_ear
        11: (0.40, 0.40, 1.0),  # left_shoulder
        12: (0.60, 0.40, 1.0),  # right_shoulder
    })
    result = _head_bbox_from_landmarks(lm, 1000, 1000)
    assert result is not None
    bx, by, bw, bh = result["bbox"]
    assert bw > 0 and bh > 0
    assert by + bh <= 500  # bbox kepala berhenti jauh di atas garis bahu (y=400)


def test_head_bbox_from_shoulders_only():
    # Simulasi orang membelakangi kamera: cuma bahu yang visible.
    lm = _landmarks({
        11: (0.40, 0.40, 1.0),
        12: (0.60, 0.40, 1.0),
    })
    result = _head_bbox_from_landmarks(lm, 1000, 1000)
    assert result is not None
    bx, by, bw, bh = result["bbox"]
    center_x = bx + bw / 2
    assert 300 < center_x < 700  # dipusatkan di antara 2 bahu
    assert by < 400  # bbox mulai di atas garis bahu


def test_head_bbox_returns_none_when_nothing_visible():
    lm = _landmarks({})
    assert _head_bbox_from_landmarks(lm, 1000, 1000) is None


if __name__ == "__main__":
    test_ambiguous_prefers_locked_face()
    test_no_lock_yet_falls_back_to_sharpest()
    test_single_face_shortcut()
    test_no_faces()
    test_select_insightface_providers_prefers_cuda()
    test_select_insightface_providers_falls_back_to_coreml()
    test_select_insightface_providers_falls_back_to_cpu()
    test_head_bbox_from_frontal_landmarks()
    test_head_bbox_from_shoulders_only()
    test_head_bbox_returns_none_when_nothing_visible()
    print("OK: pick_speaker_cx + head-bbox self-check passed")
