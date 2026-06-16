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
  ytdlpOk?: boolean;
  ytdlpInstalling?: boolean;
  onInstallYtdlp?: () => void;
}

export default function VideoUpload({
  onSelect,
  disabled,
  onYoutubeDownload,
  onCancelYoutube,
  youtubeDownloading = false,
  youtubeProgress = null,
  ytdlpOk = true,
  ytdlpInstalling = false,
  onInstallYtdlp,
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

  const isMerging    = youtubeProgress?.phase === "merging";
  const isDone       = youtubeProgress?.phase === "done";
  const dlPct        = youtubeProgress?.percent ?? 0;
  const dlSpeed      = youtubeProgress?.speed ?? "";
  const dlDownloaded = youtubeProgress?.downloaded ?? "";
  const dlTotal      = youtubeProgress?.total ?? "";
  const dlEta        = youtubeProgress?.eta && youtubeProgress.eta !== "Unknown" ? youtubeProgress.eta : "";

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
          {!ytdlpOk ? (
            <div className="ytdlp-missing">
              <p className="ytdlp-missing-title">{t("ytdlpMissing")}</p>
              <p className="ytdlp-missing-desc">{t("ytdlpMissingDesc")}</p>
              <button
                className="btn btn-primary"
                onClick={onInstallYtdlp}
                disabled={ytdlpInstalling}
              >
                {ytdlpInstalling ? t("ytdlpInstalling") : t("ytdlpInstall")}
              </button>
            </div>
          ) : youtubeDownloading ? (
            <div className="yt-progress-wrap">
              <div className="yt-progress-pct">
                {isDone ? t("ytDone") : isMerging ? t("ytMerging") : youtubeProgress
                  ? (dlSpeed
                      ? <><span className="yt-speed-main">{dlSpeed}</span>{dlDownloaded && dlTotal && <span className="yt-size-main"> · {dlDownloaded} / {dlTotal}</span>}</>
                      : t("ytStarting"))
                  : t("ytStarting")}
              </div>
              <div className="yt-progress-bar-track">
                <div
                  className={`yt-progress-bar-fill ${isMerging ? "indeterminate" : ""}`}
                  style={{ width: `${isMerging || isDone ? 100 : dlPct}%` }}
                />
              </div>
              {!isMerging && !isDone && youtubeProgress && dlEta && (
                <p className="yt-progress-meta">
                  <span>ETA {dlEta}</span>
                  <span className="yt-meta-sep">·</span>
                  <span>{dlPct.toFixed(1)}%</span>
                </p>
              )}
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
