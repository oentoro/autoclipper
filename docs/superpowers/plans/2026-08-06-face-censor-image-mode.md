# Sensor Wajah — Mode Gambar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tambah mode "Gambar" ke fitur Sensor Wajah — user pilih 1 gambar (sticker/logo) yang menutup semua wajah terdeteksi, sebagai alternatif mode Mosaic (existing, tetap default).

**Architecture:** `scripts/face_censor.py` dapat fungsi baru `overlay_image_region` (pasangan `pixelate_region` yang sudah ada) + argumen CLI opsional `--censor-image`. Rust `commands.rs` meneruskan path gambar (kalau ada) sebagai argumen tambahan ke script yang sama, tanpa mengubah struktur stage pipeline. Frontend dapat state mode (`mosaic`/`image`) + path gambar, UI selector mirror pola smart-crop-transition yang sudah ada, plus file-picker pakai `@tauri-apps/plugin-dialog`.

**Tech Stack:** Python 3 (OpenCV), Rust (Tauri 2.x, tokio), React/TypeScript, `@tauri-apps/plugin-dialog`.

## Global Constraints

- Mode default tetap Mosaic — behavior existing (`censor_faces=true`, tanpa gambar) tidak berubah
- 1 gambar global untuk semua wajah di video (bukan per-wajah)
- Gambar di-stretch ke bbox+padding 15% (sama seperti area pixelate existing)
- PNG dengan alpha channel → alpha-composite; gambar opaque → full replace
- Gagal load gambar → fallback diam-diam ke mosaic (bukan hard error)
- Reference spec: `docs/superpowers/specs/2026-08-06-face-censor-image-mode-design.md`

---

### Task 1: `overlay_image_region` — pixelate pairing function di Python

**Files:**
- Modify: `scripts/face_censor.py` (tambah fungsi setelah `pixelate_region`, baris ~65)
- Test: `scripts/test_face_censor.py` (tambah 3 test setelah test existing)

**Interfaces:**
- Produces: `overlay_image_region(frame: np.ndarray, bbox: tuple[int,int,int,int], overlay: np.ndarray, padding: float = 0.15) -> np.ndarray` — `overlay` adalah hasil `cv2.imread(path, cv2.IMREAD_UNCHANGED)`, bisa 3-channel (BGR, opaque) atau 4-channel (BGRA, alpha). Return frame yang sama, dimodifikasi in-place (pola identik `pixelate_region`).

- [ ] **Step 1: Tambah 3 test ke `scripts/test_face_censor.py`**

Tambahkan setelah `test_padding_expands_processed_area` (sebelum blok `if __name__ == "__main__":`):

```python
def test_overlay_opaque_replaces_region():
    frame = np.zeros((100, 100, 3), dtype=np.uint8)
    overlay = np.full((10, 10, 3), 200, dtype=np.uint8)  # abu-abu solid
    overlay_image_region(frame, (20, 20, 60, 60), overlay, padding=0.0)
    roi = frame[20:80, 20:80]
    assert np.all(roi == 200)


def test_overlay_alpha_blends_with_original():
    frame = np.zeros((100, 100, 3), dtype=np.uint8)  # semua hitam (0)
    overlay = np.zeros((10, 10, 4), dtype=np.uint8)
    overlay[:, :, :3] = 200  # warna abu-abu solid
    overlay[:, :, 3] = 128   # alpha ~50%
    overlay_image_region(frame, (20, 20, 60, 60), overlay, padding=0.0)
    roi = frame[20:80, 20:80]
    # hasil blend harus di antara 0 (asli) dan 200 (overlay), bukan salah satu ekstrem
    assert np.all(roi > 0)
    assert np.all(roi < 200)


def test_overlay_clamps_to_frame_edges():
    frame = np.zeros((50, 50, 3), dtype=np.uint8)
    overlay = np.full((10, 10, 3), 100, dtype=np.uint8)
    result = overlay_image_region(frame, (-10, -10, 30, 30), overlay, padding=0.2)
    assert result.shape == (50, 50, 3)
```

Update import di baris 8 dari:
```python
from face_censor import pixelate_region  # noqa: E402
```
jadi:
```python
from face_censor import pixelate_region, overlay_image_region  # noqa: E402
```

Update blok `if __name__ == "__main__":` (baris 38-42) jadi:
```python
if __name__ == "__main__":
    test_pixelate_reduces_variance()
    test_pixelate_clamps_to_frame_edges()
    test_padding_expands_processed_area()
    test_overlay_opaque_replaces_region()
    test_overlay_alpha_blends_with_original()
    test_overlay_clamps_to_frame_edges()
    print("OK: pixelate_region + overlay_image_region self-check passed")
```

- [ ] **Step 2: Jalankan test, verify FAIL**

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/test_face_censor.py`
Expected: `ImportError: cannot import name 'overlay_image_region'`

- [ ] **Step 3: Implementasi `overlay_image_region` di `scripts/face_censor.py`**

Tambahkan setelah fungsi `pixelate_region` (setelah baris 65, sebelum `def main():`):

```python
def overlay_image_region(frame, bbox, overlay, padding: float = 0.15):
    """
    Tutup area bbox (x, y, w, h) di frame dengan gambar overlay, in-place.
    Overlay di-resize (stretch) ke ukuran area target. Kalau overlay punya
    channel alpha (BGRA), di-blend; kalau opaque (BGR), full replace.
    Bbox diperbesar dengan padding sama seperti pixelate_region.
    Return frame yang sama (dimodifikasi in-place).
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

    target_w, target_h = x2 - x1, y2 - y1
    resized = cv2.resize(overlay, (target_w, target_h), interpolation=cv2.INTER_LINEAR)

    if resized.shape[2] == 4:
        overlay_bgr = resized[:, :, :3].astype(np.float32)
        alpha = (resized[:, :, 3].astype(np.float32) / 255.0)[:, :, None]
        roi = frame[y1:y2, x1:x2].astype(np.float32)
        blended = overlay_bgr * alpha + roi * (1.0 - alpha)
        frame[y1:y2, x1:x2] = blended.astype(np.uint8)
    else:
        frame[y1:y2, x1:x2] = resized

    return frame
```

Tambahkan `import numpy as np` di bagian import atas file (setelah `import cv2` di try block, sebelum blok `def pixelate_region`) kalau belum ada:

```python
try:
    import cv2
    import numpy as np
except ImportError as _e:
```

(ganti `import cv2` tunggal jadi 2 baris di dalam try block yang sudah ada, baris 31)

- [ ] **Step 4: Jalankan test, verify PASS**

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/test_face_censor.py`
Expected: `OK: pixelate_region + overlay_image_region self-check passed`

- [ ] **Step 5: Commit**

```bash
git add scripts/face_censor.py scripts/test_face_censor.py
git commit -m "feat: tambah overlay_image_region buat sensor wajah mode gambar"
```

---

### Task 2: Wire `--censor-image` CLI arg ke `main()`

**Files:**
- Modify: `scripts/face_censor.py` (fungsi `main()`, baris ~68-173)

**Interfaces:**
- Consumes: `overlay_image_region(frame, bbox, overlay, padding=0.15)` dari Task 1
- Produces: CLI `face_censor.py <input> <output> [--censor-image PATH]`

- [ ] **Step 1: Tambah argumen CLI**

Di `main()`, ubah blok argparse (baris 69-72) dari:
```python
    parser = argparse.ArgumentParser(description="Face censor — pixelate semua wajah terdeteksi")
    parser.add_argument("input",  help="Input video path")
    parser.add_argument("output", help="Output video path")
    args = parser.parse_args()
```
jadi:
```python
    parser = argparse.ArgumentParser(description="Face censor — pixelate atau tutup gambar semua wajah terdeteksi")
    parser.add_argument("input",  help="Input video path")
    parser.add_argument("output", help="Output video path")
    parser.add_argument("--censor-image", default=None, help="Path gambar buat nutup wajah (opsional, default: mosaic)")
    args = parser.parse_args()
```

- [ ] **Step 2: Load overlay image sebelum loop (setelah detector di-load, sebelum ffmpeg_cmd dibangun — setelah baris 104)**

Tambahkan setelah blok `else: emit_status("[face_censor] Detector: Haar cascade — CPU (fallback)")` (baris 104), sebelum baris `cascade_front = ...` (baris 106):

```python
    overlay_img = None
    if args.censor_image:
        overlay_img = cv2.imread(args.censor_image, cv2.IMREAD_UNCHANGED)
        if overlay_img is None:
            emit_status(f"[face_censor] Gagal load gambar sensor ({args.censor_image}), pakai mosaic.")
        else:
            emit_status(f"[face_censor] Mode: gambar ({os.path.basename(args.censor_image)})")
```

- [ ] **Step 3: Ganti pemanggilan `pixelate_region` jadi kondisional**

Ubah baris 145-146:
```python
        for f in faces:
            pixelate_region(frame, f["bbox"], padding=0.15)
```
jadi:
```python
        for f in faces:
            if overlay_img is not None:
                overlay_image_region(frame, f["bbox"], overlay_img, padding=0.15)
            else:
                pixelate_region(frame, f["bbox"], padding=0.15)
```

- [ ] **Step 4: Verify — syntax check + regression test + CLI help**

Run: `cd /Users/oentoro/Projects/autoclipper && python3 -c "import ast; ast.parse(open('scripts/face_censor.py').read())" && echo "syntax OK"`
Expected: `syntax OK`

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/test_face_censor.py`
Expected: `OK: pixelate_region + overlay_image_region self-check passed` (regresi Task 1 tetap lolos)

Run: `cd /Users/oentoro/Projects/autoclipper && python3 scripts/face_censor.py --help`
Expected: help text menampilkan `--censor-image` sebagai opsi

- [ ] **Step 5: Commit**

```bash
git add scripts/face_censor.py
git commit -m "feat: wire --censor-image CLI arg ke face_censor.py main()"
```

---

### Task 3: Rust — teruskan path gambar ke `face_censor.py`

**Files:**
- Modify: `src-tauri/src/commands.rs` — `exec_censor_faces` (baris 2192-2251), `clip_video` signature (baris 2253-2275), pemanggilan stage (baris 2367)

**Interfaces:**
- Consumes: CLI `--censor-image PATH` dari Task 2
- Produces: `clip_video` command menerima parameter baru `censor_image_path: String` dari frontend (Task 4 mengirim ini)

- [ ] **Step 1: Tambah parameter `censor_image` ke `exec_censor_faces`**

Ubah signature (baris 2192-2199) dari:
```rust
async fn exec_censor_faces(
    app: &tauri::AppHandle,
    python: &str,
    ffmpeg: &str,
    input: &str,
    output: &str,
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
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
```

Ubah pembangunan command (baris 2203-2208) dari:
```rust
    let script = find_script(app, "face_censor.py");
    let mut cmd = TokioCommand::new(python);
    cmd.args([&script, input, output])
        .env("AUTOCLIPPER_FFMPEG", ffmpeg)
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
    cmd.env("AUTOCLIPPER_FFMPEG", ffmpeg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
```

- [ ] **Step 2: Tambah parameter `censor_image_path` ke `clip_video`**

Ubah signature `clip_video` (baris 2253-2275), tambahkan setelah `censor_faces: bool,` (baris 2268):
```rust
    censor_faces: bool,
    censor_image_path: String,
```

- [ ] **Step 3: Update pemanggilan stage**

Ubah baris 2367 dari:
```rust
            Stage::CensorFaces => exec_censor_faces(&app, &python, &ffmpeg, &current_path, &dest, pid_cell).await,
```
jadi:
```rust
            Stage::CensorFaces => {
                let censor_img = if censor_image_path.trim().is_empty() { None } else { Some(censor_image_path.as_str()) };
                exec_censor_faces(&app, &python, &ffmpeg, &current_path, &dest, censor_img, pid_cell).await
            }
```

- [ ] **Step 4: Verify — cargo check**

Run: `cd /Users/oentoro/Projects/autoclipper/src-tauri && cargo check`
Expected: compiles clean, no errors (warning boleh, existing warnings di file besar ini biasa)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat: teruskan censor_image_path dari clip_video ke exec_censor_faces"
```

---

### Task 4: Frontend — state + invoke wiring di `App.tsx`

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `clip_video` Tauri command dengan parameter baru `censorImagePath: string` dari Task 3
- Produces: props `censorMode`, `onCensorModeChange`, `censorImagePath`, `onCensorImagePathChange` yang dikonsumsi `TranscriptView` di Task 5

- [ ] **Step 1: Tambah state**

Ubah baris 139 dari:
```tsx
  const [censorFaces, setCensorFaces] = useState<boolean>(false);
```
jadi:
```tsx
  const [censorFaces, setCensorFaces] = useState<boolean>(false);
  const [censorMode, setCensorMode] = useState<"mosaic" | "image">("mosaic");
  const [censorImagePath, setCensorImagePath] = useState<string>("");
```

- [ ] **Step 2: Kirim ke invoke**

Ubah baris 475 dari:
```tsx
        censorFaces,
```
jadi:
```tsx
        censorFaces,
        censorImagePath: censorFaces && censorMode === "image" ? censorImagePath : "",
```

- [ ] **Step 3: Teruskan props ke `TranscriptView`**

Ubah baris 776-777 dari:
```tsx
            censorFaces={censorFaces}
            onCensorFacesChange={setCensorFaces}
```
jadi:
```tsx
            censorFaces={censorFaces}
            onCensorFacesChange={setCensorFaces}
            censorMode={censorMode}
            onCensorModeChange={setCensorMode}
            censorImagePath={censorImagePath}
            onCensorImagePathChange={setCensorImagePath}
```

- [ ] **Step 4: Verify — typecheck**

Run: `cd /Users/oentoro/Projects/autoclipper && npx tsc --noEmit`
Expected: error soal props `TranscriptView` belum menerima `censorMode` dkk (karena Task 5 belum dikerjakan) — ini expected di titik ini, lanjut ke Task 5 sebelum verify ulang.

- [ ] **Step 5: Commit**

```bash
git add src/App.tsx
git commit -m "feat: state censorMode + censorImagePath di App.tsx"
```

---

### Task 5: Frontend — UI selector + file picker di `TranscriptView.tsx`

**Files:**
- Modify: `src/components/TranscriptView.tsx`
- Modify: `src/i18n.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes: props `censorMode`, `onCensorModeChange`, `censorImagePath`, `onCensorImagePathChange` dari Task 4; `@tauri-apps/plugin-dialog` `open()`
- Produces: UI selesai, tidak ada task selanjutnya yang bergantung ini

- [ ] **Step 1: Tambah string i18n**

Di `src/i18n.tsx`, dictionary `id` (setelah baris 114 `censorFacesHint: "Pixelate semua wajah",`):
```tsx
    censorModeMosaic: "Mosaic",
    censorModeImage: "Gambar",
    censorImagePickLabel: "Pilih Gambar",
    censorImageEmptyHint: "Pilih gambar dulu",
```

Di dictionary `en` (setelah baris 298 `censorFacesHint: "Pixelate all faces",`):
```tsx
    censorModeMosaic: "Mosaic",
    censorModeImage: "Image",
    censorImagePickLabel: "Choose Image",
    censorImageEmptyHint: "Choose an image first",
```

- [ ] **Step 2: Tambah CSS**

Di `src/styles.css`, setelah blok `.smart-crop-note { ... }` (baris 1893-1898), tambahkan:
```css
.censor-image-picker {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 0 0 23px;
}
.censor-image-btn {
  padding: 5px 10px;
  font-size: 12px;
}
```

- [ ] **Step 3: Tambah import dialog**

Di `src/components/TranscriptView.tsx`, ubah baris 1-4 dari:
```tsx
import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLang } from "../i18n";
import type { SrtSegment, Section, FontInfo, LlmModel, SubtitleStyle, ManualClip } from "../types";
```
jadi:
```tsx
import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useLang } from "../i18n";
import type { SrtSegment, Section, FontInfo, LlmModel, SubtitleStyle, ManualClip } from "../types";
```

- [ ] **Step 4: Tambah props ke interface**

Ubah baris 46-47 dari:
```tsx
  censorFaces: boolean;
  onCensorFacesChange: (v: boolean) => void;
```
jadi:
```tsx
  censorFaces: boolean;
  onCensorFacesChange: (v: boolean) => void;
  censorMode: "mosaic" | "image";
  onCensorModeChange: (v: "mosaic" | "image") => void;
  censorImagePath: string;
  onCensorImagePathChange: (v: string) => void;
```

- [ ] **Step 5: Destructure props**

Ubah baris 143-144 dari:
```tsx
  censorFaces,
  onCensorFacesChange,
```
jadi:
```tsx
  censorFaces,
  onCensorFacesChange,
  censorMode,
  onCensorModeChange,
  censorImagePath,
  onCensorImagePathChange,
```

- [ ] **Step 6: Tambah handler pick gambar**

Di dalam component, setelah destructure props dan sebelum `return` (cari fungsi handler lain yang sudah ada di file ini untuk pola penempatan, taruh dekat fungsi serupa), tambahkan:
```tsx
  async function handlePickCensorImage() {
    const path = await open({
      filters: [{ name: "Gambar", extensions: ["png", "jpg", "jpeg"] }],
      title: "Pilih gambar sensor wajah",
    });
    if (path) onCensorImagePathChange(path as string);
  }
```

- [ ] **Step 7: Tambah UI selector + file picker**

Ubah blok Sensor Wajah (baris 625-639) dari:
```tsx
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
```
jadi:
```tsx
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
              {censorMode === "image" && (
                <div className="censor-image-picker">
                  <button
                    type="button"
                    className="btn btn-secondary censor-image-btn"
                    onClick={handlePickCensorImage}
                  >
                    {t("censorImagePickLabel")}
                  </button>
                  {censorImagePath ? (
                    <span className="smart-crop-hint">{censorImagePath.split(/[\\/]/).pop()}</span>
                  ) : (
                    <span className="smart-crop-hint">{t("censorImageEmptyHint")}</span>
                  )}
                </div>
              )}
            </>
          )}
        </div>
```

- [ ] **Step 8: Verify — typecheck**

Run: `cd /Users/oentoro/Projects/autoclipper && npx tsc --noEmit`
Expected: no errors

- [ ] **Step 9: Verify — dev server manual check**

Run: `cd /Users/oentoro/Projects/autoclipper && npm run tauri dev` (atau `npm run dev` kalau cuma perlu preview frontend)

Manual: buka app → step transcript → centang "Sensor Wajah" → muncul tombol Mosaic/Gambar → klik "Gambar" → muncul tombol "Pilih Gambar" + hint "Pilih gambar dulu" → klik tombol → file dialog kebuka → pilih file PNG → hint berubah jadi nama file. Toggle balik ke Mosaic → picker hilang.

- [ ] **Step 10: Commit**

```bash
git add src/components/TranscriptView.tsx src/i18n.tsx src/styles.css
git commit -m "feat: UI selector mosaic/gambar + file picker buat sensor wajah"
```

---

## Self-Review Notes

- Spec coverage: mode selector ✅ (Task 5), 1 gambar global ✅ (Task 5 state, single path), stretch fit ✅ (Task 1), alpha-composite ✅ (Task 1), default mosaic tetap ✅ (Task 4 default state + Task 3 empty-string→None), fallback gagal load ✅ (Task 2 Step 2), error handling stage tetap pakai mekanisme existing (Task 3 tidak mengubah error path `exec_censor_faces`).
- Semua task saling terhubung lewat interface eksplisit: `overlay_image_region` (Task1→2), `censor_image: Option<&str>` param (Task3), `censorImagePath: String` (Task3↔4), props `TranscriptView` (Task4↔5).
- Tidak ada placeholder — semua step berisi kode konkret siap tempel.
