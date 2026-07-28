# Sensor Wajah (Face Censor) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambah opsi toggle "Sensor Wajah" saat export klip yang mem-pixelate SEMUA wajah terdeteksi di tiap frame video output.

**Architecture:** Script Python baru `scripts/face_censor.py` mengimpor fungsi loader/detector yang sudah ada di `scripts/smart_crop.py` (InsightFace > MediaPipe > YuNet > Haar cascade), mendeteksi wajah di SETIAP frame (tanpa sampling), lalu mem-pixelate tiap bbox wajah dan pipe hasil ke ffmpeg — pola identik dengan render loop `smart_crop.py`. Di sisi Rust, `clip_video` pipeline (`src-tauri/src/commands.rs`) yang tadinya `match (needs_burn, needs_smart)` dengan 4 cabang di-refactor jadi rantai stage linear (`concat → smart_crop? → censor_faces? → burn_subs?`) supaya nambah stage ketiga (censor) tidak meledak jadi 8 kombinasi. Frontend menambah toggle baru mirip pola `smartCrop` yang sudah ada.

**Tech Stack:** Python 3 + OpenCV (opencv-python) untuk deteksi & pixelate, Rust/Tauri untuk orkestrasi proses, React/TypeScript untuk UI.

## Global Constraints

- Sensor SEMUA wajah terdeteksi (termasuk speaker utama) — bukan cuma non-speaker.
- Gaya sensor: pixelate/mosaic saja (bukan gaussian blur, bukan kotak hitam).
- Toggle opsional di UI export, default `false`.
- Sensor wajah harus tetap bisa jalan walau smart crop OFF (independen).
- Kalau smart crop DAN sensor wajah sama-sama ON: urutan **crop dulu, baru sensor**.
- Deteksi wajah per-frame PENUH (bukan sample ~4fps seperti smart crop) — prioritas ketepatan di atas performa.
- Reuse fungsi detector yang sudah ada di `smart_crop.py`, jangan duplikat kode deteksi wajah.

---

## Task 1: `pixelate_region` — fungsi inti pixelate wajah

**Files:**
- Create: `scripts/face_censor.py` (isi sementara: cuma import + `pixelate_region`, `main()` menyusul di Task 2)
- Test: `scripts/test_face_censor.py`

**Interfaces:**
- Produces: `pixelate_region(frame: np.ndarray, bbox: tuple[int,int,int,int], padding: float = 0.15) -> np.ndarray` — bbox adalah `(x, y, w, h)` format yang sama dipakai di seluruh `smart_crop.py` (key `"bbox"` di tiap dict hasil `_detect_*`). Fungsi memodifikasi `frame` in-place DAN mengembalikannya (memudahkan chaining & testing).

- [ ] **Step 1: Buat file test yang gagal dulu**

Buat `scripts/test_face_censor.py`:

```python
#!/usr/bin/env python3
"""Self-check for pixelate_region (assert-based, no framework)."""
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
import numpy as np
from face_censor import pixelate_region  # noqa: E402


def test_pixelate_reduces_variance():
    rng = np.random.default_rng(42)
    frame = rng.integers(0, 256, size=(100, 100, 3), dtype=np.uint8)
    original_roi = frame[20:80, 20:80].copy()
    pixelate_region(frame, (20, 20, 60, 60), padding=0.0)
    result_roi = frame[20:80, 20:80]
    assert result_roi.astype(float).var() < original_roi.astype(float).var()


def test_pixelate_clamps_to_frame_edges():
    frame = np.zeros((50, 50, 3), dtype=np.uint8)
    result = pixelate_region(frame, (-10, -10, 30, 30), padding=0.2)
    assert result.shape == (50, 50, 3)


def test_padding_expands_processed_area():
    rng = np.random.default_rng(7)
    base = rng.integers(0, 256, size=(100, 100, 3), dtype=np.uint8)
    frame_no_pad = base.copy()
    frame_padded = base.copy()
    pixelate_region(frame_no_pad, (30, 30, 20, 20), padding=0.0)
    pixelate_region(frame_padded, (30, 30, 20, 20), padding=1.0)
    changed_no_pad = np.count_nonzero(np.any(frame_no_pad != base, axis=2))
    changed_padded = np.count_nonzero(np.any(frame_padded != base, axis=2))
    assert changed_padded > changed_no_pad


if __name__ == "__main__":
    test_pixelate_reduces_variance()
    test_pixelate_clamps_to_frame_edges()
    test_padding_expands_processed_area()
    print("OK: pixelate_region self-check passed")
```

- [ ] **Step 2: Jalankan test, pastikan gagal (face_censor.py belum ada)**

Run: `python3 scripts/test_face_censor.py`
Expected: `ModuleNotFoundError: No module named 'face_censor'`

- [ ] **Step 3: Buat `scripts/face_censor.py` dengan `pixelate_region`**

```python
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
```

- [ ] **Step 4: Jalankan test, pastikan lolos**

Run: `python3 scripts/test_face_censor.py`
Expected: `OK: pixelate_region self-check passed`

- [ ] **Step 5: Commit**

```bash
git add scripts/face_censor.py scripts/test_face_censor.py
git commit -m "feat: pixelate_region untuk sensor wajah"
```

---

## Task 2: CLI `main()` — deteksi tiap frame + render via ffmpeg

**Files:**
- Modify: `scripts/face_censor.py` (tambah `main()` + `if __name__ == "__main__"`)

**Interfaces:**
- Consumes: `pixelate_region` (Task 1), `emit_progress`/`emit_status`/`_load_insightface`/`_detect_insightface`/`_load_mediapipe`/`_detect_mediapipe`/`_load_yunet`/`_detect_yunet`/`_detect_cascade` (semua dari `smart_crop.py`, sudah diimpor di Task 1).
- Produces: CLI `python3 face_censor.py <input> <output>` — dipanggil dari Rust di Task 3 via `exec_censor_faces`.

- [ ] **Step 1: Tambah `main()` di akhir `scripts/face_censor.py`**

```python
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
```

- [ ] **Step 2: Verifikasi manual dengan video sample**

Run (pakai video pendek apa saja yang ada wajah, ganti path sesuai):
```bash
AUTOCLIPPER_FFMPEG=ffmpeg python3 scripts/face_censor.py /path/to/sample.mp4 /tmp/censored_output.mp4
```
Expected: proses selesai tanpa error, `PROGRESS:` dan status line muncul di stderr, `/tmp/censored_output.mp4` dihasilkan. Buka videonya — semua wajah harus kelihatan kotak-kotak pixelate, audio tetap ada.

- [ ] **Step 3: Commit**

```bash
git add scripts/face_censor.py
git commit -m "feat: main() face_censor.py — deteksi tiap frame + render ffmpeg"
```

---

## Task 3: Integrasi Rust — `exec_censor_faces` + refactor pipeline `clip_video`

**Files:**
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `find_script` (existing, `commands.rs:396`), pola `exec_smart_crop`/`exec_burn_subs` (existing, `commands.rs:2033`/`2101`) sebagai referensi struktur.
- Produces: `async fn exec_censor_faces(app: &tauri::AppHandle, python: &str, ffmpeg: &str, input: &str, output: &str, pid_cell: &Mutex<Option<u32>>) -> Result<(), String>`, parameter baru `censor_faces: bool` di command `clip_video`, event Tauri `"clip-censor-percent"` (payload `u8`, dipakai frontend di Task 4).

- [ ] **Step 1: Tambah `exec_censor_faces` — taruh persis sebelum `#[tauri::command] pub async fn clip_video` (baris ~2192)**

```rust
async fn exec_censor_faces(
    app: &tauri::AppHandle,
    python: &str,
    ffmpeg: &str,
    input: &str,
    output: &str,
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let script = find_script(app, "face_censor.py");
    let mut cmd = TokioCommand::new(python);
    cmd.args([&script, input, output])
        .env("AUTOCLIPPER_FFMPEG", ffmpeg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn()
        .map_err(|e| format!("Gagal menjalankan face_censor.py: {e}"))?;
    if let Some(pid) = child.id() { *pid_cell.lock().unwrap() = Some(pid); }

    let stderr_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let app_clone = app.clone();
        let buf = stderr_buf.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(pct_str) = line.strip_prefix("PROGRESS:") {
                    if let Ok(pct) = pct_str.trim().parse::<u8>() {
                        let _ = app_clone.emit("clip-censor-percent", pct);
                    }
                } else {
                    let mut g = buf.lock().unwrap();
                    g.push_str(&line);
                    g.push('\n');
                }
            }
        });
    }

    let output_r = child.wait_with_output().await
        .map_err(|e| format!("Gagal menunggu face_censor.py: {e}"))?;
    *pid_cell.lock().unwrap() = None;

    if output_r.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output_r.stdout);
        let stderr_captured = stderr_buf.lock().unwrap().clone();
        let src = if !stdout.trim().is_empty() { stdout.to_string() } else { stderr_captured };
        let err: serde_json::Value = serde_json::from_str(&src).unwrap_or_default();
        let msg = err["error"].as_str().map(|s| s.to_string())
            .unwrap_or_else(|| src.trim().to_string());
        Err(format!("Sensor wajah gagal: {msg}"))
    }
}
```

- [ ] **Step 2: Tambah parameter `censor_faces` ke signature `clip_video`**

Cari (baris ~2205-2206):
```rust
    smart_crop: bool,
    smart_crop_transition: String,
```
Ganti jadi:
```rust
    smart_crop: bool,
    smart_crop_transition: String,
    censor_faces: bool,
```

- [ ] **Step 3: Refactor pipeline — ganti blok `match (needs_burn, needs_smart) { ... }` (baris ~2274-2312) jadi rantai stage linear**

Cari blok ini (dari `let needs_burn = burn_subtitles || has_title;` sampai penutup `match`):
```rust
    let needs_burn = burn_subtitles || has_title;

    match (needs_burn, needs_smart) {
        // ── Case 1: no burn, no smart crop ────────────────────────────────
        (false, false) => {
            concat_groups(&app, &ffmpeg, &video_path, &groups, ffmpeg_crop.as_deref(), &output_path, pid_cell).await?;
        }

        // ── Case 2: burn (subtitle and/or title), no smart crop ───────────
        (true, false) => {
            let tmp = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            concat_groups(&app, &ffmpeg, &video_path, &groups, ffmpeg_crop.as_deref(), tmp.to_str().unwrap(), pid_cell).await?;
            let entries = if burn_subtitles { build_retimed_entries(&groups, eff_subtitle_mode, &original_by_index) } else { vec![] };
            let r = exec_burn_subs(&app, &python, &ffmpeg, &ffprobe, tmp.to_str().unwrap(), &output_path, entries, font_size, &font_path, &subtitle_style_json, eff_title, eff_title_fs, eff_title_color, pid_cell).await;
            let _ = std::fs::remove_file(&tmp);
            r?;
        }

        // ── Case 3: smart crop only, no burn ──────────────────────────────
        (false, true) => {
            let tmp = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            concat_groups(&app, &ffmpeg, &video_path, &groups, None, tmp.to_str().unwrap(), pid_cell).await?;
            let r = exec_smart_crop(&app, &python, &ffmpeg, tmp.to_str().unwrap(), &output_path, &aspect_ratio, &smart_crop_transition, pid_cell).await;
            let _ = std::fs::remove_file(&tmp);
            r?;
        }

        // ── Case 4: smart crop + burn ─────────────────────────────────────
        (true, true) => {
            let tmp_concat = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            let tmp_smart  = std::env::temp_dir().join("autoclipper_smart_tmp.mp4");
            concat_groups(&app, &ffmpeg, &video_path, &groups, None, tmp_concat.to_str().unwrap(), pid_cell).await?;
            let r = exec_smart_crop(&app, &python, &ffmpeg, tmp_concat.to_str().unwrap(), tmp_smart.to_str().unwrap(), &aspect_ratio, &smart_crop_transition, pid_cell).await;
            let _ = std::fs::remove_file(&tmp_concat);
            r?;
            let entries = if burn_subtitles { build_retimed_entries(&groups, eff_subtitle_mode, &original_by_index) } else { vec![] };
            let r = exec_burn_subs(&app, &python, &ffmpeg, &ffprobe, tmp_smart.to_str().unwrap(), &output_path, entries, font_size, &font_path, &subtitle_style_json, eff_title, eff_title_fs, eff_title_color, pid_cell).await;
            let _ = std::fs::remove_file(&tmp_smart);
            r?;
        }
    }
```

Ganti dengan:
```rust
    let needs_burn = burn_subtitles || has_title;
    let needs_censor = censor_faces;

    enum Stage { SmartCrop, CensorFaces, BurnSubs }
    let mut stage_list: Vec<Stage> = Vec::new();
    if needs_smart  { stage_list.push(Stage::SmartCrop); }
    if needs_censor { stage_list.push(Stage::CensorFaces); }
    if needs_burn   { stage_list.push(Stage::BurnSubs); }

    // concat_groups selalu jalan lebih dulu. Kalau smart crop TIDAK aktif,
    // crop filter (kalau ada) bisa langsung diselipkan di sini (satu pass);
    // kalau smart crop aktif, crop dilakukan belakangan oleh smart_crop.py
    // jadi concat di sini tidak boleh nge-crop (None).
    let concat_crop = if needs_smart { None } else { ffmpeg_crop.as_deref() };
    let concat_dest: String = if stage_list.is_empty() {
        output_path.clone()
    } else {
        std::env::temp_dir().join("autoclipper_concat_tmp.mp4").to_string_lossy().to_string()
    };
    concat_groups(&app, &ffmpeg, &video_path, &groups, concat_crop, &concat_dest, pid_cell).await?;

    let mut current_path = concat_dest;
    let last_idx = stage_list.len().saturating_sub(1);

    for (i, stage) in stage_list.iter().enumerate() {
        let dest: String = if i == last_idx {
            output_path.clone()
        } else {
            std::env::temp_dir().join(format!("autoclipper_stage{i}_tmp.mp4")).to_string_lossy().to_string()
        };

        let r: Result<(), String> = match stage {
            Stage::SmartCrop => exec_smart_crop(&app, &python, &ffmpeg, &current_path, &dest, &aspect_ratio, &smart_crop_transition, pid_cell).await,
            Stage::CensorFaces => exec_censor_faces(&app, &python, &ffmpeg, &current_path, &dest, pid_cell).await,
            Stage::BurnSubs => {
                let entries = if burn_subtitles { build_retimed_entries(&groups, eff_subtitle_mode, &original_by_index) } else { vec![] };
                exec_burn_subs(&app, &python, &ffmpeg, &ffprobe, &current_path, &dest, entries, font_size, &font_path, &subtitle_style_json, eff_title, eff_title_fs, eff_title_color, pid_cell).await
            }
        };

        let _ = std::fs::remove_file(&current_path);
        r?;
        current_path = dest;
    }
```

- [ ] **Step 4: Tambah catatan sensor wajah di pesan hasil (opsional tapi konsisten dengan pola `ar_note`/`sub_note`)**

Cari (setelah blok pipeline, sekitar baris ~2315-2318):
```rust
    let ar_note = if aspect_ratio != "original" {
        if needs_smart { format!(" [{aspect_ratio} smart]") } else { format!(" [{aspect_ratio}]") }
    } else { String::new() };
    let sub_note = if burn_subtitles { " + subtitle" } else { "" };
```
Ganti jadi:
```rust
    let ar_note = if aspect_ratio != "original" {
        if needs_smart { format!(" [{aspect_ratio} smart]") } else { format!(" [{aspect_ratio}]") }
    } else { String::new() };
    let sub_note = if burn_subtitles { " + subtitle" } else { "" };
    let censor_note = if needs_censor { " + sensor wajah" } else { "" };
```
Dan cari baris `format!(` yang memakai `{sub_note}` di `message:` (sekitar baris ~2326), tambahkan `{censor_note}` setelahnya:
```rust
            "Berhasil menggabungkan {total_segments} segmen{group_note} ({:.1}s){sub_note}{censor_note}{ar_note}",
```

- [ ] **Step 5: Compile check**

Run: `cd src-tauri && cargo check`
Expected: build sukses tanpa error (warning boleh ada kalau memang sudah ada sebelumnya).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat: exec_censor_faces + refactor pipeline clip_video jadi stage chain"
```

---

## Task 4: Frontend — toggle "Sensor Wajah"

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/TranscriptView.tsx`
- Modify: `src/i18n.tsx`

**Interfaces:**
- Consumes: Tauri command `clip_video` param `censorFaces: boolean` (Task 3), event `"clip-censor-percent"` (payload `number`, Task 3).
- Produces: state `censorFaces` di `App.tsx`, prop `censorFaces`/`onCensorFacesChange` di `TranscriptView.tsx`.

- [ ] **Step 1: Tambah state di `App.tsx`**

Cari (baris 137-138):
```tsx
  const [smartCrop, setSmartCrop] = useState<boolean>(false);
  const [smartCropTransition, setSmartCropTransition] = useState<"smooth" | "aggressive">("smooth");
```
Ganti jadi:
```tsx
  const [smartCrop, setSmartCrop] = useState<boolean>(false);
  const [smartCropTransition, setSmartCropTransition] = useState<"smooth" | "aggressive">("smooth");
  const [censorFaces, setCensorFaces] = useState<boolean>(false);
```

- [ ] **Step 2: Tambah listener event progress di `handleClip()` (`App.tsx`)**

Cari (baris ~444-451):
```tsx
    const unlistenBurn = await listen<number>("clip-burn-percent", event => {
      const pct = event.payload;
      if (pct >= 100) {
        setLoadingMsg(t("clippingFinalizing"));
      } else {
        setLoadingMsg(t("clippingBurn", { pct }));
      }
    });
```
Tambahkan tepat setelahnya:
```tsx
    const unlistenCensor = await listen<number>("clip-censor-percent", event => {
      const pct = event.payload;
      if (pct >= 100) {
        setLoadingMsg(t("clippingFinalizing"));
      } else {
        setLoadingMsg(t("clippingCensor", { pct }));
      }
    });
```

- [ ] **Step 3: Kirim `censorFaces` ke command dan cleanup listener**

Cari (baris ~462-464, di dalam `invoke<ClipResult>("clip_video", {...})`):
```tsx
        aspectRatio,
        smartCrop,
        smartCropTransition,
```
Ganti jadi:
```tsx
        aspectRatio,
        smartCrop,
        smartCropTransition,
        censorFaces,
```

Cari blok `finally` (baris ~478-486):
```tsx
    } finally {
      unlistenConcat();
      unlistenSmart();
      unlistenBurn();
      setLoading(false);
```
Ganti jadi:
```tsx
    } finally {
      unlistenConcat();
      unlistenSmart();
      unlistenBurn();
      unlistenCensor();
      setLoading(false);
```

- [ ] **Step 4: Teruskan prop ke `TranscriptView` di `App.tsx`**

Cari (baris ~760-763):
```tsx
            smartCrop={smartCrop}
            onSmartCropChange={setSmartCrop}
            smartCropTransition={smartCropTransition}
            onSmartCropTransitionChange={setSmartCropTransition}
```
Ganti jadi:
```tsx
            smartCrop={smartCrop}
            onSmartCropChange={setSmartCrop}
            smartCropTransition={smartCropTransition}
            onSmartCropTransitionChange={setSmartCropTransition}
            censorFaces={censorFaces}
            onCensorFacesChange={setCensorFaces}
```

- [ ] **Step 5: Tambah prop ke interface & destructure di `TranscriptView.tsx`**

Cari (baris 43-45, interface `Props`):
```tsx
  onSmartCropChange: (v: boolean) => void;
  smartCropTransition: "smooth" | "aggressive";
  onSmartCropTransitionChange: (v: "smooth" | "aggressive") => void;
```
Ganti jadi:
```tsx
  onSmartCropChange: (v: boolean) => void;
  smartCropTransition: "smooth" | "aggressive";
  onSmartCropTransitionChange: (v: "smooth" | "aggressive") => void;
  censorFaces: boolean;
  onCensorFacesChange: (v: boolean) => void;
```

Cari (baris 138-140, destructure di komponen):
```tsx
  onSmartCropChange,
  smartCropTransition,
  onSmartCropTransitionChange,
```
Ganti jadi:
```tsx
  onSmartCropChange,
  smartCropTransition,
  onSmartCropTransitionChange,
  censorFaces,
  onCensorFacesChange,
```

- [ ] **Step 6: Tambah blok checkbox UI di `TranscriptView.tsx`, setelah blok Smart Crop**

Cari penutup blok Smart Crop (baris ~618-619):
```tsx
          </div>
        )}

        {/* Subtitle */}
```
Ganti jadi (tambahkan blok baru sebelum komentar `{/* Subtitle */}`, TIDAK dibungkus kondisi `aspectRatio` karena sensor wajah independen dari smart crop/aspect ratio):
```tsx
          </div>
        )}

        {/* Sensor Wajah — independen dari smart crop dan aspect ratio */}
        <div className="sidebar-block smart-crop-block">
          <label className="smart-crop-toggle">
            <input
              type="checkbox"
              checked={censorFaces}
              onChange={e => onCensorFacesChange(e.target.checked)}
            />
            <span className="smart-crop-label">
              <span className="smart-crop-icon">🙈</span>
              {t("censorFacesLabel")}
              <span className="smart-crop-hint">{t("censorFacesHint")}</span>
            </span>
          </label>
        </div>

        {/* Subtitle */}
```

- [ ] **Step 7: Tambah string i18n**

Di `src/i18n.tsx`, cari blok `id` (baris 55-56):
```tsx
    clippingFaces: "Mendeteksi wajah... {pct}%",
    clippingSmartCrop: "Menerapkan smart crop... {pct}%",
```
Ganti jadi:
```tsx
    clippingFaces: "Mendeteksi wajah... {pct}%",
    clippingSmartCrop: "Menerapkan smart crop... {pct}%",
    clippingCensor: "Mensensor wajah... {pct}%",
```

Cari blok `id` di sekitar `smartCropLabel`/`smartCropHint` (baris 110-111):
```tsx
    smartCropLabel: "Smart Crop",
    smartCropHint: "Ikuti pembicara",
```
Ganti jadi:
```tsx
    smartCropLabel: "Smart Crop",
    smartCropHint: "Ikuti pembicara",
    censorFacesLabel: "Sensor Wajah",
    censorFacesHint: "Blur semua wajah",
```

Cari blok `en` yang sama (baris 245-246):
```tsx
    clippingFaces: "Detecting faces... {pct}%",
    clippingSmartCrop: "Applying smart crop... {pct}%",
```
Ganti jadi:
```tsx
    clippingFaces: "Detecting faces... {pct}%",
    clippingSmartCrop: "Applying smart crop... {pct}%",
    clippingCensor: "Censoring faces... {pct}%",
```

Cari blok `en` di sekitar `smartCropLabel`/`smartCropHint` (baris 291-292):
```tsx
    smartCropLabel: "Smart Crop",
    smartCropHint: "Follow speaker",
```
Ganti jadi:
```tsx
    smartCropLabel: "Smart Crop",
    smartCropHint: "Follow speaker",
    censorFacesLabel: "Censor Faces",
    censorFacesHint: "Blur all faces",
```

- [ ] **Step 8: Type-check frontend**

Run: `npm run build`
Expected: `tsc` dan `vite build` sukses tanpa error TypeScript (khususnya tidak ada prop/key yang hilang di `TranscriptView` atau `dict`).

- [ ] **Step 9: Manual check di dev server**

Run: `npm run dev` (dan `cargo tauri dev` kalau mau full app), buka halaman transkrip, cek checkbox baru "Sensor Wajah" muncul di sidebar (selalu muncul, tidak tergantung aspect ratio), toggle-nya berfungsi.

- [ ] **Step 10: Commit**

```bash
git add src/App.tsx src/components/TranscriptView.tsx src/i18n.tsx
git commit -m "feat: toggle Sensor Wajah di UI export"
```
