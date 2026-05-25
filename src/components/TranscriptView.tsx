import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SrtSegment, Section, FontInfo, LlmModel, SubtitleStyle } from "../types";

function hexToRgb(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `${r}, ${g}, ${b}`;
}

const ASPECT_RATIOS = [
  { value: "original", label: "Original", w: 16, h: 9,  desc: "Asli"          },
  { value: "16:9",     label: "16:9",     w: 16, h: 9,  desc: "Landscape"     },
  { value: "9:16",     label: "9:16",     w: 9,  h: 16, desc: "Shorts/Reels"  },
  { value: "1:1",      label: "1:1",      w: 1,  h: 1,  desc: "Square"        },
  { value: "4:5",      label: "4:5",      w: 4,  h: 5,  desc: "Instagram"     },
] as const;

interface Props {
  segments: SrtSegment[];
  selectedIndices: Set<number>;
  sections: Section[];
  burnSubtitles: boolean;
  aspectRatio: string;
  onToggle: (index: number) => void;
  onSelectAll: () => void;
  onClearAll: () => void;
  onClassify: () => void;
  onToggleSection: (sectionIdx: number) => void;
  onClip: () => void;
  onSaveSrt: () => void;
  onBurnSubtitlesChange: (v: boolean) => void;
  onAspectRatioChange: (v: string) => void;
  subtitleFontSize: number;
  subtitleFont: string;
  systemFonts: FontInfo[];
  onSubtitleFontSizeChange: (v: number) => void;
  onSubtitleFontChange: (v: string) => void;
  subtitleStyle: SubtitleStyle;
  onSubtitleStyleChange: (s: SubtitleStyle) => void;
  loading: boolean;
  videoPath: string;
  detectedLanguage: string;
  translateTarget: string;
  translateLangs: { code: string; label: string }[];
  onTranslate: () => void;
  onTranslateTargetChange: (v: string) => void;
  llmModels: LlmModel[];
  selectedLlm: LlmModel | null;
  onLlmChange: (m: LlmModel) => void;
  onManageModels: () => void;
}

function sectionStatus(section: Section, segments: SrtSegment[], selectedIndices: Set<number>) {
  const inSection = segments.filter(s => s.index >= section.start_index && s.index <= section.end_index);
  if (inSection.length === 0) return "empty";
  const selected = inSection.filter(s => selectedIndices.has(s.index)).length;
  if (selected === 0) return "none";
  if (selected === inSection.length) return "all";
  return "partial";
}

function formatTime(secs: number) {
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

export default function TranscriptView({
  segments,
  selectedIndices,
  sections,
  burnSubtitles,
  aspectRatio,
  onToggle,
  onSelectAll,
  onClearAll,
  onClassify,
  onToggleSection,
  onClip,
  onSaveSrt,
  onBurnSubtitlesChange,
  onAspectRatioChange,
  subtitleFontSize,
  subtitleFont,
  systemFonts,
  onSubtitleFontSizeChange,
  onSubtitleFontChange,
  subtitleStyle,
  onSubtitleStyleChange,
  loading,
  videoPath,
  detectedLanguage,
  translateTarget,
  translateLangs,
  onTranslate,
  onTranslateTargetChange,
  llmModels,
  selectedLlm,
  onLlmChange,
  onManageModels,
}: Props) {
  const [searchText, setSearchText] = useState("");
  const [showSrt, setShowSrt] = useState(false);
  const [previewReady, setPreviewReady] = useState(false);
  const fontStyleRef = useRef<HTMLStyleElement | null>(null);

  // Dynamically load the selected font file and inject a @font-face rule
  useEffect(() => {
    if (!fontStyleRef.current) {
      const el = document.createElement("style");
      el.id = "ac-font-preview";
      document.head.appendChild(el);
      fontStyleRef.current = el;
    }
    if (!subtitleFont) {
      fontStyleRef.current.textContent = "";
      setPreviewReady(false);
      return;
    }
    setPreviewReady(false);
    invoke<string>("read_font_base64", { path: subtitleFont }).then(b64 => {
      const ext = subtitleFont.split(".").pop()?.toLowerCase() ?? "ttf";
      const mime =
        ext === "otf" ? "font/opentype" :
        ext === "woff2" ? "font/woff2" :
        ext === "woff"  ? "font/woff"  : "font/truetype";
      fontStyleRef.current!.textContent =
        `@font-face { font-family: 'ac-preview'; src: url('data:${mime};base64,${b64}'); }`;
      setPreviewReady(true);
    }).catch(() => { /* font unreadable — silently skip */ });
  }, [subtitleFont]);

  const videoName = videoPath.split("/").pop() ?? videoPath;

  const hasSections = sections.length > 0;

  // When sections exist and no search, group by section; otherwise flat list
  const flatFiltered = searchText
    ? segments.filter(s => s.text.toLowerCase().includes(searchText.toLowerCase()))
    : segments;

  // For a given section, get its segments
  function segmentsOf(section: Section) {
    return segments.filter(s => s.index >= section.start_index && s.index <= section.end_index);
  }

  // Start/end time of a section derived from actual segments
  function sectionTimeRange(section: Section) {
    const segs = segmentsOf(section);
    if (segs.length === 0) return "";
    return `${formatTime(segs[0].start)} – ${formatTime(segs[segs.length - 1].end)}`;
  }

  return (
    <div className="transcript-layout">
      {/* ── Sidebar ── */}
      <div className="transcript-sidebar">

        {/* File & stats */}
        <div className="sidebar-block">
          <p className="sidebar-label">File Video</p>
          <p className="sidebar-value" title={videoPath}>{videoName}</p>
        </div>

        <div className="sidebar-block sidebar-stats-row">
          <div>
            <p className="sidebar-label">Segmen</p>
            <p className="sidebar-value">{segments.length}</p>
          </div>
          <div className="sidebar-stats-right">
            <p className="sidebar-label">Dipilih</p>
            <p className="sidebar-value selected-count">{selectedIndices.size}</p>
          </div>
        </div>

        {/* Model AI */}
        <div className="sidebar-block">
          <div className="llm-select-section">
            <div className="llm-select-header">
              <p className="sidebar-label" style={{ margin: 0 }}>
                Model AI
                {selectedLlm && (
                  <span className={`llm-source-badge ${selectedLlm.source}`}>
                    {selectedLlm.source === "local" ? "Lokal" : "Ollama"}
                  </span>
                )}
              </p>
              <button className="llm-manage-btn" onClick={onManageModels} title="Unduh / kelola model">
                + Kelola
              </button>
            </div>
            {llmModels.length === 0 ? (
              <button className="llm-empty-btn" onClick={onManageModels}>
                ⬇ Unduh model AI...
              </button>
            ) : (
              <select
                className="llm-select"
                value={selectedLlm?.path || selectedLlm?.ollama_model || ""}
                onChange={e => {
                  const m = llmModels.find(
                    lm => (lm.source === "local" ? lm.path : lm.ollama_model) === e.target.value
                  );
                  if (m) onLlmChange(m);
                }}
                disabled={loading}
              >
                {llmModels.map(m => {
                  const val = m.source === "local" ? m.path : m.ollama_model;
                  const size = m.size_mb > 0 ? ` (${(m.size_mb / 1024).toFixed(1)} GB)` : "";
                  return (
                    <option key={val} value={val}>
                      {m.name}{size}
                    </option>
                  );
                })}
              </select>
            )}
          </div>
        </div>

        {/* Actions */}
        <div className="sidebar-block">
          <div className="sidebar-actions">
            <button
              className="btn btn-primary w-full"
              onClick={onClassify}
              disabled={loading || llmModels.length === 0}
            >
              🗂 Klasifikasi Bagian
            </button>
            <div className="sidebar-actions-row">
              <button className="btn btn-secondary" onClick={onSelectAll}>✓ Pilih Semua</button>
              <button className="btn btn-ghost" onClick={onClearAll}>✕ Hapus</button>
            </div>
            <button className="btn btn-ghost w-full" onClick={onSaveSrt}>💾 Simpan SRT</button>
          </div>
        </div>

        {/* Translate */}
        <div className="sidebar-block">
          <div className="translate-section">
            <p className="sidebar-label">
              Terjemahan
              {detectedLanguage && (
                <span className="detected-lang-badge">{detectedLanguage.toUpperCase()}</span>
              )}
            </p>
            <div className="translate-row">
              <select
                className="translate-select"
                value={translateTarget}
                onChange={e => onTranslateTargetChange(e.target.value)}
                disabled={loading}
              >
                {translateLangs
                  .filter(l => l.code !== detectedLanguage)
                  .map(l => (
                    <option key={l.code} value={l.code}>{l.label}</option>
                  ))}
              </select>
              <button
                className="btn-translate"
                onClick={onTranslate}
                disabled={loading || segments.length === 0}
              >
                ⇄ Terjemahkan
              </button>
            </div>
          </div>
        </div>

        {/* Aspect Ratio */}
        <div className="sidebar-block">
          <p className="sidebar-label">Aspect Ratio</p>
          <div className="ar-grid">
            {ASPECT_RATIOS.map(ar => {
              const scale = 32 / Math.max(ar.w, ar.h);
              const bw = Math.round(ar.w * scale);
              const bh = Math.round(ar.h * scale);
              return (
                <button
                  key={ar.value}
                  className={`ar-btn ${aspectRatio === ar.value ? "active" : ""}`}
                  onClick={() => onAspectRatioChange(ar.value)}
                  title={ar.desc}
                >
                  <div className="ar-preview" style={{ width: bw, height: bh }} />
                  <span className="ar-label">{ar.label}</span>
                  <span className="ar-desc">{ar.desc}</span>
                </button>
              );
            })}
          </div>
        </div>

        {/* Subtitle */}
        <div className="sidebar-block">
          <label className="subtitle-toggle">
            <input
              type="checkbox"
              checked={burnSubtitles}
              onChange={e => onBurnSubtitlesChange(e.target.checked)}
            />
            <span className="subtitle-toggle-label">
              <span className="subtitle-toggle-icon">CC</span>
              Bakar Subtitle
            </span>
          </label>
          {burnSubtitles && (
            <div className="subtitle-settings">
              <div className="subtitle-setting-row">
                <span className="sidebar-label">Ukuran Teks</span>
                <span className="font-size-badge">
                  {subtitleFontSize === 0 ? "Auto" : `${subtitleFontSize}px`}
                </span>
              </div>
              <div className="font-size-slider-row">
                <span className="font-size-hint">A</span>
                <input
                  type="range"
                  className="font-size-slider"
                  min={0} max={72} step={2}
                  value={subtitleFontSize}
                  onChange={e => onSubtitleFontSizeChange(Number(e.target.value))}
                />
                <span className="font-size-hint large">A</span>
                {subtitleFontSize > 0 && (
                  <button className="font-reset-btn" onClick={() => onSubtitleFontSizeChange(0)} title="Reset ke Auto">↺</button>
                )}
              </div>
              <span className="sidebar-label" style={{ marginTop: 8, display: "block" }}>Font</span>
              <select
                className="font-select"
                value={subtitleFont}
                onChange={e => onSubtitleFontChange(e.target.value)}
              >
                <option value="">Default (otomatis)</option>
                {systemFonts.map(f => (
                  <option key={f.path} value={f.path}>{f.name}</option>
                ))}
              </select>
              <div
                className="font-preview-box"
                style={{
                  fontFamily: previewReady ? "'ac-preview', sans-serif" : "sans-serif",
                  fontSize: subtitleFontSize > 0 ? `${subtitleFontSize}px` : "16px",
                  color: subtitleStyle.textColor,
                  background: subtitleStyle.boxEnabled
                    ? `rgba(${hexToRgb(subtitleStyle.boxColor)}, ${subtitleStyle.boxOpacity / 100})`
                    : "#111",
                  WebkitTextStroke: subtitleStyle.outlineWidth > 0
                    ? `${subtitleStyle.outlineWidth}px ${subtitleStyle.outlineColor}`
                    : undefined,
                  justifyContent:
                    subtitleStyle.position === "top" ? "flex-start" :
                    subtitleStyle.position === "center" ? "center" : "flex-end",
                  alignItems: "center",
                }}
              >
                {subtitleStyle.allCaps ? "TEKS SUBTITLE CONTOH" : "Teks Subtitle Contoh"}
                {!previewReady && subtitleFont && (
                  <span className="font-preview-loading">memuat...</span>
                )}
              </div>

              {/* ── Style controls ── */}
              <div className="subtitle-style-section">
                {/* Colors row */}
                <div className="style-colors-row">
                  <label className="style-color-label">
                    <span>Teks</span>
                    <input
                      type="color"
                      value={subtitleStyle.textColor}
                      onChange={e => onSubtitleStyleChange({ ...subtitleStyle, textColor: e.target.value })}
                    />
                  </label>
                  <label className="style-color-label">
                    <span>Outline</span>
                    <input
                      type="color"
                      value={subtitleStyle.outlineColor}
                      onChange={e => onSubtitleStyleChange({ ...subtitleStyle, outlineColor: e.target.value })}
                    />
                  </label>
                </div>

                {/* Outline width */}
                <div className="style-row">
                  <span className="sidebar-label" style={{ flex: 1 }}>Tebal Outline</span>
                  <span className="font-size-badge">{subtitleStyle.outlineWidth}px</span>
                </div>
                <input
                  type="range"
                  className="font-size-slider"
                  min={0} max={8} step={1}
                  value={subtitleStyle.outlineWidth}
                  onChange={e => onSubtitleStyleChange({ ...subtitleStyle, outlineWidth: Number(e.target.value) })}
                />

                {/* Position */}
                <span className="sidebar-label" style={{ display: "block", marginTop: 6 }}>Posisi</span>
                <div className="style-position-row">
                  {(["top", "center", "bottom"] as const).map(pos => (
                    <button
                      key={pos}
                      className={`style-pos-btn ${subtitleStyle.position === pos ? "active" : ""}`}
                      onClick={() => onSubtitleStyleChange({ ...subtitleStyle, position: pos })}
                    >
                      {pos === "top" ? "Atas" : pos === "center" ? "Tengah" : "Bawah"}
                    </button>
                  ))}
                </div>

                {/* All caps */}
                <label className="style-box-toggle">
                  <input
                    type="checkbox"
                    checked={subtitleStyle.allCaps}
                    onChange={e => onSubtitleStyleChange({ ...subtitleStyle, allCaps: e.target.checked })}
                  />
                  <span>HURUF KAPITAL SEMUA</span>
                </label>

                {/* Background box */}
                <label className="style-box-toggle">
                  <input
                    type="checkbox"
                    checked={subtitleStyle.boxEnabled}
                    onChange={e => onSubtitleStyleChange({ ...subtitleStyle, boxEnabled: e.target.checked })}
                  />
                  <span>Background Box</span>
                </label>
                {subtitleStyle.boxEnabled && (
                  <div className="style-box-settings">
                    <label className="style-color-label">
                      <span>Warna</span>
                      <input
                        type="color"
                        value={subtitleStyle.boxColor}
                        onChange={e => onSubtitleStyleChange({ ...subtitleStyle, boxColor: e.target.value })}
                      />
                    </label>
                    <div className="style-row" style={{ flex: 1 }}>
                      <input
                        type="range"
                        className="font-size-slider"
                        min={0} max={100} step={5}
                        value={subtitleStyle.boxOpacity}
                        onChange={e => onSubtitleStyleChange({ ...subtitleStyle, boxOpacity: Number(e.target.value) })}
                      />
                      <span className="font-size-badge" style={{ marginLeft: 6 }}>{subtitleStyle.boxOpacity}%</span>
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Clip button — pinned at bottom */}
        <div className="sidebar-block sidebar-clip-block">
          <button
            className="btn btn-clip w-full"
            onClick={onClip}
            disabled={selectedIndices.size === 0 || loading}
          >
            <span>✂ Buat Klip</span>
            {selectedIndices.size > 0 && (
              <span className="btn-clip-count">{selectedIndices.size} segmen</span>
            )}
          </button>
        </div>

      </div>

      {/* ── Main area ── */}
      <div className="transcript-main">
        <div className="transcript-toolbar">
          <input
            className="search-input"
            type="text"
            placeholder="Cari teks..."
            value={searchText}
            onChange={e => setSearchText(e.target.value)}
          />
          <button className="btn btn-ghost btn-sm" onClick={() => setShowSrt(!showSrt)}>
            {showSrt ? "Tampilan Kartu" : "Tampilan SRT"}
          </button>
        </div>

        <div className="segments-list">
          {/* Section-grouped view */}
          {hasSections && !searchText ? (
            sections.map((section, sIdx) => {
              const status = sectionStatus(section, segments, selectedIndices);
              const segs = segmentsOf(section);
              return (
                <div key={sIdx} className={`section-group ${status === "all" ? "section-selected" : status === "partial" ? "section-partial" : ""}`}>
                  <button
                    className="section-header"
                    onClick={() => onToggleSection(sIdx)}
                  >
                    <div className="section-check">
                      {status === "all" ? "✓" : status === "partial" ? "◑" : "○"}
                    </div>
                    <div className="section-header-body">
                      <div className="section-header-top">
                        <span className="section-num">{sIdx + 1}</span>
                        <span className="section-name">{section.name}</span>
                        <span className="section-time">{sectionTimeRange(section)}</span>
                        <span className="section-count">{segs.length} segmen</span>
                      </div>
                      <p className="section-summary">{section.summary}</p>
                    </div>
                  </button>

                  <div className="section-segments">
                    {segs.map(seg => (
                      <div
                        key={seg.index}
                        className={`segment-card compact ${selectedIndices.has(seg.index) ? "selected" : ""}`}
                        onClick={() => onToggle(seg.index)}
                      >
                        <div className="segment-header">
                          <div className="segment-check">
                            {selectedIndices.has(seg.index) ? "✓" : "○"}
                          </div>
                          <span className="segment-index">#{seg.index}</span>
                          <span className="segment-time">{seg.start_time} → {seg.end_time}</span>
                          <span className="segment-duration">{(seg.end - seg.start).toFixed(1)}s</span>
                        </div>
                        <p className="segment-text">{seg.text}</p>
                      </div>
                    ))}
                  </div>
                </div>
              );
            })
          ) : (
            /* Flat list (no sections yet, or search active) */
            <>
              {flatFiltered.map(seg => (
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
                    <span className="segment-time">{seg.start_time} → {seg.end_time}</span>
                    <span className="segment-duration">{(seg.end - seg.start).toFixed(1)}s</span>
                  </div>
                  <p className="segment-text">{seg.text}</p>
                </div>
              ))}

              {flatFiltered.length === 0 && (
                <div className="empty-state">
                  <p>Tidak ada segmen yang cocok dengan pencarian</p>
                </div>
              )}

              {!hasSections && segments.length > 0 && !searchText && (
                <div className="classify-hint">
                  <p>Klik <strong>Klasifikasi Bagian</strong> agar AI mengelompokkan segmen berdasarkan topik — lalu pilih bagian yang ingin dijadikan video.</p>
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
