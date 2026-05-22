import type { ClipResult } from "../types";

interface Props {
  result: ClipResult | null;
  loading: boolean;
  onBack: () => void;
}

export default function ClipResults({ result, loading, onBack }: Props) {
  async function openInFinder(path: string) {
    const { Command } = await import("@tauri-apps/plugin-shell");
    Command.create("open", ["-R", path]).execute();
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
        <p className="loading-text">Menggabungkan segmen dengan FFmpeg...</p>
        <p className="loading-sub">Ini mungkin memakan beberapa saat tergantung panjang video</p>
      </div>
    );
  }

  if (!result) {
    return (
      <div className="results-container centered">
        <p>Belum ada hasil</p>
        <button className="btn btn-secondary" onClick={onBack}>← Kembali</button>
      </div>
    );
  }

  const filename = result.output_path.split("/").pop() ?? result.output_path;

  return (
    <div className="results-container">
      <div className="result-hero">
        <div className="result-hero-icon">🎬</div>
        <h2 className="result-hero-title">Video Berhasil Dibuat!</h2>
        <p className="result-hero-sub">{result.message}</p>
      </div>

      <div className="result-stats">
        <div className="stat-card">
          <span className="stat-value">{result.total_segments}</span>
          <span className="stat-label">Segmen digabung</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{formatDuration(result.duration_secs)}</span>
          <span className="stat-label">Durasi total</span>
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
          Buka di Finder
        </button>
      </div>

      <div className="results-actions">
        <button className="btn btn-secondary" onClick={onBack}>
          ← Kembali ke Transkrip
        </button>
      </div>
    </div>
  );
}
