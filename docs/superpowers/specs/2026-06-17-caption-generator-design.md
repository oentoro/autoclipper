# Caption Generator — Design Spec
Date: 2026-06-17

## Overview

Generate dua versi caption siap-copas (TikTok pendek + Instagram panjang) dari konten klip yang sudah diproses. Caption di-generate otomatis saat hasil klip ditampilkan, menggunakan LLM yang sudah ada (llama-cpp / Ollama).

## Scope

- Generate caption dari segmen klip terpilih
- Dua format: TikTok (pendek) dan Instagram (panjang)
- Hashtag otomatis dari LLM, terpisah per format
- Bahasa mengikuti `detected_language` dari Whisper (`id` → Indonesia, `en` → English, lainnya → English)
- Tombol salin — menyalin caption + hashtag sekaligus
- Tidak ada upload ke platform manapun

## Arsitektur

### 1. `scripts/analyze.py` — prompt builder baru

Tambah fungsi `build_caption_prompt(segments, language)`:
- Input: list segmen klip terpilih + kode bahasa (`id` / `en` / dll)
- Output: string prompt yang minta LLM return JSON
- Mode dipanggil via argumen `--mode caption`

JSON output yang diminta dari LLM:
```json
{
  "caption_short": "teks TikTok tanpa hashtag",
  "caption_long": "teks Instagram tanpa hashtag",
  "hashtags_short": ["#tag1", "#tag2"],
  "hashtags_long": ["#tag1", "#tag2"]
}
```

### 2. `src-tauri/src/commands.rs` — command baru

Tambah Tauri command `generate_caption`:
- Parameter: `segments: Vec<SrtSegment>`, `language: String`, plus path ke LLM (sama dengan `analyze_video`)
- Panggil `analyze.py --mode caption`
- Parse JSON response
- Return `CaptionResult { caption_short, caption_long, hashtags_short, hashtags_long }`

### 3. `src/components/ClipResults.tsx` — panel caption

Panel baru di bawah daftar klip:
- `language` diambil dari `detectedLanguage` yang sudah ada di state hasil transkripsi (sudah tersedia saat ClipResults ditampilkan)
- Auto-invoke `generate_caption` saat komponen mount (setelah klip tersedia)
- State: `idle | loading | done | error`
- Dua tab: **TikTok** / **Instagram**
- Tombol **[Salin]**: menyalin `caption + "\n\n" + hashtags.join(" ")` ke clipboard
- Tombol **[Generate]**: regenerate manual
- Tab aktif dipertahankan saat regenerate

## Format Caption

### TikTok (pendek)
```
[Hook 1 kalimat kuat]

[1–2 kalimat isi konten]

#hashtag1 #hashtag2 #hashtag3 #hashtag4 #hashtag5
```

### Instagram (panjang)
```
[Hook 1 kalimat]

[Paragraf 1 — konteks/masalah]

[Paragraf 2 — isi utama]

[Call-to-action]

.
.
.
#hashtag1 #hashtag2 ... #hashtag20
```

## Prompt LLM

```
Kamu adalah copywriter media sosial profesional.
Buat caption untuk video berdasarkan transkrip klip berikut.

Bahasa: {bahasa}
Transkrip:
{segmen}

Buat dua versi:
1. TikTok: hook 1 kalimat kuat, 1–2 kalimat isi, 5–7 hashtag relevan
2. Instagram: hook, 2–3 paragraf, call-to-action, 15–20 hashtag

Balas HANYA dengan JSON valid, tanpa teks lain:
{
  "caption_short": "...",
  "caption_long": "...",
  "hashtags_short": ["#tag1"],
  "hashtags_long": ["#tag1", "#tag2"]
}
```

## UI Wireframe

```
┌─────────────────────────────────────────┐
│ 📋 Caption                    [Generate]│
├─────────────────────────────────────────┤
│ [TikTok]  [Instagram]                   │
│                                         │
│  [teks caption sesuai tab aktif]        │
│                                         │
│  #hashtag1 #hashtag2 ...                │
│                                 [Salin] │
└─────────────────────────────────────────┘
```

State loading: skeleton/spinner menggantikan isi panel.
State error: pesan error + tombol [Coba Lagi].

## Data Flow

```
ClipResults mount
  → invoke("generate_caption", { segments, language })
    → analyze.py --mode caption --language {lang}
      → LLM (llama-cpp atau Ollama)
      → JSON response
    → parse → CaptionResult
  → UI render panel caption
```

## Error Handling

- LLM timeout / tidak tersedia → tampilkan pesan error, tombol retry
- JSON parse gagal → retry otomatis sekali, lalu tampilkan error
- Segmen kosong → tidak trigger generate, panel disembunyikan

## Yang Tidak Termasuk

- Upload ke TikTok / Instagram
- Simpan caption ke file
- Edit caption di dalam app
- Template caption yang bisa dikustomisasi user
