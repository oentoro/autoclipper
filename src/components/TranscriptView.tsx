import { useState } from "react";
import type { SrtSegment } from "../types";

interface Props {
  segments: SrtSegment[];
  selectedIndices: Set<number>;
  aiReasoning: string;
  onToggle: (index: number) => void;
  onSelectAll: () => void;
  onClearAll: () => void;
  onAiAnalyze: () => void;
  onClip: () => void;
  onSaveSrt: () => void;
  loading: boolean;
  videoPath: string;
}

export default function TranscriptView({
  segments,
  selectedIndices,
  aiReasoning,
  onToggle,
  onSelectAll,
  onClearAll,
  onAiAnalyze,
  onClip,
  onSaveSrt,
  loading,
  videoPath,
}: Props) {
  const [searchText, setSearchText] = useState("");
  const [showSrt, setShowSrt] = useState(false);

  const videoName = videoPath.split("/").pop() ?? videoPath;

  const filtered = searchText
    ? segments.filter((s) => s.text.toLowerCase().includes(searchText.toLowerCase()))
    : segments;

  return (
    <div className="transcript-layout">
      <div className="transcript-sidebar">
        <div className="sidebar-section">
          <p className="sidebar-label">File Video</p>
          <p className="sidebar-value" title={videoPath}>{videoName}</p>
        </div>

        <div className="sidebar-section">
          <p className="sidebar-label">Transkrip</p>
          <p className="sidebar-value">{segments.length} segmen</p>
        </div>

        <div className="sidebar-section">
          <p className="sidebar-label">Dipilih</p>
          <p className="sidebar-value selected-count">{selectedIndices.size} segmen</p>
        </div>

        <div className="sidebar-actions">
          <button
            className="btn btn-primary w-full"
            onClick={onAiAnalyze}
            disabled={loading}
          >
            🤖 Analisis AI
          </button>
          <button className="btn btn-secondary w-full" onClick={onSelectAll}>
            ✓ Pilih Semua
          </button>
          <button className="btn btn-ghost w-full" onClick={onClearAll}>
            ✕ Hapus Pilihan
          </button>
          <button className="btn btn-ghost w-full" onClick={onSaveSrt}>
            💾 Simpan SRT
          </button>
        </div>

        {aiReasoning && (
          <div className="ai-reasoning">
            <p className="sidebar-label">Alasan AI</p>
            <p className="reasoning-text">{aiReasoning}</p>
          </div>
        )}

        <button
          className="btn btn-clip w-full"
          onClick={onClip}
          disabled={selectedIndices.size === 0 || loading}
        >
          ✂ Buat {selectedIndices.size} Clip
        </button>
      </div>

      <div className="transcript-main">
        <div className="transcript-toolbar">
          <input
            className="search-input"
            type="text"
            placeholder="Cari teks..."
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
          />
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => setShowSrt(!showSrt)}
          >
            {showSrt ? "Tampilan Kartu" : "Tampilan SRT"}
          </button>
        </div>

        <div className="segments-list">
          {filtered.map((seg) => (
            <div
              key={seg.index}
              className={`segment-card ${selectedIndices.has(seg.index) ? "selected" : ""}`}
              onClick={() => onToggle(seg.index)}
            >
              <div className="segment-header">
                <div className="segment-check">
                  {selectedIndices.has(seg.index) ? "✓" : "○"}
                </div>
                <span className="segment-index">#{seg.index}</span>
                <span className="segment-time">
                  {seg.start_time} → {seg.end_time}
                </span>
                <span className="segment-duration">
                  {(seg.end - seg.start).toFixed(1)}s
                </span>
              </div>
              <p className="segment-text">{seg.text}</p>
            </div>
          ))}

          {filtered.length === 0 && (
            <div className="empty-state">
              <p>Tidak ada segmen yang cocok dengan pencarian</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
