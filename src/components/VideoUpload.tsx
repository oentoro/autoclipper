import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useLang } from "../i18n";
import type { YtDownloadProgress } from "../types";

interface Props {
  onSelect: (path: string) => void;
  disabled: boolean;
  onYoutubeDownload?: (url: string) => void;
  onCancelYoutube?: () => void;
  youtubeDownloading?: boolean;
  youtubeProgress?: YtDownloadProgress | null;
}

export default function VideoUpload({
  onSelect,
  disabled,
  onYoutubeDownload,
  onCancelYoutube,
  youtubeDownloading = false,
  youtubeProgress = null,
}: Props) {
  const { t } = useLang();
  const [dragOver, setDragOver] = useState(false);
  const [tab, setTab] = useState<"local" | "youtube">("local");
  const [ytUrl, setYtUrl] = useState("");

  async function handleBrowse() {
    if (disabled) return;
    const path = await open({
      filters: [{ name: "Video", extensions: ["mp4", "mkv", "avi", "mov", "webm", "flv", "m4v"] }],
      title: "Pilih file video",
    });
    if (path) onSelect(path as string);
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    setDragOver(false);
    if (disabled || tab !== "local") return;
    const file = e.dataTransfer.files[0];
    if (file) onSelect((file as File & { path?: string }).path ?? file.name);
  }

  function handleYtDownload() {
    const trimmed = ytUrl.trim();
    if (!trimmed || !onYoutubeDownload) return;
    onYoutubeDownload(trimmed);
  }

  const phaseLabel = youtubeProgress?.phase === "merging"
    ? t("ytMerging")
    : youtubeProgress?.phase === "done"
    ? t("ytDone")
    : youtubeProgress
    ? `${youtubeProgress.percent.toFixed(1)}%${youtubeProgress.speed ? `  ·  ${youtubeProgress.speed}` : ""}${youtubeProgress.eta && youtubeProgress.eta !== "Unknown" ? `  ·  ETA ${youtubeProgress.eta}` : ""}`
    : t("ytStarting");

  return (
    <div className="upload-container">
      <div className="upload-tabs">
        <button
          className={`upload-tab ${tab === "local" ? "active" : ""}`}
          onClick={() => setTab("local")}
          disabled={disabled || youtubeDownloading}
        >
          {t("tabLocal")}
        </button>
        <button
          className={`upload-tab ${tab === "youtube" ? "active" : ""}`}
          onClick={() => setTab("youtube")}
          disabled={disabled}
        >
          {t("tabYoutube")}
        </button>
      </div>

      {tab === "local" ? (
        <div
          className={`upload-zone ${dragOver ? "drag-over" : ""} ${disabled ? "disabled" : ""}`}
          onClick={handleBrowse}
          onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
          onDragLeave={() => setDragOver(false)}
          onDrop={handleDrop}
        >
          <div className="upload-icon">🎬</div>
          {disabled ? (
            <>
              <p className="upload-title">{t("uploadProcessing")}</p>
              <p className="upload-sub">{t("uploadTranscribing")}</p>
            </>
          ) : (
            <>
              <p className="upload-title">{t("uploadTitle")}</p>
              <p className="upload-sub">{t("uploadSub")}</p>
              <button className="btn btn-primary upload-btn">{t("btnSelectVideo")}</button>
            </>
          )}
        </div>
      ) : (
        <div className="yt-zone">
          <div className="yt-icon">▶</div>
          {youtubeDownloading ? (
            <div className="yt-progress-wrap">
              <p className="yt-progress-label">{phaseLabel}</p>
              <div className="yt-progress-bar-track">
                <div
                  className={`yt-progress-bar-fill ${youtubeProgress?.phase === "merging" ? "indeterminate" : ""}`}
                  style={{ width: `${youtubeProgress?.phase === "merging" ? 100 : (youtubeProgress?.percent ?? 0)}%` }}
                />
              </div>
              <button className="btn btn-cancel" onClick={onCancelYoutube}>{t("btnCancel")}</button>
            </div>
          ) : (
            <>
              <p className="yt-hint">{t("ytHint")}</p>
              <div className="yt-input-row">
                <input
                  className="yt-input"
                  type="url"
                  placeholder="https://www.youtube.com/watch?v=..."
                  value={ytUrl}
                  onChange={e => setYtUrl(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter") handleYtDownload(); }}
                  disabled={disabled}
                />
                <button
                  className="btn btn-primary"
                  onClick={handleYtDownload}
                  disabled={disabled || !ytUrl.trim()}
                >
                  {t("btnYtDownload")}
                </button>
              </div>
              <p className="yt-req" dangerouslySetInnerHTML={{ __html: t("ytReq") }} />
            </>
          )}
        </div>
      )}

      <div className="feature-grid">
        <div className="feature-card">
          <span className="feature-icon">🎙</span>
          <h3>{t("feat1Title")}</h3>
          <p>{t("feat1Desc")}</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🤖</span>
          <h3>{t("feat2Title")}</h3>
          <p>{t("feat2Desc")}</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">✂</span>
          <h3>{t("feat3Title")}</h3>
          <p>{t("feat3Desc")}</p>
        </div>
      </div>
    </div>
  );
}
