# Sensor Wajah (Face Censor) — Design Spec
Date: 2026-07-27

## Overview

Tambah opsi sensor wajah (blur mosaic/pixelate) pada video hasil clip. Semua wajah yang terdeteksi disensor, termasuk speaker utama. Fitur ini toggle opsional saat export, independen dari smart crop (tetap jalan walau smart crop mati).

## Scope

- Sensor SEMUA wajah terdeteksi (bukan cuma non-speaker)
- Gaya sensor: pixelate/mosaic (bukan gaussian blur atau kotak hitam)
- Toggle opsional di UI export, default off
- Jalan independen dari smart crop — bisa aktif walau smart crop off
- Kalau smart crop DAN sensor wajah sama-sama nyala: crop dulu, baru sensor (di atas hasil crop)
- Deteksi tiap frame penuh (bukan sample 4fps kayak smart crop) — prioritas ketepatan, bukan performa, karena ini fitur privasi
- Reuse detector yang sudah ada di smart_crop.py (InsightFace > MediaPipe > YuNet > Haar cascade), tidak duplikat kode

## Yang Tidak Termasuk

- Opsi kecualikan speaker utama dari sensor
- Gaya sensor selain pixelate (blur, kotak hitam) — bisa ditambah nanti kalau diminta
- Deteksi sample+interpolasi untuk optimasi performa (bisa ditambah nanti kalau render sensor kelamaan)

## Arsitektur

### 1. `scripts/smart_crop.py` — tidak berubah, cuma jadi sumber import

Fungsi-fungsi detector yang sudah ada (`_load_insightface`, `_detect_insightface`, `_load_mediapipe`, `_detect_mediapipe`, `_load_yunet`, `_detect_yunet`, `_detect_cascade`) diimpor langsung oleh `face_censor.py`. Tidak ada perubahan pada file ini — sudah menyediakan semua yang dibutuhkan (deteksi multi-wajah dengan bbox per wajah).

### 2. `scripts/face_censor.py` — script baru

```
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from smart_crop import (
    _load_insightface, _detect_insightface,
    _load_mediapipe, _detect_mediapipe,
    _load_yunet, _detect_yunet,
    _detect_cascade,
)
```

Alur `main()`:
1. Buka video input, ambil `src_w`, `src_h`, `fps` (pola sama seperti `smart_crop.py`)
2. Load detector sekali di luar loop, pakai prioritas yang sama: InsightFace > MediaPipe > YuNet > Haar cascade
3. Loop baca frame **satu-satu, tanpa sampling** — tiap frame:
   - Jalankan detector → dapat list bbox semua wajah di frame itu
   - Untuk tiap bbox: panggil `pixelate_region(frame, bbox, padding=0.15)` yang mem-pixelate area itu langsung di buffer frame
   - Tulis frame hasil ke stdin pipe ffmpeg (rawvideo, pola identik dengan `smart_crop.py`)
4. ffmpeg command sama seperti di `smart_crop.py`: input rawvideo dari pipe + input kedua = video asli untuk ambil audio (`-map 1:a:0?`)
5. `emit_progress` / `emit_status` pola sama (reuse helper yang sudah ada di file ini, disalin minimal atau diimpor kalau sudah generic)

Fungsi inti `pixelate_region(frame, bbox, padding=0.15)`:
- Bbox diperbesar dengan padding 15% di tiap sisi (biar tepi wajah — dagu, rambut depan — ikut ke-cover), clamp ke batas frame
- Block size pixelate proporsional ke ukuran bbox: `block = max(6, min(w, h) // 8)`
- Teknik: `cv2.resize(roi, (w//block, h//block), interpolation=INTER_LINEAR)` lalu `cv2.resize` balik ke `(w, h)` pakai `INTER_NEAREST` — hasil kotak-kotak mosaic
- Tulis balik ke `frame[y:y+h, x:x+w]`

CLI: `face_censor.py <input> <output>` — tanpa argumen tambahan (tidak butuh `--ratio` atau `--transition` karena tidak crop).

### 3. `scripts/test_face_censor.py` — test baru

Gaya sama seperti `test_smart_crop.py` (assert-based, tanpa framework). Test `pixelate_region`:
- Region hasil pixelate punya variance lebih rendah dari original (bukti sudah di-blur/kotak-kotak)
- Bbox di tepi frame (misal `x=0` atau `x+w > frame_w`) tidak crash, hasil clamp dengan benar
- Padding memperluas area yang diproses dibanding bbox asli

### 4. `src-tauri/src/commands.rs` — integrasi Rust

- Tambah parameter `censor_faces: bool` ke command `clip_video`
- Tambah fungsi `exec_censor_faces()`, mirror `exec_smart_crop()` (baris ~2033): cari `face_censor.py` via `find_script`, jalankan `python face_censor.py <tmp_in> <tmp_out>`, handle progress/status/cancel via `pid_cell` sama seperti `exec_smart_crop`

**Refactor pipeline**: kondisi `match (needs_burn, needs_smart)` (baris ~2274) diganti rantai stage linear, supaya nambah `needs_censor` tidak meledak jadi 8 kombinasi:

```rust
let needs_censor = censor_faces;

// tahap 1: selalu — concat_groups ke tmp (atau langsung output_path kalau ini stage terakhir)
// tahap 2: kalau needs_smart → exec_smart_crop tmp -> tmp
// tahap 3: kalau needs_censor → exec_censor_faces tmp -> tmp
// tahap 4: kalau needs_burn → exec_burn_subs tmp -> tmp (atau output_path kalau terakhir)
```

Pola konkret: kumpulkan stage jadi list closure/enum, jalankan berurutan, tiap stage baca dari tmp path stage sebelumnya dan tulis ke tmp path baru (atau `output_path` kalau dia stage terakhir yang butuh dijalankan). Stage yang tidak aktif dilewati (input diteruskan langsung ke stage berikutnya tanpa proses). Hapus tmp file setelah tidak dipakai lagi, sama seperti pola existing (`std::fs::remove_file`).

Urutan stage tetap: `concat → smart_crop → censor_faces → burn_subs`.

### 5. Frontend — `src/App.tsx`, `src/components/TranscriptView.tsx`, `src/i18n.tsx`

- State baru `censorFaces` (boolean, default `false`), mirror pola `smartCrop` (App.tsx:137)
- Diteruskan ke `invoke("clip_video", { ...., censorFaces })` (mirror baris 463-464)
- Checkbox baru di `TranscriptView.tsx` deket toggle smart crop, prop `censorFaces` + `onCensorFacesChange` mirror prop `smartCrop`
- String i18n baru: label checkbox ("Sensor Wajah" / "Censor Faces") + deskripsi singkat, ditambah ke `id`/`en` dictionary di `i18n.tsx` mirror entry `smartCrop` yang sudah ada

## Data Flow

```
User centang "Sensor Wajah" di UI export
  → App.tsx state censorFaces = true
  → invoke("clip_video", { ..., censorFaces: true })
    → commands.rs: concat_groups (tmp1)
    → [smart_crop aktif?] exec_smart_crop tmp1 -> tmp2
    → exec_censor_faces tmp2 (atau tmp1 kalau smart crop off) -> tmp3
      → face_censor.py: deteksi tiap frame, pixelate semua bbox wajah, pipe ke ffmpeg
    → [burn subtitle aktif?] exec_burn_subs tmp3 -> output_path
  → output_path = video final dengan wajah tersensor
```

## Error Handling

- Video tanpa wajah terdeteksi sama sekali → output sama seperti input (tidak ada bbox yang diproses), tidak error
- Semua detector gagal load (termasuk Haar cascade tidak ada) → sama seperti smart_crop.py, Haar cascade adalah fallback terakhir yang selalu tersedia via OpenCV bundled data, jadi kondisi ini tidak terjadi dalam praktik
- Proses dibatalkan user (cancel) → `pid_cell` pola sama seperti `exec_smart_crop`, kill proses ffmpeg + python

## Catatan Performa

Deteksi tiap frame (bukan sample) berarti waktu render sensor wajah kira-kira sebanding dengan total frame video dibagi sample rate smart crop saat ini (~4fps) — bisa beberapa kali lebih lambat dari smart crop untuk video yang sama. Ini trade-off yang disengaja (prioritas: tidak ada frame yang lolos sensor). Kalau nanti render dirasa kelamaan, opsi optimasi (sample+track+padding besar) bisa ditambah belakangan — di luar scope spec ini.
