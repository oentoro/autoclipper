# Subtitle Timeline Drag Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let user drag-adjust subtitle segment start/end timing on a video-synced timeline in `TranscriptView`, like trimming clips in CapCut/DaVinci Resolve.

**Architecture:** Two new pure-logic modules (`src/lib/time.ts`, `src/lib/timelineMath.ts`, unit-tested with vitest) feed a new presentational component (`src/components/SubtitleTimeline.tsx`) that renders a `<video>` preview plus a zoomable, scrollable timeline built from native pointer events (no drag library). The component is wired into `TranscriptView.tsx` (new panel above the existing segment list) and `App.tsx` (new state-update handler, mirroring the existing `handleSegmentEdit` pattern). No Rust/Python changes — `segments` (with numeric `start`/`end`) already flows straight into `clip_video` for subtitle burn timing.

**Tech Stack:** React 18 + TypeScript (existing), vitest (new devDependency, first test runner in this frontend — Python side already uses assert-based tests with no framework, but no JS equivalent exists yet).

**Spec:** `docs/superpowers/specs/2026-09-01-subtitle-timeline-design.md`

## Global Constraints

- `MIN_SEGMENT_DURATION = 0.1` seconds — floor for any drag-produced segment duration
- Zoom range: 5–200 px/second, default 40
- `start_time`/`end_time` string format after edit: `HH:MM:SS,mmm` — must match `seconds_to_srt_time` in `scripts/transcribe.py:85` exactly (zero-padded hours/min/sec, 3-digit millis)
- Timeline covers the whole video, not per-section
- Drag adjusts timing only — no drag-to-reposition of subtitle text on screen (out of scope)
- No collision/overlap prevention between neighboring segments
- No waveform, no snap-to-neighbor, no undo/redo, no multi-select drag, no wheel-zoom (all out of scope per spec)

---

## Task 1: Time formatting helper + test infra

**Files:**
- Create: `src/lib/time.ts`
- Test: `src/lib/time.test.ts`
- Modify: `package.json` (add `vitest` devDependency + `test` script)

**Interfaces:**
- Consumes: nothing
- Produces: `secondsToSrtTime(seconds: number): string` from `src/lib/time.ts` — used by Task 5

- [ ] **Step 1: Install vitest**

Run: `npm install -D vitest`

- [ ] **Step 2: Add test script**

Modify `package.json` — change:
```json
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
```
to:
```json
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "test": "vitest run"
  },
```

- [ ] **Step 3: Write the failing test**

Create `src/lib/time.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { secondsToSrtTime } from "./time";

describe("secondsToSrtTime", () => {
  it("formats zero as 00:00:00,000", () => {
    expect(secondsToSrtTime(0)).toBe("00:00:00,000");
  });

  it("formats sub-minute seconds with millis", () => {
    expect(secondsToSrtTime(5.25)).toBe("00:00:05,250");
  });

  it("formats minutes and seconds", () => {
    expect(secondsToSrtTime(75.5)).toBe("00:01:15,500");
  });

  it("formats hours", () => {
    expect(secondsToSrtTime(3661.001)).toBe("01:01:01,001");
  });

  it("zero-pads millis to 3 digits", () => {
    expect(secondsToSrtTime(1.005)).toBe("00:00:01,005");
  });
});
```

- [ ] **Step 4: Run test to verify it fails**

Run: `npm test -- src/lib/time.test.ts`
Expected: FAIL — `Cannot find module './time'` (file doesn't exist yet)

- [ ] **Step 5: Write minimal implementation**

Create `src/lib/time.ts`:
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

- [ ] **Step 6: Run test to verify it passes**

Run: `npm test -- src/lib/time.test.ts`
Expected: PASS (5/5)

- [ ] **Step 7: Commit**

```bash
git add package.json package-lock.json src/lib/time.ts src/lib/time.test.ts
git commit -m "feat: tambah secondsToSrtTime helper + vitest setup"
```

---

## Task 2: Drag math helper

**Files:**
- Create: `src/lib/timelineMath.ts`
- Test: `src/lib/timelineMath.test.ts`

**Interfaces:**
- Consumes: nothing
- Produces: from `src/lib/timelineMath.ts` — used by Task 3:
  - `MIN_SEGMENT_DURATION: number` (= 0.1)
  - `clamp(value: number, min: number, max: number): number`
  - `computeMoveDrag(origStart: number, origEnd: number, deltaSec: number, duration: number): { start: number; end: number }`
  - `computeTrimStartDrag(origStart: number, origEnd: number, deltaSec: number): { start: number; end: number }`
  - `computeTrimEndDrag(origStart: number, origEnd: number, deltaSec: number, duration: number): { start: number; end: number }`

- [ ] **Step 1: Write the failing test**

Create `src/lib/timelineMath.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import {
  clamp,
  computeMoveDrag,
  computeTrimStartDrag,
  computeTrimEndDrag,
  MIN_SEGMENT_DURATION,
} from "./timelineMath";

describe("clamp", () => {
  it("passes through in-range values", () => {
    expect(clamp(5, 0, 10)).toBe(5);
  });
  it("clamps below min", () => {
    expect(clamp(-5, 0, 10)).toBe(0);
  });
  it("clamps above max", () => {
    expect(clamp(15, 0, 10)).toBe(10);
  });
});

describe("computeMoveDrag", () => {
  it("shifts both start and end by delta", () => {
    expect(computeMoveDrag(10, 12, 3, 100)).toEqual({ start: 13, end: 15 });
  });
  it("clamps shift so start does not go below 0", () => {
    expect(computeMoveDrag(2, 5, -10, 100)).toEqual({ start: 0, end: 3 });
  });
  it("clamps shift so end does not exceed duration", () => {
    expect(computeMoveDrag(95, 98, 10, 100)).toEqual({ start: 97, end: 100 });
  });
});

describe("computeTrimStartDrag", () => {
  it("moves start left/right within bounds", () => {
    expect(computeTrimStartDrag(10, 20, 3)).toEqual({ start: 13, end: 20 });
  });
  it("clamps at 0", () => {
    expect(computeTrimStartDrag(2, 20, -10)).toEqual({ start: 0, end: 20 });
  });
  it("does not cross MIN_SEGMENT_DURATION floor relative to end", () => {
    const result = computeTrimStartDrag(10, 10.15, 10);
    expect(result.start).toBeCloseTo(10.15 - MIN_SEGMENT_DURATION, 5);
    expect(result.end).toBe(10.15);
  });
  it("never goes negative even for a degenerate near-zero end", () => {
    const result = computeTrimStartDrag(0, 0.05, -5);
    expect(result.start).toBe(0);
  });
});

describe("computeTrimEndDrag", () => {
  it("moves end left/right within bounds", () => {
    expect(computeTrimEndDrag(10, 20, -3, 100)).toEqual({ start: 10, end: 17 });
  });
  it("clamps at duration", () => {
    expect(computeTrimEndDrag(10, 95, 20, 100)).toEqual({ start: 10, end: 100 });
  });
  it("does not cross MIN_SEGMENT_DURATION floor relative to start", () => {
    const result = computeTrimEndDrag(10, 10.15, -10, 100);
    expect(result.start).toBe(10);
    expect(result.end).toBeCloseTo(10 + MIN_SEGMENT_DURATION, 5);
  });
  it("never exceeds duration even for a degenerate near-duration start", () => {
    const result = computeTrimEndDrag(99.98, 100, 5, 100);
    expect(result.end).toBe(100);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test -- src/lib/timelineMath.test.ts`
Expected: FAIL — `Cannot find module './timelineMath'`

- [ ] **Step 3: Write minimal implementation**

Create `src/lib/timelineMath.ts`:
```ts
export const MIN_SEGMENT_DURATION = 0.1;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function computeMoveDrag(
  origStart: number,
  origEnd: number,
  deltaSec: number,
  duration: number
): { start: number; end: number } {
  const shift = clamp(deltaSec, -origStart, duration - origEnd);
  return { start: origStart + shift, end: origEnd + shift };
}

export function computeTrimStartDrag(
  origStart: number,
  origEnd: number,
  deltaSec: number
): { start: number; end: number } {
  const upper = Math.max(0, origEnd - MIN_SEGMENT_DURATION);
  const start = clamp(origStart + deltaSec, 0, upper);
  return { start, end: origEnd };
}

export function computeTrimEndDrag(
  origStart: number,
  origEnd: number,
  deltaSec: number,
  duration: number
): { start: number; end: number } {
  const lower = Math.min(duration, origStart + MIN_SEGMENT_DURATION);
  const end = clamp(origEnd + deltaSec, lower, duration);
  return { start: origStart, end };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test -- src/lib/timelineMath.test.ts`
Expected: PASS (12/12)

- [ ] **Step 5: Commit**

```bash
git add src/lib/timelineMath.ts src/lib/timelineMath.test.ts
git commit -m "feat: tambah pure function drag math buat subtitle timeline"
```

---

## Task 3: `SubtitleTimeline` component

**Files:**
- Create: `src/components/SubtitleTimeline.tsx`
- Modify: `src/styles.css` (append after line 780, before `.segments-list` at line 782)
- Modify: `src/i18n.tsx` (add `subtitleTimelineZoomLabel` to both `id` and `en` dicts)

**Interfaces:**
- Consumes: `secondsToSrtTime` NOT used here (used in Task 5); uses `clamp`, `computeMoveDrag`, `computeTrimStartDrag`, `computeTrimEndDrag` from Task 2's `src/lib/timelineMath.ts`
- Produces: default export `SubtitleTimeline` from `src/components/SubtitleTimeline.tsx`, props:
  ```ts
  interface Props {
    videoPath: string;
    segments: SrtSegment[];
    onSegmentTimeChange: (index: number, newStart: number, newEnd: number) => void;
  }
  ```
  Used by Task 4.

**No automated test for this file.** It's a DOM/pointer-event-driven presentational component; the project has no React component test setup (no jsdom/testing-library anywhere, including for the existing, much larger `TranscriptView.tsx`). All drag *math* is already covered by Task 2's unit tests — this task only wires that math to pointer events and renders. Verify with `npx tsc --noEmit` (type-check) after writing, then do a full manual smoke test once Task 5 wires it into the running app.

- [ ] **Step 1: Add i18n keys**

Modify `src/i18n.tsx` — in the `id` dict, change:
```ts
    censorImagePickLabel: "Pilih Gambar",
    censorImageEmptyHint: "Pilih gambar dulu",
```
to:
```ts
    censorImagePickLabel: "Pilih Gambar",
    censorImageEmptyHint: "Pilih gambar dulu",
    subtitleTimelineZoomLabel: "Zoom Timeline",
```

In the `en` dict, change:
```ts
    censorImagePickLabel: "Choose Image",
    censorImageEmptyHint: "Choose an image first",
```
to:
```ts
    censorImagePickLabel: "Choose Image",
    censorImageEmptyHint: "Choose an image first",
    subtitleTimelineZoomLabel: "Timeline Zoom",
```

- [ ] **Step 2: Write the component**

Create `src/components/SubtitleTimeline.tsx`:
```tsx
import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useLang } from "../i18n";
import type { SrtSegment } from "../types";
import {
  clamp,
  computeMoveDrag,
  computeTrimStartDrag,
  computeTrimEndDrag,
} from "../lib/timelineMath";

interface Props {
  videoPath: string;
  segments: SrtSegment[];
  onSegmentTimeChange: (index: number, newStart: number, newEnd: number) => void;
}

type DragMode = "move" | "trim-start" | "trim-end";

interface DragState {
  index: number;
  mode: DragMode;
  startX: number;
  origStart: number;
  origEnd: number;
}

interface LivePreview {
  index: number;
  start: number;
  end: number;
}

const MIN_PX_PER_SEC = 5;
const MAX_PX_PER_SEC = 200;
const DEFAULT_PX_PER_SEC = 40;

export default function SubtitleTimeline({ videoPath, segments, onSegmentTimeChange }: Props) {
  const { t } = useLang();
  const videoRef = useRef<HTMLVideoElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const didDragRef = useRef(false);
  const [duration, setDuration] = useState(0);
  const [currentTime, setCurrentTime] = useState(0);
  const [pxPerSec, setPxPerSec] = useState(DEFAULT_PX_PER_SEC);
  const [dragState, setDragState] = useState<DragState | null>(null);
  const [livePreview, setLivePreview] = useState<LivePreview | null>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const playheadX = currentTime * pxPerSec;
    if (playheadX < el.scrollLeft || playheadX > el.scrollLeft + el.clientWidth) {
      el.scrollLeft = Math.max(0, playheadX - el.clientWidth / 2);
    }
  }, [currentTime, pxPerSec]);

  function handleRulerClick(e: React.MouseEvent<HTMLDivElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const clickSec = (e.clientX - rect.left) / pxPerSec;
    if (videoRef.current) videoRef.current.currentTime = clamp(clickSec, 0, duration);
  }

  function handleSegmentClick(seg: SrtSegment) {
    if (didDragRef.current) {
      didDragRef.current = false;
      return;
    }
    if (videoRef.current) videoRef.current.currentTime = seg.start;
  }

  function handleHandlePointerDown(e: React.PointerEvent, index: number, mode: DragMode) {
    e.stopPropagation();
    const seg = segments.find(s => s.index === index);
    if (!seg) return;
    didDragRef.current = false;
    setDragState({ index, mode, startX: e.clientX, origStart: seg.start, origEnd: seg.end });
    (e.target as Element).setPointerCapture(e.pointerId);
  }

  function handlePointerMove(e: React.PointerEvent) {
    if (!dragState) return;
    didDragRef.current = true;
    const deltaSec = (e.clientX - dragState.startX) / pxPerSec;
    const { index, mode, origStart, origEnd } = dragState;
    const result =
      mode === "move" ? computeMoveDrag(origStart, origEnd, deltaSec, duration) :
      mode === "trim-start" ? computeTrimStartDrag(origStart, origEnd, deltaSec) :
      computeTrimEndDrag(origStart, origEnd, deltaSec, duration);
    setLivePreview({ index, start: result.start, end: result.end });
  }

  function handlePointerUp() {
    if (dragState && livePreview) {
      onSegmentTimeChange(livePreview.index, livePreview.start, livePreview.end);
    }
    setDragState(null);
    setLivePreview(null);
  }

  const tickInterval = pxPerSec >= 20 ? 1 : 5;
  const ticks: number[] = [];
  for (let s = 0; s <= duration; s += tickInterval) ticks.push(s);

  return (
    <div className="subtitle-timeline">
      <video
        ref={videoRef}
        key={videoPath}
        src={convertFileSrc(videoPath)}
        controls
        className="subtitle-timeline-video"
        onLoadedMetadata={e => setDuration(e.currentTarget.duration)}
        onTimeUpdate={e => setCurrentTime(e.currentTarget.currentTime)}
      />

      <div className="timeline-zoom-row">
        <label className="timeline-zoom-label">{t("subtitleTimelineZoomLabel")}</label>
        <input
          type="range"
          min={MIN_PX_PER_SEC}
          max={MAX_PX_PER_SEC}
          value={pxPerSec}
          onChange={e => setPxPerSec(Number(e.target.value))}
          className="timeline-zoom-slider"
        />
      </div>

      <div
        className="timeline-scroll"
        ref={scrollRef}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
      >
        <div className="timeline-content" style={{ width: duration * pxPerSec }}>
          <div className="timeline-ruler" onClick={handleRulerClick}>
            {ticks.map(sec => (
              <div key={sec} className="timeline-ruler-tick" style={{ left: sec * pxPerSec }}>
                <span className="timeline-ruler-label">{sec}s</span>
              </div>
            ))}
          </div>

          <div className="timeline-track">
            {segments.map(seg => {
              const live = livePreview && livePreview.index === seg.index ? livePreview : null;
              const start = live ? live.start : seg.start;
              const end = live ? live.end : seg.end;
              return (
                <div
                  key={seg.index}
                  className="timeline-segment"
                  style={{ left: start * pxPerSec, width: Math.max(2, (end - start) * pxPerSec) }}
                  onPointerDown={e => handleHandlePointerDown(e, seg.index, "move")}
                  onClick={() => handleSegmentClick(seg)}
                  title={seg.text}
                >
                  <div
                    className="timeline-handle-left"
                    onPointerDown={e => handleHandlePointerDown(e, seg.index, "trim-start")}
                  />
                  <span className="timeline-segment-text">{seg.text}</span>
                  <div
                    className="timeline-handle-right"
                    onPointerDown={e => handleHandlePointerDown(e, seg.index, "trim-end")}
                  />
                </div>
              );
            })}
          </div>

          <div className="timeline-playhead" style={{ left: currentTime * pxPerSec }} />
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Add CSS**

Modify `src/styles.css` — after line 780 (`.search-input:focus { border-color: var(--accent); }`), before line 782 (`.segments-list {`), insert:
```css

.subtitle-timeline {
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex-shrink: 0;
  background: var(--bg2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px;
}
.subtitle-timeline-video {
  width: 100%;
  max-height: 280px;
  border-radius: 8px;
  background: #000;
}
.timeline-zoom-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.timeline-zoom-label {
  font-size: 12px;
  color: var(--text2);
  white-space: nowrap;
}
.timeline-zoom-slider {
  flex: 1;
  max-width: 200px;
}
.timeline-scroll {
  overflow-x: auto;
  overflow-y: hidden;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg3);
}
.timeline-content {
  position: relative;
  min-width: 100%;
}
.timeline-ruler {
  position: relative;
  height: 22px;
  border-bottom: 1px solid var(--border);
  cursor: pointer;
}
.timeline-ruler-tick {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--border);
}
.timeline-ruler-label {
  position: absolute;
  top: 2px;
  left: 3px;
  font-size: 10px;
  color: var(--text2);
  white-space: nowrap;
}
.timeline-track {
  position: relative;
  height: 48px;
}
.timeline-segment {
  position: absolute;
  top: 6px;
  height: 36px;
  background: var(--accent);
  border-radius: 4px;
  cursor: grab;
  display: flex;
  align-items: center;
  overflow: hidden;
  user-select: none;
}
.timeline-segment:active { cursor: grabbing; }
.timeline-segment-text {
  font-size: 11px;
  color: #fff;
  padding: 0 8px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
}
.timeline-handle-left, .timeline-handle-right {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 6px;
  background: rgba(255, 255, 255, 0.35);
  cursor: ew-resize;
}
.timeline-handle-left { left: 0; }
.timeline-handle-right { right: 0; }
.timeline-playhead {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 2px;
  background: var(--accent2);
  pointer-events: none;
}
```

- [ ] **Step 4: Type-check**

Run: `npx tsc --noEmit`
Expected: no errors (this file isn't imported anywhere yet, so it's checked standalone — confirms no typos/type mismatches before wiring it in)

- [ ] **Step 5: Commit**

```bash
git add src/components/SubtitleTimeline.tsx src/styles.css src/i18n.tsx
git commit -m "feat: komponen SubtitleTimeline (video preview + drag timeline)"
```

---

## Task 4: Wire `SubtitleTimeline` into `TranscriptView`

**Files:**
- Modify: `src/components/TranscriptView.tsx`

**Interfaces:**
- Consumes: `SubtitleTimeline` component + its `Props` from Task 3 (`src/components/SubtitleTimeline.tsx`)
- Produces: `TranscriptView` gains a new required prop `onSegmentTimeChange: (index: number, newStart: number, newEnd: number) => void` — used by Task 5

- [ ] **Step 1: Import the component**

Modify `src/components/TranscriptView.tsx` — change line 4:
```ts
import { useState, useEffect, useRef } from "react";
```
to:
```ts
import { useState, useEffect, useRef } from "react";
import SubtitleTimeline from "./SubtitleTimeline";
```

- [ ] **Step 2: Add the prop to the interface**

Modify the `Props` interface — change:
```ts
  onSegmentEdit: (index: number, newText: string) => void;
```
to:
```ts
  onSegmentEdit: (index: number, newText: string) => void;
  onSegmentTimeChange: (index: number, newStart: number, newEnd: number) => void;
```

- [ ] **Step 3: Destructure the new prop**

Modify the component's destructured params — change:
```ts
  onSegmentEdit,
```
to:
```ts
  onSegmentEdit,
  onSegmentTimeChange,
```

- [ ] **Step 4: Render the timeline panel**

Modify the JSX — change:
```tsx
      {/* ── Main area ── */}
      <div className="transcript-main">
        <div className="transcript-toolbar">
```
to:
```tsx
      {/* ── Main area ── */}
      <div className="transcript-main">
        <SubtitleTimeline
          videoPath={videoPath}
          segments={segments}
          onSegmentTimeChange={onSegmentTimeChange}
        />

        <div className="transcript-toolbar">
```

- [ ] **Step 5: Type-check**

Run: `npx tsc --noEmit`
Expected: error — `App.tsx` renders `<TranscriptView ... />` without the new required `onSegmentTimeChange` prop. This confirms the prop is correctly required; Task 5 fixes the call site.

- [ ] **Step 6: Commit**

```bash
git add src/components/TranscriptView.tsx
git commit -m "feat: render SubtitleTimeline di TranscriptView"
```

---

## Task 5: Wire into `App.tsx` (state handler)

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `secondsToSrtTime` from Task 1 (`src/lib/time.ts`); `TranscriptView`'s new `onSegmentTimeChange` prop from Task 4
- Produces: fully working feature — nothing downstream depends on this task

- [ ] **Step 1: Import the time helper**

Modify `src/App.tsx` — change line 12:
```ts
import type { SrtSegment, TranscribeResult, TranslateResult, AnalyzeResult, ClassifyResult, Section, ClipResult, AppStep, DepsStatus, FontInfo, LlmModel, DownloadProgress, SubtitleStyle, LicenseInfo, YtDownloadProgress, ManualClip } from "./types";
```
to:
```ts
import type { SrtSegment, TranscribeResult, TranslateResult, AnalyzeResult, ClassifyResult, Section, ClipResult, AppStep, DepsStatus, FontInfo, LlmModel, DownloadProgress, SubtitleStyle, LicenseInfo, YtDownloadProgress, ManualClip } from "./types";
import { secondsToSrtTime } from "./lib/time";
```

- [ ] **Step 2: Add the handler**

Modify `src/App.tsx` — change:
```ts
  function handleSegmentEdit(index: number, newText: string) {
    const updated = segments.map(s =>
      s.index === index ? { ...s, text: newText } : s
    );
    setSegments(updated);
    setSrtContent(
      updated.map(s => `${s.index}\n${s.start_time} --> ${s.end_time}\n${s.text}\n`).join("\n")
    );
  }
```
to:
```ts
  function handleSegmentEdit(index: number, newText: string) {
    const updated = segments.map(s =>
      s.index === index ? { ...s, text: newText } : s
    );
    setSegments(updated);
    setSrtContent(
      updated.map(s => `${s.index}\n${s.start_time} --> ${s.end_time}\n${s.text}\n`).join("\n")
    );
  }

  function handleSegmentTimeChange(index: number, newStart: number, newEnd: number) {
    const updated = segments.map(s =>
      s.index === index
        ? { ...s, start: newStart, end: newEnd, start_time: secondsToSrtTime(newStart), end_time: secondsToSrtTime(newEnd) }
        : s
    );
    setSegments(updated);
    setSrtContent(
      updated.map(s => `${s.index}\n${s.start_time} --> ${s.end_time}\n${s.text}\n`).join("\n")
    );
  }
```

- [ ] **Step 3: Pass the prop**

Modify the `<TranscriptView>` call — change:
```tsx
            onSegmentEdit={handleSegmentEdit}
```
to:
```tsx
            onSegmentEdit={handleSegmentEdit}
            onSegmentTimeChange={handleSegmentTimeChange}
```

- [ ] **Step 4: Type-check**

Run: `npx tsc --noEmit`
Expected: no errors

- [ ] **Step 5: Run the full test suite**

Run: `npm test`
Expected: all tests pass (17/17 across `time.test.ts` + `timelineMath.test.ts`)

- [ ] **Step 6: Manual smoke test**

Run: `npm run tauri dev`
- Load a video through the normal upload → transcribe flow, reach the "transcript" step
- Confirm the video preview + timeline render above the segment list
- Drag a segment's right handle right → confirm the block widens, the segment card below shows the new `end_time`, and it persists after releasing
- Drag a segment's left handle → confirm `start_time` updates the same way
- Drag a segment's body → confirm both times shift together, duration unchanged
- Click a segment block → confirm video seeks to that segment's start
- Play the video → confirm the playhead moves and the timeline auto-scrolls to keep it visible
- Move the zoom slider → confirm blocks resize accordingly
- Generate a clip with burned subtitles from a video where you dragged a segment's timing → confirm the output's subtitle timing reflects the drag (not the original transcribed timing)

- [ ] **Step 7: Commit**

```bash
git add src/App.tsx
git commit -m "feat: wire SubtitleTimeline drag ke App state"
```
