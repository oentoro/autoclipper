import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLang } from "../i18n";
import type { SrtSegment, Section, FontInfo, LlmModel, SubtitleStyle, ManualClip } from "../types";

function hexToRgb(hex: string): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `${r}, ${g}, ${b}`;
}

const ASPECT_RATIOS = [
  { value: "original",  label: "Original", w: 16, h: 9,  desc: "Asli"          },
  { value: "16:9",      label: "16:9",     w: 16, h: 9,  desc: "Landscape"     },
  { value: "9:16",      label: "9:16",     w: 9,  h: 16, desc: "Crop"          },
  { value: "9:16-fit",  label: "9:16 ↕",  w: 9,  h: 16, desc: "Fit Tengah"    },
  { value: "1:1",       label: "1:1",      w: 1,  h: 1,  desc: "Square"        },
  { value: "4:5",       label: "4:5",      w: 4,  h: 5,  desc: "Instagram"     },
] as const;

interface Props {
  segments: SrtSegment[];
  selectedIndices: Set<number>;
  sections: Section[];
  burnSubtitles: boolean;
  aspectRatio: string;
  smartCrop: boolean;
  onToggle: (index: number) => void;
  onSelectAll: () => void;
  onClearAll: () => void;
  onClassify: () => void;
  onAnalyze: () => void;
  onToggleSection: (sectionIdx: number) => void;
  onClip: () => void;
  onSaveSrt: () => void;
  onSegmentEdit: (index: number, newText: string) => void;
  onBurnSubtitlesChange: (v: boolean) => void;
  subtitleMode: "translated_only" | "bilingual" | "original_only";
  onSubtitleModeChange: (v: "translated_only" | "bilingual" | "original_only") => void;
  hasTranslation: boolean;
  onAspectRatioChange: (v: string) => void;
  onSmartCropChange: (v: boolean) => void;
  smartCropTransition: "smooth" | "aggressive";
  onSmartCropTransitionChange: (v: "smooth" | "aggressive") => void;
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
  transcribeDuration?: number;
  maxSectionSecs: number;
  onMaxSectionSecsChange: (v: number) => void;
  manualClips: ManualClip[];
  onAddManualClip: (startSec: number, endSec: number) => void;
  onRemoveManualClip: (id: string) => void;
  verticalTitle: string;
  verticalTitleFontSize: number;
  verticalTitleColor: string;
  onVerticalTitleChange: (v: string) => void;
  onVerticalTitleFontSizeChange: (v: number) => void;
  onVerticalTitleColorChange: (v: string) => void;
}

function parseTimestamp(ts: string): number | null {
  const parts = ts.trim().split(':').map(Number);
  if (parts.some(isNaN)) return null;
  if (parts.length === 2) {
    const [m, s] = parts;
    if (s >= 60) return null;
    return m * 60 + s;
  }
  if (parts.length === 3) {
    const [h, m, s] = parts;
    if (m >= 60 || s >= 60) return null;
    return h * 3600 + m * 60 + s;
  }
  return null;
}

function fmtTs(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  return `${m}:${String(s).padStart(2, '0')}`;
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
  smartCrop,
  onToggle,
  onSelectAll,
  onClearAll,
  onClassify,
  onAnalyze,
  onToggleSection,
  onClip,
  onSaveSrt,
  onSegmentEdit,
  onBurnSubtitlesChange,
  subtitleMode,
  onSubtitleModeChange,
  hasTranslation,
  onAspectRatioChange,
  onSmartCropChange,
  smartCropTransition,
  onSmartCropTransitionChange,
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
  transcribeDuration = 0,
  maxSectionSecs,
  onMaxSectionSecsChange,
  manualClips,
  onAddManualClip,
  onRemoveManualClip,
  verticalTitle,
  verticalTitleFontSize,
  verticalTitleColor,
  onVerticalTitleChange,
  onVerticalTitleFontSizeChange,
  onVerticalTitleColorChange,
}: Props) {
  const { t } = useLang();
  const [searchText, setSearchText] = useState("");
  const [showSrt, setShowSrt] = useState(false);
  const [previewReady, setPreviewReady] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editingText, setEditingText] = useState("");
  const fontStyleRef = useRef<HTMLStyleElement | null>(null);
  const [mcStart, setMcStart] = useState("");
  const [mcEnd, setMcEnd] = useState("");
  const [mcError, setMcError] = useState("");
  const mcEndRef = useRef<HTMLInputElement>(null);
  const editTextareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (editingIndex !== null) {
      editTextareaRef.current?.focus();
    }
  }, [editingIndex]);

  function startEdit(seg: SrtSegment, e: React.MouseEvent) {
    e.stopPropagation();
    setEditingIndex(seg.index);
    setEditingText(seg.text);
  }

  function saveEdit() {
    if (editingIndex !== null) {
      onSegmentEdit(editingIndex, editingText.trim() || editingText);
      setEditingIndex(null);
    }
  }

  function cancelEdit() {
    setEditingIndex(null);
  }

  function renderSegmentText(seg: SrtSegment) {
    if (editingIndex === seg.index) {
      return (
        <textarea
          ref={editTextareaRef}
          className="segment-text-edit"
          value={editingText}
          onChange={e => setEditingText(e.target.value)}
          onBlur={saveEdit}
          onKeyDown={e => {
            e.stopPropagation();
            if (e.key === "Escape") { cancelEdit(); }
            if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); saveEdit(); }
          }}
          onClick={e => e.stopPropagation()}
          rows={2}
        />
      );
    }
    return (
      <p
        className="segment-text"
        onClick={e => e.stopPropagation()}
        onDoubleClick={e => startEdit(seg, e)}
        title={t("segmentEditHint")}
      >
        {seg.text}
      </p>
    );
  }

  // Clear search when sections first appear so the section-grouped view is shown
  useEffect(() => {
    if (sections.length > 0) setSearchText("");
  }, [sections.length]);

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
          <p className="sidebar-label">{t("sidebarFile")}</p>
          <p className="sidebar-value" title={videoPath}>{videoName}</p>
        </div>

        <div className="sidebar-block sidebar-stats-row">
          <div>
            <p className="sidebar-label">{t("sidebarSegments")}</p>
            <p className="sidebar-value">{segments.length}</p>
          </div>
          <div className="sidebar-stats-right">
            <p className="sidebar-label">{t("sidebarSelected")}</p>
            <p className="sidebar-value selected-count">{selectedIndices.size}</p>
          </div>
        </div>

        {transcribeDuration > 0 && (
          <div className="sidebar-block transcribe-duration-block">
            <p className="sidebar-label">{t("sidebarTranscribeTime")}</p>
            <p className="transcribe-duration-value">
              {String(Math.floor(transcribeDuration / 60)).padStart(2, "0")}:{String(transcribeDuration % 60).padStart(2, "0")}
            </p>
          </div>
        )}

        {/* Model AI */}
        <div className="sidebar-block">
          <div className="llm-select-section">
            <div className="llm-select-header">
              <p className="sidebar-label" style={{ margin: 0 }}>
                {t("sidebarAIModel")}
                {selectedLlm && (
                  <span className={`llm-source-badge ${selectedLlm.source}`}>
                    {selectedLlm.source === "local" ? t("badgeLocal") : t("badgeOllama")}
                  </span>
                )}
              </p>
              <button className="llm-manage-btn" onClick={onManageModels}>
                {t("btnManage")}
              </button>
            </div>
            {llmModels.length === 0 ? (
              <button className="llm-empty-btn" onClick={onManageModels}>
                {t("btnDownloadModel")}
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
            <div className="classify-options-row">
              <label className="classify-options-label">{t("labelMaxDuration")}</label>
              <div className="classify-options-input-row">
                <input
                  type="number"
                  className="classify-duration-input"
                  min={20}
                  max={600}
                  step={5}
                  value={maxSectionSecs}
                  onChange={e => onMaxSectionSecsChange(Math.max(20, Number(e.target.value)))}
                  disabled={loading}
                />
                <span className="classify-options-unit">{t("labelSeconds")}</span>
              </div>
            </div>
            <button
              className="btn btn-primary w-full"
              onClick={onClassify}
              disabled={loading || llmModels.length === 0}
            >
              {t("btnClassify")}
            </button>
            {hasSections && (
              <p className="classify-sections-count">{t("classifySectionsFound", { n: sections.length })}</p>
            )}
            <button
              className="btn btn-secondary w-full"
              onClick={onAnalyze}
              disabled={loading || llmModels.length === 0}
            >
              {t("btnAnalyze")}
            </button>
            <div className="sidebar-actions-row">
              <button className="btn btn-secondary" onClick={onSelectAll}>{t("btnSelectAll")}</button>
              <button className="btn btn-ghost" onClick={onClearAll}>{t("btnClearAll")}</button>
            </div>
            <button className="btn btn-ghost w-full" onClick={onSaveSrt}>{t("btnSaveSrt")}</button>
          </div>
        </div>

        {/* Manual Clips */}
        <div className="sidebar-block">
          <p className="sidebar-label">{t("manualClipTitle")}</p>
          <div className="mc-form">
            <div className="mc-form-row">
              <input
                type="text"
                className="mc-ts-input"
                placeholder="0:00"
                value={mcStart}
                onChange={e => { setMcStart(e.target.value); setMcError(""); }}
                onKeyDown={e => { if (e.key === 'Tab' || e.key === 'Enter') { e.preventDefault(); mcEndRef.current?.focus(); } }}
                disabled={loading}
              />
              <span className="mc-arrow">→</span>
              <input
                ref={mcEndRef}
                type="text"
                className="mc-ts-input"
                placeholder="0:20"
                value={mcEnd}
                onChange={e => { setMcEnd(e.target.value); setMcError(""); }}
                onKeyDown={e => {
                  if (e.key === 'Enter') {
                    const s = parseTimestamp(mcStart);
                    const en = parseTimestamp(mcEnd);
                    if (s === null || en === null) { setMcError(t("mcErrorFormat")); return; }
                    if (en <= s) { setMcError(t("mcErrorEnd")); return; }
                    onAddManualClip(s, en);
                    setMcStart(""); setMcEnd(""); setMcError("");
                  }
                }}
                disabled={loading}
              />
              <button
                className="btn btn-secondary mc-add-btn"
                disabled={loading}
                onClick={() => {
                  const s = parseTimestamp(mcStart);
                  const en = parseTimestamp(mcEnd);
                  if (s === null || en === null) { setMcError(t("mcErrorFormat")); return; }
                  if (en <= s) { setMcError(t("mcErrorEnd")); return; }
                  onAddManualClip(s, en);
                  setMcStart(""); setMcEnd(""); setMcError("");
                }}
              >+</button>
            </div>
            {mcError && <p className="mc-error">{mcError}</p>}
          </div>
          {manualClips.length > 0 && (
            <ul className="mc-list">
              {manualClips.map(clip => (
                <li key={clip.id} className="mc-item">
                  <span className="mc-range">{fmtTs(clip.startSec)} → {fmtTs(clip.endSec)}</span>
                  <span className="mc-dur">({Math.round(clip.endSec - clip.startSec)}s)</span>
                  <button className="mc-remove" onClick={() => onRemoveManualClip(clip.id)}>✕</button>
                </li>
              ))}
            </ul>
          )}
          {manualClips.length === 0 && (
            <p className="mc-hint">{t("mcHint")}</p>
          )}
        </div>

        {/* Translate */}
        <div className="sidebar-block">
          <div className="translate-section">
            <p className="sidebar-label">
              {t("sidebarTranslate")}
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
                {t("btnTranslate")}
              </button>
            </div>
          </div>
        </div>

        {/* Aspect Ratio */}
        <div className="sidebar-block">
          <p className="sidebar-label">{t("sidebarAspect")}</p>
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

        {/* Vertical Title — only for 9:16-fit */}
        {aspectRatio === "9:16-fit" && (
          <div className="sidebar-block vertical-title-block">
            <p className="sidebar-label">{t("verticalTitleSection")}</p>
            <p className="sidebar-hint">{t("verticalTitleHint")}</p>
            <input
              className="vertical-title-input"
              type="text"
              placeholder={t("verticalTitlePlaceholder")}
              value={verticalTitle}
              onChange={e => onVerticalTitleChange(e.target.value)}
            />
            {verticalTitle.trim().length > 0 && (
              <>
                <div className="subtitle-setting-row" style={{ marginTop: 8 }}>
                  <span className="sidebar-label">{t("verticalTitleFontSize")}</span>
                  <span className="font-size-badge">{verticalTitleFontSize}px</span>
                </div>
                <div className="font-size-slider-row">
                  <span className="font-size-hint">A</span>
                  <input
                    type="range"
                    className="font-size-slider"
                    min={24} max={96} step={4}
                    value={verticalTitleFontSize}
                    onChange={e => onVerticalTitleFontSizeChange(Number(e.target.value))}
                  />
                  <span className="font-size-hint large">A</span>
                </div>
                <div className="style-colors-row" style={{ marginTop: 6 }}>
                  <label className="style-color-label">
                    <span>{t("verticalTitleColor")}</span>
                    <input
                      type="color"
                      value={verticalTitleColor}
                      onChange={e => onVerticalTitleColorChange(e.target.value)}
                    />
                  </label>
                </div>
              </>
            )}
          </div>
        )}

        {/* Smart Crop — hidden for original and 9:16-fit (no crop happens) */}
        {aspectRatio !== "original" && aspectRatio !== "9:16-fit" && (
          <div className="sidebar-block smart-crop-block">
            <label className="smart-crop-toggle">
              <input
                type="checkbox"
                checked={smartCrop}
                onChange={e => onSmartCropChange(e.target.checked)}
              />
              <span className="smart-crop-label">
                <span className="smart-crop-icon">🎯</span>
                {t("smartCropLabel")}
                <span className="smart-crop-hint">{t("smartCropHint")}</span>
              </span>
            </label>
            {smartCrop && (
              <>
                <div className="smart-crop-transition">
                  {([
                    { value: "smooth",     labelKey: "smartCropSmooth",     hintKey: "smartCropSmoothHint"     },
                    { value: "aggressive", labelKey: "smartCropAggressive", hintKey: "smartCropAggressiveHint" },
                  ] as const).map(opt => (
                    <button
                      key={opt.value}
                      className={`transition-btn ${smartCropTransition === opt.value ? "active" : ""}`}
                      onClick={() => onSmartCropTransitionChange(opt.value)}
                      title={t(opt.hintKey)}
                    >
                      {t(opt.labelKey)}
                    </button>
                  ))}
                </div>
                <p className="smart-crop-note">
                  {smartCropTransition === "aggressive"
                    ? t("smartCropNoteAggressive")
                    : t("smartCropNoteSmooth")
                  }
                </p>
              </>
            )}
          </div>
        )}

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
              {t("subtitleBurn")}
            </span>
          </label>
          {burnSubtitles && hasTranslation && (
            <div className="subtitle-mode-group">
              <span className="sidebar-label">{t("subtitleMode")}</span>
              <div className="subtitle-mode-options">
                {(["translated_only", "bilingual", "original_only"] as const).map(mode => (
                  <label key={mode} className={`subtitle-mode-option${subtitleMode === mode ? " active" : ""}`}>
                    <input
                      type="radio"
                      name="subtitle-mode"
                      value={mode}
                      checked={subtitleMode === mode}
                      onChange={() => onSubtitleModeChange(mode)}
                    />
                    {t(mode === "translated_only" ? "subtitleModeTranslated"
                      : mode === "bilingual" ? "subtitleModeBilingual"
                      : "subtitleModeOriginal")}
                  </label>
                ))}
              </div>
            </div>
          )}
          {burnSubtitles && (
            <div className="subtitle-settings">
              <div className="subtitle-setting-row">
                <span className="sidebar-label">{t("subtitleFontSize")}</span>
                <span className="font-size-badge">
                  {subtitleFontSize === 0 ? t("wordsAuto") : `${subtitleFontSize}px`}
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
                  <button className="font-reset-btn" onClick={() => onSubtitleFontSizeChange(0)}>↺</button>
                )}
              </div>
              <span className="sidebar-label" style={{ marginTop: 8, display: "block" }}>{t("subtitleFont")}</span>
              <select
                className="font-select"
                value={subtitleFont}
                onChange={e => onSubtitleFontChange(e.target.value)}
              >
                <option value="">{t("subtitleFontDefault")}</option>
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
                {subtitleStyle.allCaps ? t("subtitlePreviewTextCaps") : t("subtitlePreviewText")}
                {!previewReady && subtitleFont && (
                  <span className="font-preview-loading">{t("subtitleFontLoading")}</span>
                )}
              </div>

              {/* ── Style controls ── */}
              <div className="subtitle-style-section">
                <div className="style-colors-row">
                  <label className="style-color-label">
                    <span>{t("subtitleColorText")}</span>
                    <input
                      type="color"
                      value={subtitleStyle.textColor}
                      onChange={e => onSubtitleStyleChange({ ...subtitleStyle, textColor: e.target.value })}
                    />
                  </label>
                  <label className="style-color-label">
                    <span>{t("subtitleColorOutline")}</span>
                    <input
                      type="color"
                      value={subtitleStyle.outlineColor}
                      onChange={e => onSubtitleStyleChange({ ...subtitleStyle, outlineColor: e.target.value })}
                    />
                  </label>
                </div>

                <div className="style-row">
                  <span className="sidebar-label" style={{ flex: 1 }}>{t("subtitleOutlineWidth")}</span>
                  <span className="font-size-badge">{subtitleStyle.outlineWidth}px</span>
                </div>
                <input
                  type="range"
                  className="font-size-slider"
                  min={0} max={8} step={1}
                  value={subtitleStyle.outlineWidth}
                  onChange={e => onSubtitleStyleChange({ ...subtitleStyle, outlineWidth: Number(e.target.value) })}
                />

                <span className="sidebar-label" style={{ display: "block", marginTop: 6 }}>{t("subtitlePosition")}</span>
                <div className="style-position-row">
                  {(["top", "center", "bottom"] as const).map(pos => (
                    <button
                      key={pos}
                      className={`style-pos-btn ${subtitleStyle.position === pos ? "active" : ""}`}
                      onClick={() => onSubtitleStyleChange({ ...subtitleStyle, position: pos })}
                    >
                      {pos === "top" ? t("subtitlePosTop") : pos === "center" ? t("subtitlePosCenter") : t("subtitlePosBottom")}
                    </button>
                  ))}
                </div>

                <label className="style-box-toggle">
                  <input
                    type="checkbox"
                    checked={subtitleStyle.allCaps}
                    onChange={e => onSubtitleStyleChange({ ...subtitleStyle, allCaps: e.target.checked })}
                  />
                  <span>{t("subtitleAllCaps")}</span>
                </label>

                <label className="style-box-toggle">
                  <input
                    type="checkbox"
                    checked={subtitleStyle.boxEnabled}
                    onChange={e => onSubtitleStyleChange({ ...subtitleStyle, boxEnabled: e.target.checked })}
                  />
                  <span>{t("subtitleBgBox")}</span>
                </label>
                {subtitleStyle.boxEnabled && (
                  <div className="style-box-settings">
                    <label className="style-color-label">
                      <span>{t("subtitleBgColor")}</span>
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
            disabled={selectedIndices.size === 0 && manualClips.length === 0 || loading}
          >
            <span>{t("btnClip")}</span>
            {(selectedIndices.size > 0 || manualClips.length > 0) && (
              <span className="btn-clip-count">
                {[
                  selectedIndices.size > 0 ? t("clipCountSegments", { n: selectedIndices.size }) : "",
                  manualClips.length > 0 ? t("clipCountManual", { n: manualClips.length }) : "",
                ].filter(Boolean).join(" + ")}
              </span>
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
            placeholder={t("searchPlaceholder")}
            value={searchText}
            onChange={e => setSearchText(e.target.value)}
          />
          <button className="btn btn-ghost btn-sm" onClick={() => setShowSrt(!showSrt)}>
            {showSrt ? t("btnCardView") : t("btnSrtView")}
          </button>
        </div>

        <div className="segments-list">
          {/* SRT view */}
          {showSrt ? (
            <div className="srt-view">
              <pre className="srt-content">
                {segments.map(seg =>
                  `${seg.index}\n${seg.start_time} --> ${seg.end_time}\n${seg.text}\n`
                ).join("\n")}
              </pre>
            </div>
          ) : hasSections && !searchText ? (
            sections.map((section, sIdx) => {
              const status = sectionStatus(section, segments, selectedIndices);
              const segs = segmentsOf(section);
              return (
                <div key={sIdx} className={`section-group ${status === "all" ? "section-selected" : status === "partial" ? "section-partial" : ""}`}>
                  <div
                    className="section-header"
                    role="button"
                    tabIndex={0}
                    onClick={() => onToggleSection(sIdx)}
                    onKeyDown={e => (e.key === "Enter" || e.key === " ") && onToggleSection(sIdx)}
                  >
                    <div className="section-check">
                      {status === "all" ? "✓" : status === "partial" ? "◑" : "○"}
                    </div>
                    <div className="section-header-body">
                      <div className="section-header-top">
                        <span className="section-num">{sIdx + 1}</span>
                        <span className="section-name">{section.name}</span>
                        <span className="section-time">{sectionTimeRange(section)}</span>
                        <span className="section-count">{t("sectionSegments", { n: segs.length })}</span>
                      </div>
                      <div className="section-summary">{section.summary}</div>
                    </div>
                  </div>

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
                        {renderSegmentText(seg)}
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
                  {renderSegmentText(seg)}
                </div>
              ))}

              {flatFiltered.length === 0 && (
                <div className="empty-state">
                  <p>{t("noResults")}</p>
                </div>
              )}

              {!hasSections && segments.length > 0 && !searchText && (
                <div className="classify-hint">
                  <p dangerouslySetInnerHTML={{ __html: t("classifyHint") }} />
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
