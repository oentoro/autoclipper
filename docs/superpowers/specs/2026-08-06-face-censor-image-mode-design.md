# Sensor Wajah — Mode Gambar (Image Overlay) — Design Spec
Date: 2026-08-06

## Overview

Tambah mode kedua ke fitur sensor wajah (lihat [[2026-07-27-face-censor-design.md]]): selain mosaic (existing), user bisa pilih 1 gambar (sticker/emoji/logo) untuk menutup semua wajah terdeteksi, bukan pixelate. Mosaic tetap default — mode gambar opsional, mengubah cara menutup bbox saja, tidak mengubah deteksi/pipeline stage yang sudah ada.

## Scope

- Selector mode "Mosaic" / "Gambar" muncul di UI saat toggle Sensor Wajah aktif, mirip pola tombol smooth/aggressive pada smart crop transition
- Mode Gambar: 1 gambar global dipakai untuk semua wajah terdeteksi di seluruh video (bukan per-wajah/per-orang)
- Gambar di-stretch (resize tanpa jaga aspect ratio) supaya persis menutup bbox wajah + padding 15%, sama seperti area yang di-pixelate sekarang
- Kalau gambar punya alpha channel (PNG transparan), alpha-composite ke frame asli (bagian transparan menampakkan frame di baliknya). Kalau gambar opaque (JPG atau PNG tanpa alpha), full replace kotak seperti mosaic
- Default mode tetap Mosaic — behavior existing tidak berubah kalau user tidak eksplisit pilih Gambar

## Yang Tidak Termasuk

- Multi-gambar / gambar berbeda per wajah (butuh face identity tracking, di luar scope — detector saat ini per-frame tanpa identity)
- Gambar animasi (GIF/video overlay)
- Fit mode "contain" (jaga aspect ratio, gambar di tengah bbox) — stretch dipilih karena lebih simpel dan konsisten nutup penuh
- Rotasi/orientasi gambar mengikuti kemiringan wajah — overlay selalu axis-aligned ke bbox

## Arsitektur

### 1. `scripts/face_censor.py`

Fungsi baru `overlay_image_region(frame, bbox, overlay, padding=0.15)`, pasangan `pixelate_region` yang sudah ada:

- Hitung area target sama persis seperti `pixelate_region` (bbox + padding 15%, clamp ke batas frame)
- Resize `overlay` (BGR atau BGRA) ke ukuran area target pakai `cv2.resize(..., interpolation=cv2.INTER_LINEAR)` (stretch, tanpa jaga aspect ratio)
- Kalau `overlay` 4-channel (ada alpha):
  - Split BGR + alpha (`alpha = overlay[:,:,3] / 255.0`)
  - Blend: `frame[y1:y2, x1:x2] = overlay_bgr * alpha + frame[y1:y2, x1:x2] * (1 - alpha)` per channel
- Kalau `overlay` 3-channel (opaque): full replace langsung, sama seperti mosaic (`frame[y1:y2, x1:x2] = resized`)
- Return frame yang sama (in-place), pola sama seperti `pixelate_region` untuk testability

`main()`:
- Argumen baru opsional `--censor-image PATH` (default `None`)
- Kalau `--censor-image` di-pass: load sekali sebelum loop frame — `overlay_img = cv2.imread(path, cv2.IMREAD_UNCHANGED)`. Kalau gagal load (`None`), fallback ke mosaic (jangan crash) + `emit_status` warning
- Di loop per-frame, ganti pemanggilan tunggal `pixelate_region` jadi kondisional:
  ```python
  for f in faces:
      if overlay_img is not None:
          overlay_image_region(frame, f["bbox"], overlay_img, padding=0.15)
      else:
          pixelate_region(frame, f["bbox"], padding=0.15)
  ```

### 2. `scripts/test_face_censor.py`

Tambah test untuk `overlay_image_region`, pola sama (assert-based, no framework):

- `test_overlay_opaque_replaces_region` — overlay 3-channel penuh warna solid X, region hasil harus semua piksel = warna X (full replace, bukan blend)
- `test_overlay_alpha_blends_with_original` — overlay 4-channel dengan alpha 0.5 warna solid, region hasil harus di antara warna asli dan warna overlay (bukan salah satu ekstrem)
- `test_overlay_clamps_to_frame_edges` — bbox di luar batas frame (sama kasus seperti `test_pixelate_clamps_to_frame_edges`), tidak crash, shape frame tidak berubah

### 3. `src-tauri/src/commands.rs`

- `exec_censor_faces`: tambah parameter `censor_image: Option<&str>`. Kalau `Some(path)`, tambahkan `"--censor-image"` dan `path` ke `cmd.args([...])`
- `clip_video`: tambah parameter `censor_image_path: String`. Konversi ke `Option<&str>`: string kosong → `None` (mosaic), non-kosong → `Some(&censor_image_path)`
- Pemanggilan di stage loop (baris ~2367): `Stage::CensorFaces => exec_censor_faces(&app, &python, &ffmpeg, &current_path, &dest, censor_img_opt, pid_cell).await`

Tidak ada perubahan pada urutan stage atau struktur `Stage` enum — mode gambar hanya mengubah argumen yang dikirim ke script yang sudah ada di pipeline.

### 4. Frontend — `src/App.tsx`, `src/components/TranscriptView.tsx`, `src/i18n.tsx`

**State baru (App.tsx, mirror pola `smartCropTransition`):**
```ts
const [censorMode, setCensorMode] = useState<"mosaic" | "image">("mosaic");
const [censorImagePath, setCensorImagePath] = useState<string>("");
```

**Invoke (App.tsx, mirror baris 475):**
```ts
censorImagePath: censorFaces && censorMode === "image" ? censorImagePath : "",
```

**UI (TranscriptView.tsx, di dalam blok `censorFaces &&` setelah checkbox existing baris ~626-639):**
- Selector 2 tombol "Mosaic" / "Gambar" (pola identik `smart-crop-transition` baris 599-613), prop `censorMode` + `onCensorModeChange`
- Kalau `censorMode === "image"`: tombol "Pilih Gambar" pakai `open()` dari `@tauri-apps/plugin-dialog`, filter `{ name: "Gambar", extensions: ["png", "jpg", "jpeg"] }`, hasil path disimpan ke `censorImagePath` lewat `onCensorImagePathChange`. Tampilkan nama file terpilih (basename) di sebelah tombol sebagai konfirmasi visual
- Kalau `censorMode === "image"` dan `censorImagePath` masih kosong: tampilkan hint text kecil "Pilih gambar dulu" (tidak block render — lihat Error Handling)

**i18n baru (id + en):**
- `censorModeMosaic` / `censorModeImage` — label 2 tombol
- `censorImagePickLabel` — teks tombol pilih file ("Pilih Gambar" / "Choose Image")
- `censorImageEmptyHint` — hint kalau belum pilih gambar

## Data Flow

```
User centang "Sensor Wajah" → pilih mode "Gambar" → klik "Pilih Gambar" → dialog file
  → censorImagePath = "/path/to/sticker.png"
  → invoke("clip_video", { ..., censorFaces: true, censorImagePath: "/path/to/sticker.png" })
    → commands.rs: exec_censor_faces(..., censor_image: Some("/path/to/sticker.png"), ...)
      → face_censor.py --censor-image /path/to/sticker.png
        → load sticker.png sekali (IMREAD_UNCHANGED, cek alpha)
        → tiap frame: deteksi wajah → overlay_image_region tiap bbox (stretch + alpha blend kalau ada)
  → output_path = video dengan wajah tertutup gambar
```

## Error Handling

- `censorMode === "image"` tapi `censorImagePath` kosong saat submit → frontend fallback kirim `censorImagePath: ""`, backend otomatis jalan mosaic (bukan error) — konsisten dengan prinsip "jangan block user", tapi hint UI tetap muncul supaya user sadar mode Gambar belum efektif
- `cv2.imread` gagal load file (path rusak/format tidak didukung) → `overlay_img is None`, script fallback diam-diam ke `pixelate_region` + `emit_status` pesan warning ke UI (bukan hard error, video tetap ter-render)
- Gambar dengan channel selain 3/4 (misal grayscale 1-channel) → di luar scope, tidak divalidasi khusus; kasus ini sangat jarang untuk file dari dialog PNG/JPG dan `cv2.imread` dengan `IMREAD_UNCHANGED` pada file grayscale akan tetap punya shape yang bisa gagal saat blending — kalau terjadi, exception naik jadi error stage seperti error handling stage lain yang sudah ada (`exec_censor_faces` sudah menangkap non-zero exit code sebagai error)

## Catatan Performa

Tidak ada perubahan performa signifikan dibanding mode mosaic — `cv2.resize` untuk overlay image sebanding cost-nya dengan dua kali `cv2.resize` di `pixelate_region`. Alpha blend (kalau ada) menambah beberapa operasi array per-frame per-bbox, diabaikan (bukan bottleneck dibanding cost deteksi wajah tiap frame).
