import { useEffect, useMemo, useRef, useState } from "react";
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
  const safeDuration = Number.isFinite(duration) && duration > 0 ? duration : 0;

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
    if (videoRef.current) videoRef.current.currentTime = clamp(clickSec, 0, safeDuration);
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
      mode === "move" ? computeMoveDrag(origStart, origEnd, deltaSec, safeDuration) :
      mode === "trim-start" ? computeTrimStartDrag(origStart, origEnd, deltaSec) :
      computeTrimEndDrag(origStart, origEnd, deltaSec, safeDuration);
    setLivePreview({ index, start: result.start, end: result.end });
  }

  function handlePointerUp() {
    if (dragState && livePreview) {
      onSegmentTimeChange(livePreview.index, livePreview.start, livePreview.end);
    }
    setDragState(null);
    setLivePreview(null);
  }

  const tickInterval = Math.max(1, Math.ceil(70 / pxPerSec));
  const ticks = useMemo(() => {
    const result: number[] = [];
    for (let s = 0; s <= safeDuration; s += tickInterval) result.push(s);
    return result;
  }, [safeDuration, tickInterval]);

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
        <div className="timeline-content" style={{ width: safeDuration * pxPerSec }}>
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
