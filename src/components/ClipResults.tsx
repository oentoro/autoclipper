import type { ClipResult } from "../types";

interface Props {
  results: ClipResult[];
  loading: boolean;
  onBack: () => void;
}

export default function ClipResults({ results, loading, onBack }: Props) {
  const success = results.filter((r) => r.success);
  const failed = results.filter((r) => !r.success);

  async function openFolder(path: string) {
    const dir = path.substring(0, path.lastIndexOf("/"));
    const { Command } = await import("@tauri-apps/plugin-shell");
    Command.create("open", [dir]).execute();
  }

  if (loading) {
    return (
      <div className="results-container">
        <div className="results-loading">
          <div className="spinner large" />
          <p>Sedang memproses clip dengan FFmpeg...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="results-container">
      <div className="results-header">
        <h2>Hasil Clipping</h2>
        <div className="results-summary">
          <span className="badge badge-success">{success.length} berhasil</span>
          {failed.length > 0 && (
            <span className="badge badge-error">{failed.length} gagal</span>
          )}
        </div>
      </div>

      <div className="results-list">
        {results.map((r, i) => (
          <div key={i} className={`result-card ${r.success ? "success" : "error"}`}>
            <div className="result-icon">{r.success ? "✓" : "✕"}</div>
            <div className="result-info">
              <p className="result-name">{r.output_path.split("/").pop()}</p>
              <p className="result-path">{r.output_path}</p>
              <p className="result-msg">{r.message}</p>
            </div>
            {r.success && (
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => openFolder(r.output_path)}
              >
                Buka Folder
              </button>
            )}
          </div>
        ))}
      </div>

      <div className="results-actions">
        <button className="btn btn-secondary" onClick={onBack}>
          ← Kembali ke Transkrip
        </button>
      </div>
    </div>
  );
}
