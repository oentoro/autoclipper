# Subtitle Timeline Drag — Design Spec
Date: 2026-09-01

## Overview

Tambah panel video preview + timeline horizontal ke `TranscriptView` (step "transcript") supaya user bisa geser-geser timing (start/end) tiap segmen subtitle secara visual, sinkron dengan preview video — mirip trim clip di CapCut/DaVinci Resolve. Timeline mencakup seluruh durasi video (bukan per-section), dengan zoom biar tetap presisi buat video panjang. Ini fitur baru murni di frontend — tidak ada perubahan pipeline Rust/Python, karena `segments` (array dengan `start`/`end` numerik) sudah langsung dikirim ke `clip_video` untuk burn subtitle timing.

## Scope

- Komponen baru `SubtitleTimeline.tsx`: `<video>` preview (pola sama seperti step "ready", `convertFileSrc(videoPath)`) + timeline di bawahnya
- Timeline nampilin semua segmen sebagai blok horizontal, posisi/lebar proporsional ke waktu (px/detik, di-scale lewat zoom)
- Drag badan blok → geser `start`+`end` bareng (durasi tetap)
- Drag handle kiri/kanan blok → trim `start` atau `end` sendiri-sendiri
- Klik ruler/timeline kosong → seek video ke waktu itu
- Klik blok segmen → seek video ke `start` segmen itu
- Playhead (garis vertikal) ngikutin `video.currentTime` saat playback, timeline auto-scroll biar playhead tetap kelihatan
- Slider zoom (px/detik, range 5–200, default 40)
- Ditaruh di `TranscriptView.tsx`, panel baru di atas `segments-list` yang sudah ada
- Edit lewat drag update `segments` state di `App.tsx` (numeric `start`/`end` + regenerate `start_time`/`end_time` string), segmen di `segments-list` yang sudah ada otomatis reflect perubahan (state sama)

## Yang Tidak Termasuk

- Drag reposisi teks subtitle di layar video (lihat obrolan brainstorming — user pilih scope timing saja, bukan posisi)
- Collision/overlap prevention antar segmen bertetangga — boleh overlap setelah drag, ini cuma metadata timing caption, bukan video track yang harus non-overlapping
- Waveform audio di timeline (nice-to-have visual, tidak perlu buat fungsi drag)
- Snap-to-neighbor / magnetic snapping saat drag
- Undo/redo khusus drag (aplikasi belum punya undo history sama sekali — di luar scope ini)
- Multi-select drag (geser beberapa segmen sekaligus)
- Wheel-zoom (ctrl+scroll) — pakai slider zoom biasa untuk v1

## Arsitektur

### 1. `src/types.ts`

Tidak ada perubahan struktur — `SrtSegment.start`/`end` (number, detik) dan `start_time`/`end_time` (string, format SRT) sudah cukup.

### 2. `src/lib/time.ts` (baru, helper kecil)

```ts
export function secondsToSrtTime(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  const ms = Math.round((seconds - Math.floor(seconds)) * 1000);
  const pad2 = (n: number) => String(n).padStart(2, "0");
  const pad3 = (n: number) => String(n).padStart(3, "0");
  return `${pad2(h)}:${pad2(m)}:${pad2(s)},${pad3(ms)}`;
}
```

Format identik `seconds_to_srt_time` di `scripts/transcribe.py:85` (dipertahankan biar `start_time`/`end_time` yang ditampilkan di `segments-list` tetap konsisten format dengan yang backend hasilkan).

### 3. `src/components/SubtitleTimeline.tsx` (baru)

Props:
```ts
interface Props {
  videoPath: string;
  segments: SrtSegment[];
  onSegmentTimeChange: (index: number, newStart: number, newEnd: number) => void;
}
```

State internal:
- `videoRef` (ref ke `<video>`)
- `duration` (detik, dari `video.onloadedmetadata` → `video.duration`)
- `currentTime` (detik, update dari `video.ontimeupdate`, dipakai posisi playhead)
- `pxPerSec` (default 40, diubah slider zoom)
- `dragState: { index: number; mode: "move" | "trim-start" | "trim-end"; startX: number; origStart: number; origEnd: number } | null` — drag aktif, null kalau idle

Render:
- `<video ref={videoRef} src={convertFileSrc(videoPath)} controls />`
- Slider zoom: `<input type="range" min={5} max={200} value={pxPerSec} onChange={...} />`
- Container scroll-x (`overflow-x: auto`) berisi:
  - Ruler: tick tiap detik/beberapa detik tergantung `pxPerSec` (skip label kalau terlalu rapat, threshold sederhana misal render label tiap tick kalau `pxPerSec >= 20`, else tiap 5 detik), `onClick` di ruler → `videoRef.current.currentTime = clickX / pxPerSec`
  - Playhead: `<div className="timeline-playhead" style={{ left: currentTime * pxPerSec }} />`
  - Track segmen: tiap `SrtSegment` → `<div className="timeline-segment" style={{ left: seg.start * pxPerSec, width: (seg.end - seg.start) * pxPerSec }}>` dengan 2 handle anak (`.timeline-handle-left`, `.timeline-handle-right`) buat trim, badan blok buat move

Drag logic (native `pointerdown`/`pointermove`/`pointerup`, tanpa library):
```ts
function onHandlePointerDown(e: React.PointerEvent, index: number, mode: "move" | "trim-start" | "trim-end") {
  e.stopPropagation();
  const seg = segments[index];
  setDragState({ index, mode, startX: e.clientX, origStart: seg.start, origEnd: seg.end });
  (e.target as Element).setPointerCapture(e.pointerId);
}

function onPointerMove(e: React.PointerEvent) {
  if (!dragState) return;
  const deltaSec = (e.clientX - dragState.startX) / pxPerSec;
  const { index, mode, origStart, origEnd } = dragState;
  let newStart = origStart, newEnd = origEnd;
  if (mode === "move") {
    const shift = clamp(deltaSec, -origStart, duration - origEnd);
    newStart = origStart + shift;
    newEnd = origEnd + shift;
  } else if (mode === "trim-start") {
    newStart = clamp(origStart + deltaSec, 0, origEnd - MIN_SEGMENT_DURATION);
  } else if (mode === "trim-end") {
    newEnd = clamp(origEnd + deltaSec, origStart + MIN_SEGMENT_DURATION, duration);
  }
  setLivePreview({ index, start: newStart, end: newEnd }); // update visual, belum commit
}

function onPointerUp() {
  if (!dragState || !livePreview) { setDragState(null); return; }
  onSegmentTimeChange(livePreview.index, livePreview.start, livePreview.end);
  setDragState(null);
  setLivePreview(null);
}
```

`MIN_SEGMENT_DURATION = 0.1` (detik) — konstanta modul, cegah durasi nol/negatif.

Blok segmen pakai `livePreview` buat posisi render kalau sedang di-drag (index cocok), else pakai `seg.start`/`seg.end` asli — biar drag terasa responsif tanpa nunggu re-render dari parent tiap pixel gerak.

Auto-scroll playhead: `useEffect` yang jalan tiap `currentTime` berubah — kalau `currentTime * pxPerSec` di luar `scrollLeft`..`scrollLeft + clientWidth` container, set `scrollLeft` container biar playhead balik ke tengah viewport.

### 4. `src/App.tsx`

Handler baru (mirror pola `handleSegmentEdit` baris ~515):
```ts
function handleSegmentTimeChange(index: number, newStart: number, newEnd: number) {
  const updated = segments.map(s =>
    s.index === index
      ? { ...s, start: newStart, end: newEnd, start_time: secondsToSrtTime(newStart), end_time: secondsToSrtTime(newEnd) }
      : s
  );
  setSegments(updated);
}
```

Import `secondsToSrtTime` dari `src/lib/time.ts`.

Prop baru ke `TranscriptView`: `onSegmentTimeChange={handleSegmentTimeChange}` (di samping `onSegmentEdit` yang sudah ada, baris ~769).

### 5. `src/components/TranscriptView.tsx`

- Props interface tambah `onSegmentTimeChange: (index: number, newStart: number, newEnd: number) => void;`
- Import `SubtitleTimeline` dari `./SubtitleTimeline`
- Render `<SubtitleTimeline videoPath={videoPath} segments={segments} onSegmentTimeChange={onSegmentTimeChange} />` di `transcript-main`, sebelum `segments-list` (baris ~909)

`videoPath` sudah jadi prop `TranscriptView` (dipakai di baris 283 buat `videoName`), tidak perlu prop baru.

### 6. `src/styles.css`

Class baru: `.subtitle-timeline`, `.subtitle-timeline-video`, `.timeline-zoom-slider`, `.timeline-scroll`, `.timeline-ruler`, `.timeline-ruler-tick`, `.timeline-playhead`, `.timeline-track`, `.timeline-segment`, `.timeline-handle-left`, `.timeline-handle-right`. Ikuti konvensi warna/spacing yang sudah dipakai di `.segment-card`/`.ar-preview` sekitarnya.

## Data Flow

```
User buka step "transcript" → SubtitleTimeline render <video> + timeline dari `segments`
User drag handle kanan blok segmen #5
  → pointermove: hitung newEnd, update livePreview (visual saja)
  → pointerup: onSegmentTimeChange(5, seg.start, newEnd)
    → App.tsx: handleSegmentTimeChange → segments state ke-update (start/end + start_time/end_time)
      → TranscriptView re-render: segments-list & SubtitleTimeline sama-sama reflect start/end baru
User klik "Generate Clips"
  → invoke("clip_video", { segments, ... }) — segments yang dikirim sudah include timing hasil drag
    → backend burn subtitle sesuai start/end baru (tidak ada perubahan di commands.rs/py — path sudah ada)
```

## Error Handling

- `video.duration` belum ready (`NaN`/`Infinity` sebelum metadata load) → timeline render kosong/loading state sampai `onloadedmetadata` fire, tidak crash
- Drag melewati batas video (`< 0` atau `> duration`) → di-clamp di `onPointerMove`, tidak pernah kirim nilai di luar batas ke `onSegmentTimeChange`
- Drag bikin `end <= start` (drag kilat/cepat) → `MIN_SEGMENT_DURATION` clamp cegah ini di kedua mode trim
- `videoPath` file hilang/tidak bisa dibuka → `<video>` browser native error state (ikon broken), tidak block timeline (timeline masih render dari `segments`, cuma preview visual yang gagal) — konsisten dengan `<video>` step "ready" yang juga tidak ada error handling eksplisit

## Catatan Performa

Drag pakai `livePreview` lokal (bukan update `segments` App-level tiap pixel gerak) supaya tidak trigger re-render seluruh `TranscriptView` (termasuk `segments-list` yang bisa ratusan item) tiap pointermove — cuma `SubtitleTimeline` re-render, dan di situ pun cuma 1 blok yang berubah posisi. Commit ke parent state cuma sekali di `pointerup`. Video panjang (ratusan segmen) tetap tampil lewat scroll-x biasa — tidak virtualize blok timeline karena render `<div>` per segmen ringan (bukan komponen berat), skip optimisasi ini sampai ada bukti nyata terasa lambat.
