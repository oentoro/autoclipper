import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { LlmModel, DownloadProgress, WhisperModelInfo, WhisperDownloadProgress } from "../types";

interface RecommendedModel {
  id: string;
  name: string;
  description: string;
  tag?: string;
  size_gb: number;
  filename: string;
  url: string;
  requires_token: boolean;
}

const RECOMMENDED: RecommendedModel[] = [
  {
    id: "qwen3b",
    name: "Qwen 2.5 3B",
    description: "Cepat, gratis, tidak perlu akun — cocok untuk kebanyakan video",
    tag: "Direkomendasikan",
    size_gb: 1.9,
    filename: "Qwen2.5-3B-Instruct-Q4_K_M.gguf",
    url: "https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf",
    requires_token: false,
  },
  {
    id: "qwen7b",
    name: "Qwen 2.5 7B",
    description: "Lebih akurat, gratis — butuh ~5 GB RAM ekstra",
    size_gb: 4.7,
    filename: "Qwen2.5-7B-Instruct-Q4_K_M.gguf",
    url: "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf",
    requires_token: false,
  },
  {
    id: "gemma4-4b",
    name: "Gemma 4 4B (Google)",
    description: "Model Google terbaru — gratis, tidak perlu akun",
    size_gb: 5.0,
    filename: "gemma-4-E4B-it-Q4_K_M.gguf",
    url: "https://huggingface.co/ggml-org/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf",
    requires_token: false,
  },
  {
    id: "gemma4-2b",
    name: "Gemma 4 2B (Google)",
    description: "Model Google ringan — gratis, tidak perlu akun",
    size_gb: 4.6,
    filename: "gemma-4-E2B-it-Q8_0.gguf",
    url: "https://huggingface.co/ggml-org/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q8_0.gguf",
    requires_token: false,
  },
];

interface Props {
  llmModels: LlmModel[];
  selectedLlm: LlmModel | null;
  downloads: Record<string, DownloadProgress>;
  onLlmChange: (m: LlmModel) => void;
  onClose: () => void;
  onRefresh: () => void;
}

function fmtSize(mb: number) {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
}

function fmtGb(bytes: number) {
  return (bytes / 1073741824).toFixed(2);
}

// ── Whisper tab ────────────────────────────────────────────────────────────────

function WhisperTab() {
  const [models, setModels] = useState<WhisperModelInfo[]>([]);
  const [dlProgress, setDlProgress] = useState<Record<string, WhisperDownloadProgress>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [starting, setStarting] = useState<Set<string>>(new Set());

  async function refresh() {
    try {
      const list = await invoke<WhisperModelInfo[]>("list_whisper_models");
      setModels(list);
    } catch { /* ignore */ }
  }

  useEffect(() => {
    refresh();
    const unlistenPromise = listen<WhisperDownloadProgress>("whisper-download-progress", event => {
      const p = event.payload;
      setDlProgress(prev => ({ ...prev, [p.model_key]: p }));
      setStarting(prev => { const n = new Set(prev); n.delete(p.model_key); return n; });
      if (p.done && !p.error) {
        setTimeout(refresh, 300);
        setDlProgress(prev => {
          const n = { ...prev };
          delete n[p.model_key];
          return n;
        });
      }
      if (p.error) {
        setErrors(prev => ({ ...prev, [p.model_key]: p.error! }));
        setDlProgress(prev => {
          const n = { ...prev };
          delete n[p.model_key];
          return n;
        });
      }
    });
    return () => { unlistenPromise.then(fn => fn()); };
  }, []);

  async function handleDownload(m: WhisperModelInfo) {
    const key = `${m.id}-${m.backend}`;
    setErrors(prev => { const n = { ...prev }; delete n[key]; return n; });
    setStarting(prev => { const n = new Set(prev); n.add(key); return n; });
    invoke("download_whisper_model", { modelId: m.id, backend: m.backend })
      .catch(e => {
        const msg = String(e);
        if (!msg.includes("dibatalkan")) {
          setErrors(prev => ({ ...prev, [key]: msg }));
        }
        setDlProgress(prev => { const n = { ...prev }; delete n[key]; return n; });
      })
      .finally(() => {
        setStarting(prev => { const n = new Set(prev); n.delete(key); return n; });
      });
  }

  async function handleCancel(m: WhisperModelInfo) {
    const key = `${m.id}-${m.backend}`;
    await invoke("cancel_whisper_download", { modelKey: key });
    setDlProgress(prev => { const n = { ...prev }; delete n[key]; return n; });
  }

  async function handleDelete(m: WhisperModelInfo) {
    if (!m.cache_path) return;
    try {
      await invoke("delete_whisper_model", { cachePath: m.cache_path });
      await refresh();
    } catch { /* ignore */ }
  }

  const backendLabel = models[0]?.backend === "mlx" ? "MLX (Apple Silicon GPU)" : "CPU (faster-whisper)";

  return (
    <div className="mm-whisper-tab">
      <p className="mm-whisper-desc">
        Model Whisper digunakan untuk transkripsi suara. Unduh model sebelum transkripsi
        agar prosesnya tidak terhambat oleh download. Backend aktif: <strong>{backendLabel}</strong>.
      </p>
      <div className="mm-model-list">
        {models.map(m => {
          const key = `${m.id}-${m.backend}`;
          const dl = dlProgress[key];
          const isActive = !!dl;
          const isStarting = starting.has(key);
          const err = errors[key];

          return (
            <div key={key} className={`mm-model-card ${m.cached ? "downloaded" : ""}`}>
              <div className="mm-model-header">
                <div className="mm-model-name-row">
                  <span className="mm-model-name">{m.name}</span>
                  <span className="mm-tag whisper-preset">Preset: {m.preset_label}</span>
                  {m.cached && <span className="mm-tag active">✓ Tersedia</span>}
                </div>
                <span className="mm-model-size">~{fmtSize(m.size_mb)}</span>
              </div>
              <p className="mm-model-desc">{m.description}</p>

              {err && <p className="mm-error">{err}</p>}

              {isActive && dl ? (
                <div className="mm-progress">
                  <div className="mm-progress-bar">
                    <div className="mm-progress-fill" style={{ width: `${dl.percent}%` }} />
                  </div>
                  <div className="mm-progress-info">
                    <span className="mm-progress-pct">{dl.percent}%</span>
                    <button className="mm-btn-cancel" onClick={() => handleCancel(m)}>
                      ✕ Batalkan
                    </button>
                  </div>
                </div>
              ) : isStarting ? (
                <button className="mm-btn mm-btn-download" disabled style={{ opacity: 0.6, cursor: "default" }}>
                  ⏳ Memulai download...
                </button>
              ) : m.cached ? (
                <div className="mm-actions">
                  <button className="mm-btn mm-btn-delete" onClick={() => handleDelete(m)}>
                    🗑 Hapus
                  </button>
                </div>
              ) : (
                <button className="mm-btn mm-btn-download" onClick={() => handleDownload(m)}>
                  ⬇ Unduh ~{fmtSize(m.size_mb)}
                </button>
              )}
            </div>
          );
        })}
      </div>
      <p className="mm-footer-note">
        Model tersimpan di cache HuggingFace lokal. Tidak diperlukan koneksi internet setelah diunduh.
      </p>
    </div>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────

export default function ModelManager({
  llmModels,
  selectedLlm,
  downloads,
  onLlmChange,
  onClose,
  onRefresh,
}: Props) {
  const [activeTab, setActiveTab] = useState<"llm" | "whisper">("whisper");
  const [hfToken, setHfToken] = useState("");
  const [showToken, setShowToken] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [partials, setPartials] = useState<Record<string, number>>({});

  async function refreshPartials() {
    try {
      const p = await invoke<Record<string, number>>("get_partial_downloads");
      setPartials(p);
    } catch { /* ignore */ }
  }

  useEffect(() => { refreshPartials(); }, []);

  function getLocalModel(filename: string) {
    return llmModels.find(m => m.source === "local" && m.path.includes(filename));
  }

  function isDownloaded(filename: string) {
    return !!getLocalModel(filename);
  }

  async function handleDownload(rec: RecommendedModel) {
    setErrors(prev => { const n = { ...prev }; delete n[rec.filename]; return n; });
    invoke("download_llm_model", {
      url: rec.url,
      filename: rec.filename,
      hfToken: rec.requires_token ? hfToken : "",
    }).then(() => {
      refreshPartials();
      onRefresh();
    }).catch((e: unknown) => {
      setErrors(prev => ({ ...prev, [rec.filename]: String(e) }));
      refreshPartials();
    });
  }

  async function handleCancel(filename: string) {
    await invoke("cancel_llm_download", { filename });
    setTimeout(refreshPartials, 400);
  }

  async function handleDiscard(filename: string) {
    try {
      await invoke("discard_partial_download", { filename });
      setPartials(prev => { const n = { ...prev }; delete n[filename]; return n; });
    } catch { /* ignore */ }
  }

  async function handleDelete(model: LlmModel) {
    try {
      await invoke("delete_llm_model", { path: model.path });
      onRefresh();
    } catch { /* ignore */ }
  }

  const extraLocalModels = llmModels.filter(
    m => m.source === "local" && !RECOMMENDED.some(r => m.path.includes(r.filename))
  );

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-box model-manager" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">Kelola Model AI</h2>
          <button className="modal-close" onClick={onClose}>✕</button>
        </div>

        {/* Tabs */}
        <div className="mm-tabs">
          <button
            className={`mm-tab ${activeTab === "whisper" ? "active" : ""}`}
            onClick={() => setActiveTab("whisper")}
          >
            🎙 Whisper (Transkripsi)
          </button>
          <button
            className={`mm-tab ${activeTab === "llm" ? "active" : ""}`}
            onClick={() => setActiveTab("llm")}
          >
            🤖 LLM (Klasifikasi)
          </button>
        </div>

        {activeTab === "whisper" ? (
          <div className="mm-scroll">
            <WhisperTab />
          </div>
        ) : (
          <div className="mm-scroll">
            {/* HuggingFace token section */}
            {RECOMMENDED.some(r => r.requires_token) && (
              <div className="mm-token-section">
                <button className="mm-token-toggle" onClick={() => setShowToken(!showToken)}>
                  🔑 Token HuggingFace {showToken ? "▲" : "▼"}
                </button>
                {showToken && (
                  <div className="mm-token-body">
                    <p className="mm-token-hint">
                      Diperlukan untuk beberapa model. Cara mendapatkan token gratis:{" "}
                      <span className="mm-token-step">1.</span> Daftar di huggingface.co →{" "}
                      <span className="mm-token-step">2.</span> Settings → Access Tokens → New token →{" "}
                      <span className="mm-token-step">3.</span> Setujui lisensi model di halaman modelnya.
                    </p>
                    <input
                      type="password"
                      className="mm-token-input"
                      placeholder="hf_xxxxxxxxxxxxxxxxxxxxxx"
                      value={hfToken}
                      onChange={e => setHfToken(e.target.value)}
                      autoComplete="off"
                    />
                  </div>
                )}
              </div>
            )}

            <div className="mm-section">
              <h3 className="mm-section-title">Model yang Tersedia</h3>
              <div className="mm-model-list">
                {RECOMMENDED.map(rec => {
                  const dl         = downloads[rec.filename];
                  const isActive   = dl && !dl.done;
                  const err        = errors[rec.filename];
                  const local      = getLocalModel(rec.filename);
                  const done       = isDownloaded(rec.filename);
                  const isSelected = selectedLlm?.path === local?.path;
                  const partial    = !done && !isActive ? (partials[rec.filename] ?? 0) : 0;
                  const hasPartial = partial > 0;

                  return (
                    <div
                      key={rec.id}
                      className={`mm-model-card ${done ? "downloaded" : ""} ${isSelected ? "active-model" : ""}`}
                    >
                      <div className="mm-model-header">
                        <div className="mm-model-name-row">
                          <span className="mm-model-name">{rec.name}</span>
                          {rec.tag && <span className="mm-tag recommended">{rec.tag}</span>}
                          {isSelected && <span className="mm-tag active">✓ Aktif</span>}
                          {done && !isSelected && <span className="mm-tag active">✓ Tersedia</span>}
                          {rec.requires_token && <span className="mm-tag token">🔑 Token</span>}
                          {hasPartial && <span className="mm-tag partial">⏸ Tersimpan</span>}
                        </div>
                        <span className="mm-model-size">{rec.size_gb.toFixed(1)} GB</span>
                      </div>
                      <p className="mm-model-desc">{rec.description}</p>

                      {err && <p className="mm-error">{err}</p>}

                      {isActive && dl ? (
                        <div className="mm-progress">
                          <div className="mm-progress-bar">
                            <div className="mm-progress-fill" style={{ width: `${dl.percent}%` }} />
                          </div>
                          <div className="mm-progress-info">
                            <span className="mm-progress-pct">{dl.percent.toFixed(0)}%</span>
                            <span className="mm-progress-bytes">
                              {fmtGb(dl.downloaded)} / {fmtGb(dl.total)} GB
                            </span>
                            <button className="mm-btn-cancel" onClick={() => handleCancel(rec.filename)}>
                              ✕ Batalkan
                            </button>
                          </div>
                        </div>
                      ) : done && local ? (
                        <div className="mm-actions">
                          {!isSelected && (
                            <button className="mm-btn mm-btn-use" onClick={() => onLlmChange(local)}>
                              ✓ Gunakan
                            </button>
                          )}
                          <button className="mm-btn mm-btn-delete" onClick={() => handleDelete(local)}>
                            🗑 Hapus
                          </button>
                        </div>
                      ) : hasPartial ? (
                        <div className="mm-resume-section">
                          <p className="mm-resume-info">
                            {fmtGb(partial)} GB tersimpan dari {rec.size_gb.toFixed(1)} GB
                          </p>
                          <div className="mm-progress-bar mm-progress-bar--slim">
                            <div
                              className="mm-progress-fill"
                              style={{ width: `${Math.min((partial / (rec.size_gb * 1073741824)) * 100, 100)}%` }}
                            />
                          </div>
                          <div className="mm-actions">
                            <button className="mm-btn mm-btn-download" onClick={() => handleDownload(rec)}>
                              ▶ Lanjutkan
                            </button>
                            <button
                              className="mm-btn mm-btn-delete"
                              onClick={() => handleDiscard(rec.filename)}
                              title="Hapus file yang sudah diunduh dan mulai dari awal"
                            >
                              🗑 Mulai ulang
                            </button>
                          </div>
                        </div>
                      ) : (
                        <button
                          className="mm-btn mm-btn-download"
                          onClick={() => handleDownload(rec)}
                          disabled={rec.requires_token && !hfToken}
                          title={rec.requires_token && !hfToken ? "Masukkan token HuggingFace terlebih dahulu" : ""}
                        >
                          ⬇ Unduh {rec.size_gb.toFixed(1)} GB
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>

            {extraLocalModels.length > 0 && (
              <div className="mm-section">
                <h3 className="mm-section-title">Model Lokal Lainnya</h3>
                <div className="mm-model-list">
                  {extraLocalModels.map(m => (
                    <div key={m.path} className={`mm-model-card downloaded ${selectedLlm?.path === m.path ? "active-model" : ""}`}>
                      <div className="mm-model-header">
                        <div className="mm-model-name-row">
                          <span className="mm-model-name">{m.name}</span>
                          {selectedLlm?.path === m.path && <span className="mm-tag active">✓ Aktif</span>}
                        </div>
                        <span className="mm-model-size">{(m.size_mb / 1024).toFixed(1)} GB</span>
                      </div>
                      <div className="mm-actions">
                        {selectedLlm?.path !== m.path && (
                          <button className="mm-btn mm-btn-use" onClick={() => onLlmChange(m)}>
                            ✓ Gunakan
                          </button>
                        )}
                        <button className="mm-btn mm-btn-delete" onClick={() => handleDelete(m)}>
                          🗑 Hapus
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <p className="mm-footer-note">
              Model disimpan di <code>src-tauri/vendor/llm/</code> (dev) atau folder data app (produksi).
              Anda juga bisa menaruh file .gguf secara manual ke folder tersebut.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
