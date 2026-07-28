#!/usr/bin/env python3
"""Self-check for pick_speaker_cx continuity bias (assert-based, no framework)."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from smart_crop import pick_speaker_cx  # noqa: E402


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


if __name__ == "__main__":
    test_ambiguous_prefers_locked_face()
    test_no_lock_yet_falls_back_to_sharpest()
    test_single_face_shortcut()
    test_no_faces()
    print("OK: pick_speaker_cx continuity self-check passed")
