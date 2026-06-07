import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
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
    ? "Menggabungkan audio & video..."
    : youtubeProgress?.phase === "done"
    ? "Selesai!"
    : youtubeProgress
    ? `${youtubeProgress.percent.toFixed(1)}%${youtubeProgress.speed ? `  ·  ${youtubeProgress.speed}` : ""}${youtubeProgress.eta && youtubeProgress.eta !== "Unknown" ? `  ·  ETA ${youtubeProgress.eta}` : ""}`
    : "Memulai download...";

  return (
    <div className="upload-container">
      {/* Source tab switcher */}
      <div className="upload-tabs">
        <button
          className={`upload-tab ${tab === "local" ? "active" : ""}`}
          onClick={() => setTab("local")}
          disabled={disabled || youtubeDownloading}
        >
          📁 File Lokal
        </button>
        <button
          className={`upload-tab ${tab === "youtube" ? "active" : ""}`}
          onClick={() => setTab("youtube")}
          disabled={disabled}
        >
          ▶ YouTube
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
              <p className="upload-title">Memproses video...</p>
              <p className="upload-sub">Sedang mentranskripsi audio</p>
            </>
          ) : (
            <>
              <p className="upload-title">Drag & drop video atau klik untuk browse</p>
              <p className="upload-sub">Format: MP4, MKV, AVI, MOV, WebM, FLV</p>
              <button className="btn btn-primary upload-btn">Pilih Video</button>
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
              <button className="btn btn-cancel" onClick={onCancelYoutube}>✕ Batalkan</button>
            </div>
          ) : (
            <>
              <p className="yt-hint">Paste link video YouTube</p>
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
                  ⬇ Download
                </button>
              </div>
              <p className="yt-req">Video disimpan ke folder <code>Downloads</code> · Diperlukan: <code>yt-dlp</code> (<code>pip install yt-dlp</code>)</p>
            </>
          )}
        </div>
      )}

      <div className="feature-grid">
        <div className="feature-card">
          <span className="feature-icon">🎙</span>
          <h3>Transkripsi Otomatis</h3>
          <p>Ubah audio menjadi teks secara akurat</p>
          <span className="feature-badge">Whisper</span>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🤖</span>
          <h3>Analisis AI</h3>
          <p>Pilih segmen terpenting dari konten secara otomatis</p>
          <span className="feature-badge">Gemma3</span>
        </div>
        <div className="feature-card">
          <span className="feature-icon">✂</span>
          <h3>Penggabungan Video</h3>
          <p>Gabungkan segmen pilihan menjadi satu video</p>
          <span className="feature-badge">FFmpeg</span>
        </div>
      </div>
    </div>
  );
}
