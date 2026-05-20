import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

interface Props {
  onSelect: (path: string) => void;
  disabled: boolean;
}

export default function VideoUpload({ onSelect, disabled }: Props) {
  const [dragOver, setDragOver] = useState(false);

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
    if (disabled) return;
    const file = e.dataTransfer.files[0];
    if (file) onSelect((file as File & { path?: string }).path ?? file.name);
  }

  return (
    <div className="upload-container">
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
            <p className="upload-sub">Whisper sedang mentranskripsi audio</p>
          </>
        ) : (
          <>
            <p className="upload-title">Drag & drop video atau klik untuk browse</p>
            <p className="upload-sub">Format: MP4, MKV, AVI, MOV, WebM, FLV</p>
            <button className="btn btn-primary upload-btn">
              Pilih Video
            </button>
          </>
        )}
      </div>

      <div className="feature-grid">
        <div className="feature-card">
          <span className="feature-icon">🎙</span>
          <h3>Whisper AI</h3>
          <p>Speech recognition akurat untuk bahasa Indonesia</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🤖</span>
          <h3>Gemma4 Analysis</h3>
          <p>AI otomatis memilih segmen terpenting dari konten</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">✂</span>
          <h3>FFmpeg Clipping</h3>
          <p>Clip video berkualitas tinggi tanpa re-encode</p>
        </div>
      </div>
    </div>
  );
}
