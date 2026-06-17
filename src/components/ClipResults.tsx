import { invoke } from "@tauri-apps/api/core";
import { useState, useEffect, useRef } from "react";
import { useLang } from "../i18n";
import type { ClipResult, CaptionResult, SrtSegment } from "../types";

interface Props {
  result: ClipResult | null;
  loading: boolean;
  onBack: () => void;
  selectedSegments: SrtSegment[];
  detectedLanguage: string;
  modelPath: string;
  ollamaModel: string;
}

// Hoisted outside component to avoid re-evaluation on every render
type CaptionState = "idle" | "loading" | "done" | "error";

export default function ClipResults({
  result,
  loading,
  onBack,
  selectedSegments,
  detectedLanguage,
  modelPath,
  ollamaModel,
}: Props) {
  const { t } = useLang();

  const [captionState, setCaptionState] = useState<CaptionState>("idle");
  const [caption, setCaption] = useState<CaptionResult | null>(null);
  const [captionTab, setCaptionTab] = useState<"tiktok" | "instagram">("tiktok");
  const [copied, setCopied] = useState(false);
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!result || loading || selectedSegments.length === 0) return;
    generateCaption();
  }, [result]);

  // Clean up any pending "copied" timer on unmount
  useEffect(() => {
    return () => {
      if (copiedTimerRef.current !== null) {
        clearTimeout(copiedTimerRef.current);
      }
    };
  }, []);

  async function generateCaption() {
    setCaptionState("loading");
    setCopied(false);
    if (copiedTimerRef.current !== null) {
      clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = null;
    }
    try {
      const res = await invoke<CaptionResult>("generate_caption", {
        segments: selectedSegments,
        language: detectedLanguage,
        modelPath,
        ollamaModel,
      });
      setCaption(res);
      setCaptionState("done");
    } catch {
      setCaptionState("error");
    }
  }

  async function copyCaption() {
    if (!caption) return;
    const hashtags = captionTab === "tiktok"
      ? caption.hashtags_short
      : caption.hashtags_long;
    const text = captionTab === "tiktok"
      ? caption.caption_short
      : caption.caption_long;
    try {
      await navigator.clipboard.writeText(`${text}\n\n${hashtags.join(" ")}`);
      setCopied(true);
      if (copiedTimerRef.current !== null) clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = setTimeout(() => {
        setCopied(false);
        copiedTimerRef.current = null;
      }, 2000);
    } catch {
      // clipboard write failed — do not flip copied state
    }
  }

  function openInFinder(path: string) {
    invoke("reveal_in_file_manager", { path });
  }

  function formatDuration(secs: number) {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  }

  if (loading) {
    return (
      <div className="results-container centered">
        <div className="spinner large" />
        <p className="loading-text">{t("clippingLoading")}</p>
        <p className="loading-sub">{t("clippingLoadingSub")}</p>
      </div>
    );
  }

  if (!result) {
    return (
      <div className="results-container centered">
        <p>{t("noResults2")}</p>
        <button className="btn btn-secondary" onClick={onBack}>{t("btnBack")}</button>
      </div>
    );
  }

  const filename = result.output_path.split("/").pop() ?? result.output_path;

  return (
    <div className="results-container">
      <div className="result-hero">
        <div className="result-hero-icon">🎬</div>
        <h2 className="result-hero-title">{t("resultTitle")}</h2>
        <p className="result-hero-sub">{result.message}</p>
      </div>

      <div className="result-stats">
        <div className="stat-card">
          <span className="stat-value">{result.total_segments}</span>
          <span className="stat-label">{t("statSegments")}</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{formatDuration(result.duration_secs)}</span>
          <span className="stat-label">{t("statDuration")}</span>
        </div>
      </div>

      <div className="result-file-card">
        <div className="result-file-icon">📄</div>
        <div className="result-file-info">
          <p className="result-file-name">{filename}</p>
          <p className="result-file-path">{result.output_path}</p>
        </div>
        <button
          className="btn btn-primary"
          onClick={() => openInFinder(result.output_path)}
        >
          {t("btnOpenFinder")}
        </button>
      </div>

      {selectedSegments.length > 0 && (
        <div className="caption-panel">
          <div className="caption-header">
            <span className="caption-title">{t("captionTitle")}</span>
            <button
              className="btn btn-sm btn-secondary"
              onClick={generateCaption}
              disabled={captionState === "loading"}
            >
              {t("captionBtnRegenerate")}
            </button>
          </div>

          {captionState === "loading" && (
            <div className="caption-loading">
              <div className="spinner small" />
              <span>{t("captionGenerating")}</span>
            </div>
          )}

          {captionState === "error" && (
            <div className="caption-error">
              <span>{t("captionError")}</span>
              <button className="btn btn-sm btn-secondary" onClick={generateCaption}>
                {t("captionBtnRetry")}
              </button>
            </div>
          )}

          {captionState === "done" && caption && (
            <>
              <div className="caption-tabs">
                <button
                  className={`caption-tab ${captionTab === "tiktok" ? "active" : ""}`}
                  onClick={() => setCaptionTab("tiktok")}
                >
                  {t("captionTabTiktok")}
                </button>
                <button
                  className={`caption-tab ${captionTab === "instagram" ? "active" : ""}`}
                  onClick={() => setCaptionTab("instagram")}
                >
                  {t("captionTabInstagram")}
                </button>
              </div>

              <div className="caption-body">
                <p className="caption-text">
                  {captionTab === "tiktok" ? caption.caption_short : caption.caption_long}
                </p>
                <p className="caption-hashtags">
                  {(captionTab === "tiktok" ? caption.hashtags_short : caption.hashtags_long).join(" ")}
                </p>
              </div>

              <div className="caption-footer">
                <button className="btn btn-primary" onClick={copyCaption}>
                  {copied ? t("captionBtnCopied") : t("captionBtnCopy")}
                </button>
              </div>
            </>
          )}
        </div>
      )}

      <div className="results-actions">
        <button className="btn btn-secondary" onClick={onBack}>
          {t("btnBackTranscript")}
        </button>
      </div>
    </div>
  );
}
