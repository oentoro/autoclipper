import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import type { DepsStatus } from "../types";

interface Props {
  status: DepsStatus;
  onRetry: () => void;
  onContinue: () => void;
}

export default function DepsCheck({ status, onRetry, onContinue }: Props) {
  const [installing, setInstalling] = useState<Set<string>>(new Set());

  async function handleInstall(name: string, installCmd: string) {
    setInstalling((prev) => new Set(prev).add(name));
    try {
      await invoke("install_dependency", { installCmd });
    } catch (e) {
      console.error(e);
    } finally {
      setInstalling((prev) => {
        const s = new Set(prev);
        s.delete(name);
        return s;
      });
    }
  }

  const required = status.checks.filter((c) => !c.optional);
  const optional = status.checks.filter((c) => c.optional);
  const missingRequired = required.filter((c) => !c.ok);

  function renderDep(dep: DepsStatus["checks"][number]) {
    const isInstalling = installing.has(dep.name);
    return (
      <div key={dep.name} className={`dep-item ${dep.ok ? "ok" : "fail"}`}>
        <span className="dep-icon">{dep.ok ? "✓" : "✗"}</span>
        <div className="dep-info">
          <div className="dep-name-row">
            <span className="dep-name">{dep.name}</span>
            {!dep.ok && dep.install_cmd && (
              <button
                className="btn btn-install btn-sm"
                onClick={() => handleInstall(dep.name, dep.install_cmd!)}
                disabled={isInstalling}
                title={dep.install_cmd}
              >
                {isInstalling ? "Membuka terminal..." : "↓ Install"}
              </button>
            )}
            {!dep.ok && dep.download_url && (
              <button
                className="btn btn-secondary btn-sm"
                onClick={() => openUrl(dep.download_url!)}
                title={dep.download_url}
              >
                ↗ Download Manual
              </button>
            )}
          </div>
          {dep.ok && dep.path && (
            <span className="dep-path">{dep.path}</span>
          )}
          {!dep.ok && dep.error && (
            <span className="dep-error">{dep.error}</span>
          )}
          {!dep.ok && dep.install_cmd && (
            <code className="dep-cmd">{dep.install_cmd}</code>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="deps-screen">
      <div className="deps-box">
        <div className="deps-header">
          <span className="deps-icon">{status.all_required_ok ? "✓" : "⚠"}</span>
          <div>
            <h2 className="deps-title">Pemeriksaan Dependensi</h2>
            <p className="deps-subtitle">
              {status.all_required_ok
                ? "Semua dependensi wajib terpenuhi"
                : `${missingRequired.length} dependensi wajib belum terinstall`}
            </p>
          </div>
        </div>

        <div className="deps-section-label">Wajib</div>
        <div className="deps-list">
          {required.map(renderDep)}
        </div>

        <div className="deps-section-label" style={{ marginTop: 16 }}>Opsional</div>
        <div className="deps-list">
          {optional.map((dep) => {
            const isInstalling = installing.has(dep.name);
            return (
              <div key={dep.name} className={`dep-item ${dep.ok ? "ok" : "warn"}`}>
                <span className="dep-icon">{dep.ok ? "✓" : "○"}</span>
                <div className="dep-info">
                  <div className="dep-name-row">
                    <span className="dep-name">{dep.name}</span>
                    {!dep.ok && dep.install_cmd && (
                      <button
                        className="btn btn-install btn-sm"
                        onClick={() => handleInstall(dep.name, dep.install_cmd!)}
                        disabled={isInstalling}
                        title={dep.install_cmd}
                      >
                        {isInstalling ? "Membuka terminal..." : "↓ Install"}
                      </button>
                    )}
                    {!dep.ok && dep.download_url && (
                      <button
                        className="btn btn-secondary btn-sm"
                        onClick={() => openUrl(dep.download_url!)}
                        title={dep.download_url}
                      >
                        ↗ Download Manual
                      </button>
                    )}
                  </div>
                  {dep.ok && dep.path && (
                    <span className="dep-path">{dep.path}</span>
                  )}
                  {!dep.ok && dep.error && (
                    <span className="dep-error">{dep.error}</span>
                  )}
                  {!dep.ok && dep.install_cmd && (
                    <code className="dep-cmd">{dep.install_cmd}</code>
                  )}
                </div>
              </div>
            );
          })}
        </div>

        {status.build_notes.length > 0 && (
          <div className="deps-build-notes">
            <p className="deps-section-label">Persyaratan Build</p>
            <div className="deps-notes-body">
              {status.build_notes.map((note, i) =>
                i === 0
                  ? <p key={i} className="deps-note-header">{note}</p>
                  : <code key={i} className="deps-note-cmd">{note}</code>
              )}
            </div>
          </div>
        )}

        {!status.all_required_ok && (
          <p className="deps-install-hint">
            Klik "↓ Install" untuk membuka terminal dan menjalankan installer secara otomatis.
            Setelah selesai, klik "Periksa Ulang".
          </p>
        )}

        <div className="deps-actions">
          <button className="btn btn-secondary" onClick={onRetry}>
            ↺ Periksa Ulang
          </button>
          {status.all_required_ok && (
            <button className="btn btn-primary" onClick={onContinue}>
              Lanjutkan →
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
