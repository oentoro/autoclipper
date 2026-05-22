import type { DepsStatus } from "../types";

interface Props {
  status: DepsStatus;
  onRetry: () => void;
  onContinue: () => void;
}

export default function DepsCheck({ status, onRetry, onContinue }: Props) {
  const required = status.checks.filter((c) => !c.optional);
  const optional = status.checks.filter((c) => c.optional);
  const missingRequired = required.filter((c) => !c.ok);

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
          {required.map((dep) => (
            <div key={dep.name} className={`dep-item ${dep.ok ? "ok" : "fail"}`}>
              <span className="dep-icon">{dep.ok ? "✓" : "✗"}</span>
              <div className="dep-info">
                <span className="dep-name">{dep.name}</span>
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
          ))}
        </div>

        <div className="deps-section-label" style={{ marginTop: 16 }}>Opsional</div>
        <div className="deps-list">
          {optional.map((dep) => (
            <div key={dep.name} className={`dep-item ${dep.ok ? "ok" : "warn"}`}>
              <span className="dep-icon">{dep.ok ? "✓" : "○"}</span>
              <div className="dep-info">
                <span className="dep-name">{dep.name}</span>
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
          ))}
        </div>

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
