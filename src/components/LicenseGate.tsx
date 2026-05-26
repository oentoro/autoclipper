import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import type { LicenseInfo } from "../types";

interface Props {
  onLicensed: (info: LicenseInfo) => void;
}

export default function LicenseGate({ onLicensed }: Props) {
  const [checking, setChecking] = useState(true);
  const [key, setKey] = useState("");
  const [activating, setActivating] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<LicenseInfo | null>("check_license").then((info) => {
      if (info) onLicensed(info);
    }).finally(() => setChecking(false));
  }, []);

  async function handleActivate() {
    if (!key.trim()) return;
    setActivating(true);
    setError("");
    try {
      const info = await invoke<LicenseInfo>("activate_license", { key: key.trim() });
      onLicensed(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setActivating(false);
    }
  }

  if (checking) {
    return (
      <div className="loading-overlay" style={{ position: "fixed" }}>
        <div className="loading-box">
          <div className="spinner" />
          <p>Memverifikasi lisensi...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="license-gate">
      <div className="license-card">
        <div className="license-logo">✂</div>
        <h1 className="license-title">AutoClipper</h1>
        <p className="license-subtitle">Masukkan license key untuk melanjutkan</p>

        <div className="license-form">
          <input
            className="license-input"
            type="text"
            placeholder="XXXX-XXXX-XXXX-XXXX"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleActivate()}
            disabled={activating}
            spellCheck={false}
            autoComplete="off"
          />
          <button
            className="btn btn-primary license-btn"
            onClick={handleActivate}
            disabled={activating || !key.trim()}
          >
            {activating ? (
              <><span className="spinner-sm" /> Mengaktifkan...</>
            ) : (
              "Aktifkan Lisensi"
            )}
          </button>
        </div>

        {error && (
          <div className="license-error">{error}</div>
        )}

        <p className="license-buy-text">
          Belum punya license?{" "}
          <a
            className="license-buy-link"
            href="#"
            onClick={(e) => {
              e.preventDefault();
              // Ganti URL ini dengan URL Lemon Squeezy store Anda
              open("https://your-store.lemonsqueezy.com").catch(() => {});
            }}
          >
            Beli sekarang
          </a>
        </p>
      </div>
    </div>
  );
}
