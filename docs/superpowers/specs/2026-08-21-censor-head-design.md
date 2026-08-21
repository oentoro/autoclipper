# Sensor Kepala (Head Censor) — Design Spec
Date: 2026-08-21

## Overview

Tambah target sensor baru ke fitur sensor wajah yang sudah ada ([[2026-07-27-face-censor-design.md]], [[2026-08-06-face-censor-image-mode-design.md]]): selain "Wajah" (existing, deteksi via InsightFace/MediaPipe FaceDetection/YuNet/Haar), user bisa pilih "Kepala" — mensensor seluruh area kepala termasuk saat wajah tidak sepenuhnya kelihatan (menoleh, profil ekstrem, membelakangi kamera), selama badan/bahu orang tersebut masih ke-frame.

Mode blending (Mosaic/Gambar, dari spec sebelumnya) tetap independen dan tidak berubah — target menentukan **bbox apa yang disensor**, mode menentukan **cara menutupnya**. Kedua dimensi ini orthogonal.

## Scope

- Selector baru "Target Sensor" (Wajah / Kepala) di UI, tampil di dalam blok `censorFaces &&` yang sama, sejajar dengan selector Mosaic/Gambar yang sudah ada
- Default tetap "Wajah" — behavior existing tidak berubah kalau user tidak eksplisit pilih "Kepala"
- Target "Kepala" pakai detector baru: MediaPipe Tasks `PoseLandmarker` (multi-person, `num_poses` di-cap), bukan face detector chain yang ada — karena face detector by definition butuh wajah kelihatan, tidak cukup untuk "kepala sebagian/membelakangi"
- Bbox kepala diestimasi dari landmark pose (hidung/mata/telinga untuk skala + posisi, bahu untuk skala referensi ekspansi ke atas biar rambut ikut ter-cover)
- Kalau download/load `PoseLandmarker` gagal, fallback ke face-detector chain existing dengan padding diperbesar (0.6, dari 0.15 default) sebagai aproksimasi kasar area kepala — tetap tidak bisa cover kasus wajah 100% tidak kelihatan, tapi tidak block user

## Yang Tidak Termasuk

- Deteksi kepala tanpa badan sama sekali ke-frame (misal crop sangat ketat yang cuma nampilin kepala doang, tanpa bahu) — di luar jangkauan pose landmark, fallback ke face-chain expanded-padding kalau kasus ini terjadi dan wajah kebetulan kelihatan, atau tidak tersensor kalau wajah juga tidak kelihatan
- Tracking identity antar frame (sama seperti face censor — tiap frame dideteksi independen, tidak ada continuity/smoothing)
- Custom jumlah `num_poses` yang bisa diatur user — di-hardcode ke angka aman (lihat Arsitektur)
- Opsi selain MediaPipe Pose (YOLOv5-CrowdHuman, YOLOv8-person) — sudah dievaluasi dan ditolak karena lisensi (AGPL) atau hosting/lisensi model tidak stabil untuk closed-source app; lihat percakapan brainstorming

## Arsitektur

### 1. `scripts/smart_crop.py` — detector baru

Ditambahkan berdampingan dengan `_load_yunet`/`_detect_yunet` dkk (setelah blok YuNet, sebelum `_nms`):

```python
_POSE_URL = "https://storage.googleapis.com/mediapipe-models/pose_landmarker/pose_landmarker_lite/float16/latest/pose_landmarker_lite.task"
_POSE_FILENAME = "pose_landmarker_lite.task"
_POSE_MAX_PERSONS = 6  # cap wajar buat mosaic multi-speaker

def _pose_model_path() -> str:
    cache = os.path.expanduser("~/.cache/autoclipper")
    return os.path.join(cache, _POSE_FILENAME)

def _download_pose_model() -> bool:
    # pola identik _download_yunet(): skip kalau sudah ada, urlretrieve ke .tmp lalu rename,
    # catch Exception -> emit_status warning, return False
    ...

def _load_pose_landmarker():
    """Return mediapipe.tasks.vision.PoseLandmarker instance atau None."""
    try:
        import mediapipe as mp
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

def _detect_pose_heads(frame, landmarker) -> list[dict]:
    """
    Return list of dicts {bbox:(x,y,w,h), score} — satu entry per orang terdeteksi.
    Landmark index (MediaPipe Pose): 0 nose, 2 left_eye, 5 right_eye,
    7 left_ear, 8 right_ear, 11 left_shoulder, 12 right_shoulder.
    """
    import mediapipe as mp
    h, w = frame.shape[:2]
    mp_image = mp.Image(image_format=mp.ImageFormat.SRGB,
                         data=cv2.cvtColor(frame, cv2.COLOR_BGR2RGB))
    result = landmarker.detect(mp_image)
    if not result or not result.pose_landmarks:
        return []

    heads = []
    for lm in result.pose_landmarks:
        pts_visible = [(lm[i].x * w, lm[i].y * h) for i in (0, 2, 5, 7, 8)
                        if lm[i].visibility > 0.5]
        shoulder_pts = [(lm[i].x * w, lm[i].y * h) for i in (11, 12)
                         if lm[i].visibility > 0.5]
        if not pts_visible and not (len(shoulder_pts) == 2):
            continue  # ga cukup landmark buat estimasi

        shoulder_w = (abs(shoulder_pts[0][0] - shoulder_pts[1][0])
                      if len(shoulder_pts) == 2 else None)

        if pts_visible:
            xs = [p[0] for p in pts_visible]
            ys = [p[1] for p in pts_visible]
            cx, cy = sum(xs) / len(xs), sum(ys) / len(ys)
            scale = shoulder_w if shoulder_w else (max(xs) - min(xs) + 40)
        else:
            # cuma bahu kelihatan (membelakangi) — pusatkan kepala di atas bahu
            cx = sum(p[0] for p in shoulder_pts) / 2
            cy = min(p[1] for p in shoulder_pts) - shoulder_w * 0.6
            scale = shoulder_w

        half = max(20, scale * 0.7)
        bx = int(max(0, cx - half))
        by = int(max(0, cy - half * 1.3))  # bias ke atas buat rambut
        bw = int(min(w, cx + half) - bx)
        bh = int(min(h, cy + half * 0.9) - by)
        if bw > 0 and bh > 0:
            heads.append({"bbox": (bx, by, bw, bh), "score": 1.0})
    return heads
```

Catatan: konstanta rasio (`0.7`, `1.3`, `0.9`, `0.6`) hasil kalibrasi manual (bukan hasil training) — cukup buat cover kepala+rambut tanpa kegedean ke badan, di-tune lebih lanjut kalau perlu setelah lihat hasil video nyata.

### 2. `scripts/face_censor.py`

`main()`:
- Argumen baru `--target` dengan `choices=["face", "head"]`, default `"face"`
- Kalau `args.target == "head"`:
  - `pose_landmarker = _load_pose_landmarker()`
  - Kalau `None` (download/load gagal): `emit_status` warning + set flag fallback, pipeline lanjut pakai face-detector chain existing (`insight_app`/`mp_detector`/`yunet`/cascade) tapi `padding=0.6` di panggilan `pixelate_region`/`overlay_image_region` (bukan 0.15)
  - Kalau berhasil: di loop per-frame, ganti pemanggilan detector jadi `faces = _detect_pose_heads(frame, pose_landmarker)`, padding tetap 0.15 (bbox pose sudah mencakup area kepala penuh, tidak perlu padding besar tambahan)
- Kalau `args.target == "face"` (default): behavior persis seperti sekarang, tidak berubah

```python
padding = 0.15
if args.target == "head":
    pose_landmarker = _load_pose_landmarker()
    if pose_landmarker is None:
        emit_status("[face_censor] PoseLandmarker tidak tersedia, fallback ke deteksi wajah (padding diperbesar).")
        padding = 0.6
    else:
        emit_status("[face_censor] Detector: MediaPipe PoseLandmarker (target: kepala)")
else:
    pose_landmarker = None

...

while True:
    ret, frame = cap.read()
    if not ret:
        break

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

Detector face-chain (`insight_app`/`mp_detector`/`yunet`/cascade) tetap di-load lebih dulu seperti sekarang, dipakai baik sebagai default (`target=face`) maupun sebagai fallback (`target=head` tapi pose gagal load) — tidak ada load ganda yang sia-sia.

### 3. `scripts/test_face_censor.py`

Tambah test buat `_detect_pose_heads` (import dari `smart_crop`), pola sama (assert-based, no framework), pakai landmark palsu (bukan model asli — cukup object/list dengan atribut `.x .y .visibility` sesuai index yang dipakai):

- `test_pose_head_bbox_from_frontal_landmarks` — semua landmark relevan visible (nose/eyes/ears/shoulders) → bbox dihasilkan, posisinya di atas garis bahu
- `test_pose_head_bbox_from_shoulders_only` — cuma shoulder visible (nose/eyes/ears visibility rendah, simulasi membelakangi kamera) → bbox tetap dihasilkan (bukan list kosong), dipusatkan di atas titik tengah bahu
- `test_pose_head_returns_empty_when_no_landmarks_visible` — semua visibility rendah → return `[]`
- `test_pose_head_multi_person` — 2 set landmark (2 "orang") dalam satu `result.pose_landmarks` → return 2 bbox

### 4. `src-tauri/src/commands.rs`

- `exec_censor_faces` (baris ~2192): tambah parameter `target: Option<&str>`. Kalau `Some("head")`: `cmd.args(["--target", "head"])`. Kalau `None`/`Some("face")`: tidak menambah argumen (default script sudah `"face"`)
- `clip_video` (baris ~2258): tambah parameter `censor_target: String` (nilai `"face"` atau `"head"`, default dari frontend `"face"`)
- Di stage loop (baris ~2372-2374): teruskan `censor_target` ke `exec_censor_faces`:
  ```rust
  Stage::CensorFaces => {
      let censor_img = if censor_image_path.trim().is_empty() { None } else { Some(censor_image_path.as_str()) };
      let censor_tgt = if censor_target == "head" { Some("head") } else { None };
      exec_censor_faces(&app, &python, &ffmpeg, &current_path, &dest, censor_img, censor_tgt, pid_cell).await
  }
  ```

Tidak ada perubahan pada `Stage` enum atau urutan stage — target hanya mengubah argumen script, sama seperti mode gambar sebelumnya.

### 5. Frontend — `src/App.tsx`, `src/components/TranscriptView.tsx`, `src/i18n.tsx`

**State baru (App.tsx, dekat `censorMode`):**
```ts
const [censorTarget, setCensorTarget] = useState<"face" | "head">("face");
```

**Invoke (App.tsx, dekat baris 477-478):**
```ts
censorTarget,
```
(selalu dikirim, tidak kondisional ke `censorFaces` — backend abaikan kalau `censor_faces=false` karena stage `CensorFaces` tidak dijalankan sama sekali)

**UI (TranscriptView.tsx, di dalam blok `censorFaces &&`, sebelum atau sesudah selector Mosaic/Gambar yang sudah ada di baris ~658-671):**
- Selector 2 tombol "Wajah" / "Kepala", pola identik selector Mosaic/Gambar (`className="smart-crop-transition"`, tombol `transition-btn`), prop `censorTarget` + `onCensorTargetChange`
- Tidak ada interaksi khusus dengan selector Mosaic/Gambar — keduanya tampil bersamaan, independen

**Props baru di `TranscriptViewProps`:**
```ts
censorTarget: "face" | "head";
onCensorTargetChange: (v: "face" | "head") => void;
```

**i18n baru (id + en):**
- `censorTargetFace` / `censorTargetHead` — label 2 tombol ("Wajah" / "Kepala")

## Data Flow

```
User centang "Sensor Wajah" → pilih target "Kepala" → (opsional pilih mode Gambar juga)
  → invoke("clip_video", { ..., censorFaces: true, censorTarget: "head", censorImagePath: "..." })
    → commands.rs: exec_censor_faces(..., target: Some("head"), censor_image: ..., ...)
      → face_censor.py --target head [--censor-image ...]
        → _load_pose_landmarker() — download pose_landmarker_lite.task kalau belum ada
          → sukses: tiap frame → _detect_pose_heads → bbox kepala per orang
          → gagal: fallback face-chain existing, padding=0.6
        → tiap bbox: overlay_image_region atau pixelate_region (sama seperti target=face)
  → output_path = video dengan kepala tersensor
```

## Error Handling

- Download `.task` model gagal (offline / CDN unreachable) → `_download_pose_model()` return `False`, `emit_status` warning, fallback ke face-chain padding=0.6 — video tetap ter-render, tidak block user (konsisten dengan pola YuNet→cascade)
- Package `mediapipe` versi lama tanpa Tasks API (`mediapipe.tasks` tidak ada) → `ImportError` tertangkap di `_load_pose_landmarker`, sama-sama fallback ke face-chain
- Pose terdeteksi tapi tidak ada landmark kepala/bahu yang cukup visible (misal orang terpotong badan di tepi frame) → orang itu di-skip untuk frame tersebut (tidak tersensor), sama seperti face detector yang tidak temukan wajah — tidak dianggap error
- `num_poses` cap (6) terlampaui (>6 orang dalam satu frame) → PoseLandmarker hanya kembalikan sampai batas cap (confidence tertinggi), sisanya tidak tersensor — kasus jarang untuk konten short-clip, tidak ditangani khusus (dicatat di scope exclusion kalau perlu revisit)

## Catatan Performa

`PoseLandmarker` (varian `lite`) berjalan per-frame sama seperti face detector existing (bukan sampling seperti smart_crop.py's ~4fps) — konsisten dengan filosofi face_censor.py yang mementingkan ketepatan di atas performa untuk fitur privasi. Model `lite` dipilih spesifik karena ukuran kecil (~5MB) dan latency rendah dibanding varian `full`/`heavy`; kalau akurasi kurang di percobaan nyata, upgrade ke `pose_landmarker_full.task` adalah perubahan 1 baris (ganti `_POSE_URL`/`_POSE_FILENAME`).
