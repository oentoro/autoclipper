import { invoke } from "@tauri-apps/api/core";
import { useLang } from "../i18n";
import type { ClipResult } from "../types";

interface Props {
  result: ClipResult | null;
  loading: boolean;
  onBack: () => void;
}

export default function ClipResults({ result, loading, onBack }: Props) {
  const { t } = useLang();

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

      <div className="results-actions">
        <button className="btn btn-secondary" onClick={onBack}>
          {t("btnBackTranscript")}
        </button>
      </div>
    </div>
  );
}
