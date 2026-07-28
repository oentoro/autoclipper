# Burn Subtitles — Native FFmpeg Rewrite — Design Spec
Date: 2026-07-28

## Overview

Proses burn subtitle sekarang lambat karena arsitekturnya: ffmpeg decode → pipe raw frame ke Python → PIL gambar teks per frame → pipe raw frame lagi ke ffmpeg encode. Benchmark (30s video 1080x1920, 900 frame, 12 subtitle entry) nunjukin breakdown:

| Komponen | Waktu | Porsi |
|---|---|---|
| ffmpeg decode+encode murni (1 proses, no Python) | 2.56s | baseline |
| + overhead relay raw-frame lewat Python (no PIL) | +2.15s | |
| + PIL gambar teks per frame | +4.2s | |
| **Total (implementasi sekarang)** | **8.9s** | 71% overhead Python/PIL, 29% ffmpeg |

Ganti encoder ke GPU (`h264_videotoolbox`) doang cuma hemat ~6% (8.9s → 8.36s) — encode CPU usage turun 2x tapi wall-time nyaris sama, karena encode bukan bottleneck utama.

Solusi: render tiap teks subtitle UNIK jadi PNG transparan (pakai render logic yang SAMA PERSIS kayak sekarang — no rewrite di situ), lalu satu proses ffmpeg nge-overlay PNG-PNG itu di window waktu masing-masing. Cost sisi Python jadi ngikutin JUMLAH ENTRY UNIK (puluhan-ratusan), bukan JUMLAH FRAME (ribuan) — dan ffmpeg-nya sendiri cuma 1 proses native (bisa hardware-accelerated).

Feasibility udah divalidasi: filter_complex dengan 200 overlay entry di video 60s selesai dalam 7.9s (lebih cepat dari implementasi lama buat video 30s/12-entry aja).

## Scope

- Fungsi baru `burn_native()` di `scripts/burn_subtitles.py` — reuse 100% fungsi rendering existing (`find_font`, `wrap_text`, `_compute_subtitle_layout`, `hex_to_rgb`), TIDAK ada perubahan logic CJK/word-wrap/box-style.
- Fungsi lama `burn()` (per-frame PIL+pipe) TETAP ADA sebagai fallback — dipanggil otomatis kalau `burn_native()` gagal (exception apapun).
- Hardware encoder (videotoolbox/nvenc/qsv/amf tergantung platform) dengan fallback ke libx264 kalau HW encoder gagal jalan.
- Progress reporting tetap format `PROGRESS:N` yang sama (parse dari ffmpeg `-progress pipe:2`), TIDAK perlu perubahan di sisi Rust (`commands.rs`) atau frontend.
- Output JSON (`{"success":true,"frames":N}` / `{"error":...}`) format sama persis — CLI contract dengan Rust caller tidak berubah.

## Yang Tidak Termasuk

- Port ke ASS/libass filter (dipertimbangkan, ditolak — risiko re-implement word-wrap/CJK/box style dari nol, hasil render bisa beda pixel dari sekarang)
- Perubahan di `src-tauri/src/commands.rs` atau frontend (integrasi transparan lewat CLI contract yang sama)
- Tuning kualitas/bitrate final per-platform (videotoolbox/nvenc/qsv/amf) — didesain pakai `-b:v` (bitrate eksplisit berdasar resolusi) supaya ukuran file predictable, tapi angka bitrate final divalidasi visual+size pas implementasi, bukan diasumsikan dari awal

## Arsitektur

### 1. `scripts/burn_subtitles.py` — fungsi baru `burn_native()`

Alur:
1. Baca video info (`get_video_info`, sudah ada) — w, h, fps, total_frames.
2. Buat temp dir (`tempfile.mkdtemp`).
3. Untuk tiap ENTRY UNIK berdasarkan teks (sama seperti caching layout yang sudah ada), render 1 PNG transparan ukuran w×h:
   - Reuse `_build_subtitle_overlay` (box mode) — perlu tweak kecil: saat ini fungsi ini selalu gambar rectangle box; perlu tambahan cek `box_enabled` (sekarang cek ini ada di `draw_subtitle`, bukan di `_build_subtitle_overlay`) supaya box gak muncul kalau `boxEnabled=False`.
   - Untuk mode non-box: perlu path baru yang gambar stroke+fill text ke canvas RGBA transparan (bukan ke opaque frame image seperti `draw_subtitle` sekarang) — logic wrap/posisi/font SAMA, cuma target canvasnya transparan.
   - Title (kalau ada) dapat 1 PNG sendiri, dirender sekali (persisten sepanjang durasi).
   - Simpan tiap PNG ke file di temp dir (BUKAN disimpan di RAM) — ini yang menghindari masalah RAM blowup yang disebutkan di komentar existing (baris ~470 `burn_subtitles.py`).
4. Bangun filter_complex graph:
   ```
   [0:v][1:v]overlay=0:0:enable='between(t,START1,END1)'[v1];
   [v1][2:v]overlay=0:0:enable='between(t,START2,END2)'[v2];
   ...
   [vN-1][N:v]overlay=0:0:enable='between(t,0,DURATION)'[vout]   <- title, kalau ada, PALING TERAKHIR (z-order di atas subtitle, sama seperti burn() lama)
   ```
   Ditulis ke file (bukan inline argumen) — dipakai lewat `-filter_complex_script <file>` supaya gak kena limit panjang command-line di Windows buat entry count besar.
5. Jalankan SATU proses ffmpeg: `-i input -i png1 -i png2 ... -filter_complex_script graph.txt -map [vout] -map 0:a? -c:v <encoder> ... -c:a copy output`.
6. Parse `-progress pipe:2` output ffmpeg, emit `PROGRESS:N` (format sama seperti sekarang) berdasarkan `out_time_ms` dibagi total durasi.
7. Hapus temp dir (try/finally).

### 2. Encoder selection

Fungsi baru `_pick_encoder()`:
- macOS → coba `h264_videotoolbox`
- Windows → coba `h264_nvenc`, lalu `h264_qsv`, lalu `h264_amf`
- Linux → coba `h264_nvenc`, lalu `h264_vaapi`
- Availability check: `ffmpeg -encoders` output, cek nama encoder ada di listing (build-level check, bukan jaminan hardware tersedia).
- Kalau semua HW encoder gak ada di listing atau proses ffmpeg akhir gagal (`returncode != 0`) → retry SATU KALI dengan `libx264 -preset fast -crf 23` (encoder existing) di path native yang sama (tanpa balik ke `burn()` lama dulu — HW encoder gagal bukan berarti seluruh pendekatan native gagal).
- Kalau retry libx264 di native path JUGA gagal → baru lempar exception, ketangkep di `__main__`, fallback total ke `burn()` lama.
- Bitrate: `-b:v` dihitung dari resolusi (bukan `-crf`/`-q:v`, supaya konsisten predictable di semua encoder) — nilai awal berbasis heuristik umum (1080p ~8-10 Mbps), divalidasi ulang saat implementasi dengan perbandingan ukuran file & kualitas visual vs `libx264 crf 23` yang sekarang jadi baseline.

### 3. Integrasi `__main__`

```python
try:
    frames = burn_native(...)
except Exception as e:
    emit_status(f"[burn] native path gagal ({e}), fallback ke metode lama")
    frames = burn(...)
```
Output JSON tetap `{"success": true, "frames": N}` — tidak ada perubahan di `exec_burn_subs` (`src-tauri/src/commands.rs`) maupun frontend.

## Testing

Gaya assert-based, tanpa framework (konsisten dengan `test_smart_crop.py`, `test_face_censor.py`):
- **Parity test**: render teks+style yang sama lewat path lama (`draw_subtitle` ke frame opaque) vs path baru (render ke canvas RGBA transparan) — assert hasil pixel yang terlihat (bukan background transparan) identik/sangat mirip. Membuktikan reuse render logic tidak mengubah visual.
- **Smoke test end-to-end**: video + entries sintetis kecil → `burn_native()` sukses, output ada, ukuran wajar (dalam rentang yang masuk akal dibanding baseline).
- **Fallback test**: paksa `burn_native()` raise (monkeypatch/mock) → pastikan `burn()` lama benar-benar kepanggil dan tetap menghasilkan output valid.

## Error Handling

- `burn_native()` exception apapun (termasuk kedua encoder gagal) → tertangkap di `__main__`, fallback ke `burn()` lama, tidak ada perubahan ke error contract JSON yang sudah ada.
- Temp dir PNG selalu dihapus (try/finally), baik sukses maupun gagal, supaya tidak menumpuk file temp di disk user.
- Kalau font/CJK gagal (kasus yang sudah ditangani `find_font` sekarang) — tidak berubah, dipakai apa adanya oleh `burn_native()` juga.

## Data Flow

```
commands.rs: exec_burn_subs()
  → python burn_subtitles.py <input> <entries.json> <output> [--font-size --font --style --title ...]
    → __main__: try burn_native()
        → render PNG per teks unik (temp dir)
        → bangun filter_complex_script
        → 1x ffmpeg: decode + overlay chain + encode (HW encoder, fallback libx264)
        → parse -progress pipe:2 → emit PROGRESS:N
        → sukses → return frames, hapus temp dir
      except → emit_status(fallback) → burn() lama (proses sekarang, tidak berubah)
    → print JSON {"success":true,"frames":N} ke stdout
  ← Rust parse JSON, sama seperti sekarang
```
