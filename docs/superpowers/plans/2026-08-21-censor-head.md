# Sensor Kepala (Head Censor) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambah target "Kepala" ke fitur Sensor Wajah — saat dipilih, seluruh area kepala (termasuk saat wajah tidak sepenuhnya kelihatan: menoleh, profil ekstrem, membelakangi kamera) disensor, bukan cuma area wajah.

**Architecture:** `scripts/smart_crop.py` dapat detector baru berbasis MediaPipe Tasks `PoseLandmarker` (multi-person) — bbox kepala diestimasi dari landmark hidung/mata/telinga/bahu, bukan dari face detector yang butuh wajah kelihatan. `scripts/face_censor.py` dapat argumen CLI `--target face|head` (default `face`, backward compatible); saat `head`, detector diganti ke pose landmarker, dengan fallback ke face-detector chain existing (padding diperbesar) kalau model pose gagal dimuat. `commands.rs` meneruskan target sebagai argumen tambahan ke script yang sama, tanpa mengubah struktur stage pipeline. Frontend dapat state target (`face`/`head`), UI selector mirror pola mosaic/gambar yang sudah ada (reuse class CSS, tidak perlu style baru).

**Tech Stack:** Python 3 (OpenCV, MediaPipe Tasks API), Rust (Tauri 2.x, tokio), React/TypeScript.

**Spec:** `docs/superpowers/specs/2026-08-21-censor-head-design.md`

## Global Constraints

- Target default tetap `"face"` — behavior existing (`censor_faces=true`, tanpa target eksplisit) tidak berubah
- Model PoseLandmarker: `pose_landmarker_lite.task`, download sekali dari `https://storage.googleapis.com/mediapipe-models/pose_landmarker/pose_landmarker_lite/float16/latest/pose_landmarker_lite.task`, cache di `~/.cache/autoclipper/` (pola identik `_download_yunet`)
- `num_poses` di-cap ke 6 (cukup untuk mosaic multi-speaker, tidak user-configurable)
- Kalau download/load PoseLandmarker gagal → fallback diam-diam ke face-detector chain existing dengan `padding=0.6` (bukan hard error)
- Mode blending (Mosaic/Gambar, dari spec sebelumnya) tetap orthogonal — target menentukan bbox, mode menentukan cara menutup, keduanya independen
- Reference spec: `docs/superpowers/specs/2026-08-21-censor-head-design.md`

---

### Task 1: Pose head-bbox detector di `smart_crop.py`

**Files:**
- Modify: `scripts/smart_crop.py` (tambah setelah `_load_yunet`, sebelum komentar `# ── Non-max suppression`, baris ~289-292)
- Test: `scripts/test_smart_crop.py`

**Interfaces:**
- Produces:
  - `_load_pose_landmarker()` → `mediapipe.tasks.python.vision.PoseLandmarker` instance atau `None`
  - `_head_bbox_from_landmarks(landmarks, w: int, h: int) -> dict | None` — `landmarks` adalah sequence 33 objek dengan atribut `.x .y .visibility` (format sama seperti `PoseLandmarkerResult.pose_landmarks[i]`). Return `{"bbox": (x,y,w,h), "score": 1.0}` atau `None`
  - `_detect_pose_heads(frame, landmarker) -> list[dict]` — list `{"bbox": ..., "score": ...}`, satu per orang terdeteksi

- [ ] **Step 1: Tambah test ke `scripts/test_smart_crop.py`**

Ubah baris 1-7 dari:
```python
#!/usr/bin/env python3
"""Self-check for pick_speaker_cx continuity bias (assert-based, no framework)."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from smart_crop import pick_speaker_cx, _select_insightface_providers  # noqa: E402
```
jadi:
```python
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
```

Tambahkan setelah `test_select_insightface_providers_falls_back_to_cpu` (sebelum blok `if __name__ == "__main__":`):
```python
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
```

Ubah blok `if __name__ == "__main__":` (di ujung file) dari:
```python
if __name__ == "__main__":
    test_ambiguous_prefers_locked_face()
    test_no_lock_yet_falls_back_to_sharpest()
    test_single_face_shortcut()
    test_no_faces()
    test_select_insightface_providers_prefers_cuda()
    test_select_insightface_providers_falls_back_to_coreml()
    test_select_insightface_providers_falls_back_to_cpu()
    print("OK: pick_speaker_cx continuity self-check passed")
```
jadi:
```python
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
```

- [ ] **Step 2: Jalankan test, verify FAIL**

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/test_smart_crop.py`
Expected: `ImportError: cannot import name '_head_bbox_from_landmarks'`

- [ ] **Step 3: Implementasi detector di `scripts/smart_crop.py`**

Cari blok ini di `_load_yunet` (fungsi existing berakhir dengan):
```python
        except Exception as e:
            emit_status(f"[smart_crop] YuNet load gagal ({e}), pakai cascade fallback.")
            return None


# ── Non-max suppression ───────────────────────────────────────────────────────
```

Sisipkan blok baru di antara `return None` dan komentar `# ── Non-max suppression`:
```python
        except Exception as e:
            emit_status(f"[smart_crop] YuNet load gagal ({e}), pakai cascade fallback.")
            return None


# ── Pose-based head detection ─────────────────────────────────────────────────

_POSE_URL = (
    "https://storage.googleapis.com/mediapipe-models/pose_landmarker/"
    "pose_landmarker_lite/float16/latest/pose_landmarker_lite.task"
)
_POSE_FILENAME = "pose_landmarker_lite.task"
_POSE_MAX_PERSONS = 6  # cap wajar buat mosaic multi-speaker


def _pose_model_path() -> str:
    cache = os.path.expanduser("~/.cache/autoclipper")
    return os.path.join(cache, _POSE_FILENAME)


def _download_pose_model() -> bool:
    path = _pose_model_path()
    if os.path.exists(path):
        return True
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        emit_status("[smart_crop] Mengunduh model PoseLandmarker (~5 MB)...")
        urllib.request.urlretrieve(_POSE_URL, path + ".tmp")
        os.rename(path + ".tmp", path)
        emit_status("[smart_crop] Model PoseLandmarker berhasil diunduh.")
        return True
    except Exception as e:
        emit_status(f"[smart_crop] Download PoseLandmarker gagal ({e}), pakai fallback.")
        if os.path.exists(path + ".tmp"):
            try:
                os.remove(path + ".tmp")
            except OSError:
                pass
        return False


def _load_pose_landmarker():
    """Return mediapipe.tasks.python.vision.PoseLandmarker instance, atau None."""
    try:
        from mediapipe.tasks import python as mp_python
        from mediapipe.tasks.python import vision
    except Exception:
        return None
    if not _download_pose_model():
        return None
    try:
        base_options = mp_python.BaseOptions(model_asset_path=_pose_model_path())
        options = vision.PoseLandmarkerOptions(
            base_options=base_options,
            running_mode=vision.RunningMode.IMAGE,
            num_poses=_POSE_MAX_PERSONS,
            min_pose_detection_confidence=0.5,
        )
        return vision.PoseLandmarker.create_from_options(options)
    except Exception as e:
        emit_status(f"[smart_crop] PoseLandmarker load gagal ({e}), pakai fallback.")
        return None


# Landmark index (MediaPipe Pose, 33 titik):
# 0 nose, 2 left_eye, 5 right_eye, 7 left_ear, 8 right_ear,
# 11 left_shoulder, 12 right_shoulder
_POSE_FACE_IDX = (0, 2, 5, 7, 8)
_POSE_SHOULDER_IDX = (11, 12)


def _head_bbox_from_landmarks(landmarks, w: int, h: int):
    """
    landmarks: sequence 33 objek dengan atribut .x .y .visibility (normalized 0..1),
    format sama seperti PoseLandmarkerResult.pose_landmarks[i].
    Return {"bbox": (x, y, w, h), "score": 1.0} atau None kalau landmark ga cukup.
    """
    pts_visible = [(landmarks[i].x * w, landmarks[i].y * h) for i in _POSE_FACE_IDX
                   if landmarks[i].visibility > 0.5]
    shoulder_pts = [(landmarks[i].x * w, landmarks[i].y * h) for i in _POSE_SHOULDER_IDX
                    if landmarks[i].visibility > 0.5]

    if not pts_visible and len(shoulder_pts) < 2:
        return None

    shoulder_w = abs(shoulder_pts[0][0] - shoulder_pts[1][0]) if len(shoulder_pts) == 2 else None

    if pts_visible:
        xs = [p[0] for p in pts_visible]
        ys = [p[1] for p in pts_visible]
        cx, cy = sum(xs) / len(xs), sum(ys) / len(ys)
        scale = shoulder_w if shoulder_w else (max(xs) - min(xs) + 40)
    else:
        cx = (shoulder_pts[0][0] + shoulder_pts[1][0]) / 2
        cy = min(shoulder_pts[0][1], shoulder_pts[1][1]) - shoulder_w * 0.6
        scale = shoulder_w

    half = max(20, scale * 0.7)
    bx = int(max(0, cx - half))
    by = int(max(0, cy - half * 1.3))
    bw = int(min(w, cx + half) - bx)
    bh = int(min(h, cy + half * 0.9) - by)
    if bw <= 0 or bh <= 0:
        return None
    return {"bbox": (bx, by, bw, bh), "score": 1.0}


def _detect_pose_heads(frame, landmarker) -> list[dict]:
    """Return list of {"bbox": (x,y,w,h), "score": float}, satu per orang terdeteksi."""
    import mediapipe as mp
    h, w = frame.shape[:2]
    mp_image = mp.Image(image_format=mp.ImageFormat.SRGB,
                         data=cv2.cvtColor(frame, cv2.COLOR_BGR2RGB))
    result = landmarker.detect(mp_image)
    if not result or not result.pose_landmarks:
        return []
    heads = []
    for landmarks in result.pose_landmarks:
        head = _head_bbox_from_landmarks(landmarks, w, h)
        if head is not None:
            heads.append(head)
    return heads


# ── Non-max suppression ───────────────────────────────────────────────────────
```

- [ ] **Step 4: Jalankan test, verify PASS**

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/test_smart_crop.py`
Expected: `OK: pick_speaker_cx + head-bbox self-check passed`

- [ ] **Step 5: Commit**

```bash
git add scripts/smart_crop.py scripts/test_smart_crop.py
git commit -m "feat: tambah pose-based head detector buat sensor kepala"
```

---

### Task 2: Wire `--target` CLI arg ke `face_censor.py`

**Files:**
- Modify: `scripts/face_censor.py`

**Interfaces:**
- Consumes: `_load_pose_landmarker()`, `_detect_pose_heads(frame, landmarker)` dari Task 1
- Produces: CLI `face_censor.py <input> <output> [--censor-image PATH] [--target face|head]`

- [ ] **Step 1: Tambah import**

Ubah baris 18-28 dari:
```python
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
```
jadi:
```python
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
    _load_pose_landmarker,
    _detect_pose_heads,
)
```

- [ ] **Step 2: Tambah argumen CLI `--target`**

Ubah baris 104-108 dari:
```python
    parser = argparse.ArgumentParser(description="Face censor — pixelate atau tutup gambar semua wajah terdeteksi")
    parser.add_argument("input",  help="Input video path")
    parser.add_argument("output", help="Output video path")
    parser.add_argument("--censor-image", default=None, help="Path gambar buat nutup wajah (opsional, default: mosaic)")
    args = parser.parse_args()
```
jadi:
```python
    parser = argparse.ArgumentParser(description="Face censor — pixelate atau tutup gambar semua wajah/kepala terdeteksi")
    parser.add_argument("input",  help="Input video path")
    parser.add_argument("output", help="Output video path")
    parser.add_argument("--censor-image", default=None, help="Path gambar buat nutup wajah (opsional, default: mosaic)")
    parser.add_argument("--target", choices=["face", "head"], default="face", help="Target sensor: 'face' (default) atau 'head'")
    args = parser.parse_args()
```

- [ ] **Step 3: Load pose landmarker saat `target=head`**

Ubah baris 126-140 dari:
```python
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
```
jadi:
```python
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

    padding = 0.15
    pose_landmarker = None
    if args.target == "head":
        pose_landmarker = _load_pose_landmarker()
        if pose_landmarker is None:
            emit_status("[face_censor] PoseLandmarker tidak tersedia, fallback ke deteksi wajah (padding diperbesar).")
            padding = 0.6
        else:
            emit_status("[face_censor] Detector: MediaPipe PoseLandmarker (target: kepala)")
```

- [ ] **Step 4: Pakai `pose_landmarker` di loop deteksi + `padding` dinamis**

Ubah baris 182-196 dari:
```python
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
            if overlay_img is not None:
                overlay_image_region(frame, f["bbox"], overlay_img, padding=0.15)
            else:
                pixelate_region(frame, f["bbox"], padding=0.15)
```
jadi:
```python
        if pose_landmarker is not None:
            faces = _detect_pose_heads(frame, pose_landmarker)
        elif insight_app is not None:
            faces = _detect_insightface(frame, insight_app)
        elif mp_detector is not None:
            faces = _detect_mediapipe(frame, mp_detector)
        elif yunet is not None:
            faces = _detect_yunet(frame, yunet)
        else:
            gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY)
            faces = _detect_cascade(gray, cascade_front, cascade_profile)

        for f in faces:
            if overlay_img is not None:
                overlay_image_region(frame, f["bbox"], overlay_img, padding=padding)
            else:
                pixelate_region(frame, f["bbox"], padding=padding)
```

- [ ] **Step 5: Verify — syntax check + regression test + CLI help**

Run: `cd /Users/oentoro/Projects/autoclipper && python3 -c "import ast; ast.parse(open('scripts/face_censor.py').read())" && echo "syntax OK"`
Expected: `syntax OK`

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/test_face_censor.py`
Expected: `OK: pixelate_region + overlay_image_region self-check passed` (regresi tetap lolos, test ini tidak menyentuh target/pose)

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/face_censor.py --help`
Expected: help text menampilkan `--target {face,head}` sebagai opsi

- [ ] **Step 6: Commit**

```bash
git add scripts/face_censor.py
git commit -m "feat: wire --target head CLI arg ke face_censor.py main()"
```

---

### Task 3: Rust — teruskan target ke `face_censor.py`

**Files:**
- Modify: `src-tauri/src/commands.rs` — `exec_censor_faces` (baris ~2192-2255), `clip_video` signature (baris ~2258-2279), pemanggilan stage (baris ~2372-2375)

**Interfaces:**
- Consumes: CLI `--target face|head` dari Task 2
- Produces: `clip_video` command menerima parameter baru `censor_target: String` dari frontend (Task 4 mengirim ini)

- [ ] **Step 1: Tambah parameter `censor_target` ke `exec_censor_faces`**

Ubah signature dari:
```rust
async fn exec_censor_faces(
    app: &tauri::AppHandle,
    python: &str,
    ffmpeg: &str,
    input: &str,
    output: &str,
    censor_image: Option<&str>,
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
```
jadi:
```rust
async fn exec_censor_faces(
    app: &tauri::AppHandle,
    python: &str,
    ffmpeg: &str,
    input: &str,
    output: &str,
    censor_image: Option<&str>,
    censor_target: Option<&str>,
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
```

Ubah pembangunan command dari:
```rust
    let script = find_script(app, "face_censor.py");
    let mut cmd = TokioCommand::new(python);
    cmd.args([&script, input, output]);
    if let Some(img) = censor_image {
        cmd.args(["--censor-image", img]);
    }
    cmd.env("AUTOCLIPPER_FFMPEG", ffmpeg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
```
jadi:
```rust
    let script = find_script(app, "face_censor.py");
    let mut cmd = TokioCommand::new(python);
    cmd.args([&script, input, output]);
    if let Some(img) = censor_image {
        cmd.args(["--censor-image", img]);
    }
    if let Some(target) = censor_target {
        cmd.args(["--target", target]);
    }
    cmd.env("AUTOCLIPPER_FFMPEG", ffmpeg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
```

- [ ] **Step 2: Tambah parameter `censor_target` ke `clip_video`**

Ubah signature `clip_video`, tambahkan setelah `censor_image_path: String,`:
```rust
    censor_faces: bool,
    censor_image_path: String,
    censor_target: String,
```

- [ ] **Step 3: Update pemanggilan stage**

Ubah:
```rust
            Stage::CensorFaces => {
                let censor_img = if censor_image_path.trim().is_empty() { None } else { Some(censor_image_path.as_str()) };
                exec_censor_faces(&app, &python, &ffmpeg, &current_path, &dest, censor_img, pid_cell).await
            }
```
jadi:
```rust
            Stage::CensorFaces => {
                let censor_img = if censor_image_path.trim().is_empty() { None } else { Some(censor_image_path.as_str()) };
                let censor_tgt = if censor_target == "head" { Some("head") } else { None };
                exec_censor_faces(&app, &python, &ffmpeg, &current_path, &dest, censor_img, censor_tgt, pid_cell).await
            }
```

- [ ] **Step 4: Verify — cargo check**

Run: `cd /Users/oentoro/Projects/autoclipper/src-tauri && cargo check`
Expected: compiles clean, no errors baru (warning existing boleh)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat: teruskan censor_target dari clip_video ke exec_censor_faces"
```

---

### Task 4: Frontend — state + invoke wiring di `App.tsx`

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `clip_video` Tauri command dengan parameter baru `censorTarget: string` dari Task 3
- Produces: props `censorTarget`, `onCensorTargetChange` yang dikonsumsi `TranscriptView` di Task 5

- [ ] **Step 1: Tambah state**

Ubah baris 139-141 dari:
```tsx
  const [censorFaces, setCensorFaces] = useState<boolean>(false);
  const [censorMode, setCensorMode] = useState<"mosaic" | "image">("mosaic");
  const [censorImagePath, setCensorImagePath] = useState<string>("");
```
jadi:
```tsx
  const [censorFaces, setCensorFaces] = useState<boolean>(false);
  const [censorMode, setCensorMode] = useState<"mosaic" | "image">("mosaic");
  const [censorImagePath, setCensorImagePath] = useState<string>("");
  const [censorTarget, setCensorTarget] = useState<"face" | "head">("face");
```

- [ ] **Step 2: Kirim ke invoke**

Ubah baris 477-478 dari:
```tsx
        censorFaces,
        censorImagePath: censorFaces && censorMode === "image" ? censorImagePath : "",
```
jadi:
```tsx
        censorFaces,
        censorImagePath: censorFaces && censorMode === "image" ? censorImagePath : "",
        censorTarget,
```

- [ ] **Step 3: Teruskan props ke `TranscriptView`**

Ubah baris 779-784 dari:
```tsx
            censorFaces={censorFaces}
            onCensorFacesChange={setCensorFaces}
            censorMode={censorMode}
            onCensorModeChange={setCensorMode}
            censorImagePath={censorImagePath}
            onCensorImagePathChange={setCensorImagePath}
```
jadi:
```tsx
            censorFaces={censorFaces}
            onCensorFacesChange={setCensorFaces}
            censorMode={censorMode}
            onCensorModeChange={setCensorMode}
            censorImagePath={censorImagePath}
            onCensorImagePathChange={setCensorImagePath}
            censorTarget={censorTarget}
            onCensorTargetChange={setCensorTarget}
```

- [ ] **Step 4: Verify — typecheck**

Run: `cd /Users/oentoro/Projects/autoclipper && npx tsc --noEmit`
Expected: error soal props `TranscriptView` belum menerima `censorTarget`/`onCensorTargetChange` (karena Task 5 belum dikerjakan) — expected di titik ini, lanjut ke Task 5 sebelum verify ulang.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx
git commit -m "feat: state censorTarget di App.tsx"
```

---

### Task 5: Frontend — UI selector Wajah/Kepala di `TranscriptView.tsx`

**Files:**
- Modify: `src/components/TranscriptView.tsx`
- Modify: `src/i18n.tsx`

**Interfaces:**
- Consumes: props `censorTarget`, `onCensorTargetChange` dari Task 4
- Produces: UI selesai, tidak ada task selanjutnya yang bergantung ini

- [ ] **Step 1: Tambah string i18n**

Di `src/i18n.tsx`, dictionary `id` — ubah baris 113-114 dari:
```tsx
    censorFacesLabel: "Sensor Wajah",
    censorFacesHint: "Pixelate semua wajah",
```
jadi:
```tsx
    censorFacesLabel: "Sensor Wajah",
    censorFacesHint: "Pixelate semua wajah",
    censorTargetFace: "Wajah",
    censorTargetHead: "Kepala",
```

Di dictionary `en` — ubah baris 301-302 dari:
```tsx
    censorFacesLabel: "Censor Faces",
    censorFacesHint: "Pixelate all faces",
```
jadi:
```tsx
    censorFacesLabel: "Censor Faces",
    censorFacesHint: "Pixelate all faces",
    censorTargetFace: "Face",
    censorTargetHead: "Head",
```

- [ ] **Step 2: Tambah props ke interface**

Ubah baris 51-52 dari:
```tsx
  censorImagePath: string;
  onCensorImagePathChange: (v: string) => void;
```
jadi:
```tsx
  censorImagePath: string;
  onCensorImagePathChange: (v: string) => void;
  censorTarget: "face" | "head";
  onCensorTargetChange: (v: "face" | "head") => void;
```

- [ ] **Step 3: Destructure props**

Ubah baris 152-153 dari:
```tsx
  censorImagePath,
  onCensorImagePathChange,
```
jadi:
```tsx
  censorImagePath,
  onCensorImagePathChange,
  censorTarget,
  onCensorTargetChange,
```

- [ ] **Step 4: Tambah UI selector**

Ubah blok Sensor Wajah dari:
```tsx
          {censorFaces && (
            <>
              <div className="smart-crop-transition">
                {([
                  { value: "mosaic", labelKey: "censorModeMosaic" },
                  { value: "image",  labelKey: "censorModeImage"  },
                ] as const).map(opt => (
                  <button
                    key={opt.value}
                    className={`transition-btn ${censorMode === opt.value ? "active" : ""}`}
                    onClick={() => onCensorModeChange(opt.value)}
                  >
                    {t(opt.labelKey)}
                  </button>
                ))}
              </div>
```
jadi:
```tsx
          {censorFaces && (
            <>
              <div className="smart-crop-transition">
                {([
                  { value: "face", labelKey: "censorTargetFace" },
                  { value: "head", labelKey: "censorTargetHead" },
                ] as const).map(opt => (
                  <button
                    key={opt.value}
                    className={`transition-btn ${censorTarget === opt.value ? "active" : ""}`}
                    onClick={() => onCensorTargetChange(opt.value)}
                  >
                    {t(opt.labelKey)}
                  </button>
                ))}
              </div>
              <div className="smart-crop-transition">
                {([
                  { value: "mosaic", labelKey: "censorModeMosaic" },
                  { value: "image",  labelKey: "censorModeImage"  },
                ] as const).map(opt => (
                  <button
                    key={opt.value}
                    className={`transition-btn ${censorMode === opt.value ? "active" : ""}`}
                    onClick={() => onCensorModeChange(opt.value)}
                  >
                    {t(opt.labelKey)}
                  </button>
                ))}
              </div>
```

(Reuse class `smart-crop-transition`/`transition-btn` yang sudah ada — tidak perlu CSS baru, sudah men-style baris tombol berulang seperti selector smart-crop-transition/mosaic-gambar.)

- [ ] **Step 5: Verify — typecheck**

Run: `cd /Users/oentoro/Projects/autoclipper && npx tsc --noEmit`
Expected: no errors

- [ ] **Step 6: Verify — dev server manual check**

Run: `cd /Users/oentoro/Projects/autoclipper && npm run tauri dev` (atau `npm run dev` kalau cuma perlu preview frontend)

Manual: buka app → step transcript → centang "Sensor Wajah" → muncul 2 baris tombol: "Wajah/Kepala" lalu "Mosaic/Gambar" → klik "Kepala" → tombol aktif berpindah, tidak mengganggu selector Mosaic/Gambar di bawahnya → jalankan clip dengan video ada orang menoleh/membelakangi kamera → area kepala tersensor (bukan cuma area wajah).

- [ ] **Step 7: Commit**

```bash
git add src/components/TranscriptView.tsx src/i18n.tsx
git commit -m "feat: UI selector wajah/kepala buat target sensor"
```

---

## Self-Review Notes

- Spec coverage: detector pose multi-person ✅ (Task 1), estimasi bbox dari landmark wajah atau bahu-saja ✅ (Task 1, `_head_bbox_from_landmarks`), CLI `--target` default `face` ✅ (Task 2), fallback ke face-chain padding 0.6 saat pose gagal load ✅ (Task 2 Step 3), UI selector independen dari mode Mosaic/Gambar ✅ (Task 5), default target tetap `face` (backward compatible) ✅ (Task 4 state default + Task 3 `censor_target == "head"` check).
- Semua task saling terhubung lewat interface eksplisit: `_load_pose_landmarker`/`_detect_pose_heads`/`_head_bbox_from_landmarks` (Task1→2), `--target` CLI arg (Task2→3), `censor_target: String` (Task3↔4), props `TranscriptView` (Task4↔5).
- Tidak ada placeholder — semua step berisi kode konkret siap tempel.
- Type consistency: `"face" | "head"` dipakai konsisten di state App.tsx (Task4), props TranscriptView (Task5), dan `choices=["face", "head"]` di Python argparse (Task2) — tidak ada mismatch nama.
