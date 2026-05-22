import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import VideoUpload from "./components/VideoUpload";
import TranscriptView from "./components/TranscriptView";
import ClipResults from "./components/ClipResults";
import DepsCheck from "./components/DepsCheck";
import type { SrtSegment, TranscribeResult, AnalyzeResult, ClipResult, AppStep, DepsStatus } from "./types";

export default function App() {
  const [depsStatus, setDepsStatus] = useState<DepsStatus | null>(null);
  const [depsChecking, setDepsChecking] = useState(true);
  const [showDeps, setShowDeps] = useState(false);

  useEffect(() => {
    invoke<DepsStatus>("check_dependencies")
      .then((status) => {
        setDepsStatus(status);
        setShowDeps(!status.all_required_ok);
      })
      .finally(() => setDepsChecking(false));
  }, []);

  function recheckDeps() {
    setDepsChecking(true);
    invoke<DepsStatus>("check_dependencies")
      .then((status) => {
        setDepsStatus(status);
        if (status.all_required_ok) setShowDeps(false);
      })
      .finally(() => setDepsChecking(false));
  }

  const [step, setStep] = useState<AppStep>("upload");
  const [videoPath, setVideoPath] = useState<string>("");
  const [segments, setSegments] = useState<SrtSegment[]>([]);
  const [srtContent, setSrtContent] = useState<string>("");
  const [selectedIndices, setSelectedIndices] = useState<Set<number>>(new Set());
  const [aiReasoning, setAiReasoning] = useState<string>("");
  const [clipResult, setClipResult] = useState<ClipResult | null>(null);
  const [burnSubtitles, setBurnSubtitles] = useState<boolean>(true);
  const [aspectRatio, setAspectRatio] = useState<string>("original");
  const [error, setError] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(false);
  const [loadingMsg, setLoadingMsg] = useState<string>("");

  async function handleVideoSelect(path: string) {
    setVideoPath(path);
    setError("");
    setSegments([]);
    setSrtContent("");
    setSelectedIndices(new Set());
    setAiReasoning("");
    setClipResult(null);
    await startTranscription(path);
  }

  async function startTranscription(path: string) {
    setStep("transcribing");
    setLoading(true);
    setLoadingMsg("Mentranskripsi audio menggunakan Whisper... (mungkin beberapa menit)");

    try {
      const result = await invoke<TranscribeResult>("transcribe_video", { videoPath: path });
      setSegments(result.segments);
      setSrtContent(result.srt_content);
      setStep("transcript");
    } catch (e) {
      setError(String(e));
      setStep("upload");
    } finally {
      setLoading(false);
      setLoadingMsg("");
    }
  }

  async function handleAiAnalyze() {
    if (segments.length === 0) return;
    setLoading(true);
    setLoadingMsg("AI sedang menganalisis transkrip...");
    setError("");

    try {
      const result = await invoke<AnalyzeResult>("analyze_transcript", { segments });
      setSelectedIndices(new Set(result.important_indices));
      setAiReasoning(result.reasoning);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setLoadingMsg("");
    }
  }

  function toggleSegment(index: number) {
    setSelectedIndices((prev) => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  }

  function selectAll() {
    setSelectedIndices(new Set(segments.map((s) => s.index)));
  }

  function clearAll() {
    setSelectedIndices(new Set());
  }

  async function handleClip() {
    if (selectedIndices.size === 0) {
      setError("Pilih minimal satu segmen untuk dijadikan clip");
      return;
    }

    const videoName = videoPath.split("/").pop()?.replace(/\.[^.]+$/, "") ?? "output";
    const outputPath = await save({
      filters: [{ name: "Video MP4", extensions: ["mp4"] }],
      defaultPath: `${videoName}_short.mp4`,
      title: "Simpan video hasil gabungan",
    });

    if (!outputPath) return;

    setStep("clipping");
    setLoading(true);
    setLoadingMsg(`Menggabungkan ${selectedIndices.size} segmen menjadi satu video...`);
    setError("");

    try {
      const result = await invoke<ClipResult>("clip_video", {
        videoPath,
        segments,
        selectedIndices: Array.from(selectedIndices),
        outputPath: outputPath as string,
        burnSubtitles,
        aspectRatio,
      });
      setClipResult(result);
      setStep("done");
    } catch (e) {
      setError(String(e));
      setStep("transcript");
    } finally {
      setLoading(false);
      setLoadingMsg("");
    }
  }

  async function handleSaveSrt() {
    const path = await save({
      filters: [{ name: "SRT", extensions: ["srt"] }],
      defaultPath: "transkrip.srt",
    });
    if (path) {
      const { writeTextFile } = await import("@tauri-apps/plugin-fs");
      await writeTextFile(path, srtContent);
    }
  }

  function handleReset() {
    setStep("upload");
    setVideoPath("");
    setSegments([]);
    setSrtContent("");
    setSelectedIndices(new Set());
    setAiReasoning("");
    setClipResult(null);
    setError("");
  }

  if (depsChecking) {
    return (
      <div className="loading-overlay" style={{ position: "fixed" }}>
        <div className="loading-box">
          <div className="spinner" />
          <p>Memeriksa dependensi...</p>
        </div>
      </div>
    );
  }

  if (showDeps && depsStatus) {
    return (
      <DepsCheck
        status={depsStatus}
        onRetry={recheckDeps}
        onContinue={() => setShowDeps(false)}
      />
    );
  }

  return (
    <div className="app">
      <header className="app-header">
        <div className="header-content">
          <h1 className="app-title">
            <span className="title-icon">✂</span>
            AutoClipper
          </h1>
          <p className="app-subtitle">Clipping video otomatis berbasis AI</p>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-ghost btn-sm" onClick={() => setShowDeps(true)} title="Cek dependensi">
            ⚙ Dependensi
          </button>
          {step !== "upload" && (
            <button className="btn btn-ghost" onClick={handleReset}>
              ↩ Mulai Ulang
            </button>
          )}
        </div>
      </header>

      <div className="step-bar">
        {["upload", "transcript", "done"].map((s, i) => {
          const labels = ["Upload Video", "Pilih Segmen", "Hasil Clip"];
          const current = step === "transcribing" ? 0 : step === "clipping" ? 2 : ["upload", "transcript", "done"].indexOf(step);
          return (
            <div key={s} className={`step-item ${i <= current ? "active" : ""}`}>
              <div className="step-num">{i + 1}</div>
              <span>{labels[i]}</span>
            </div>
          );
        })}
      </div>

      <main className="app-main">
        {error && (
          <div className="alert alert-error">
            <strong>Error:</strong> {error}
            <button className="alert-close" onClick={() => setError("")}>✕</button>
          </div>
        )}

        {loading && (
          <div className="loading-overlay">
            <div className="loading-box">
              <div className="spinner" />
              <p>{loadingMsg}</p>
            </div>
          </div>
        )}

        {(step === "upload" || step === "transcribing") && (
          <VideoUpload onSelect={handleVideoSelect} disabled={step === "transcribing"} />
        )}

        {(step === "transcript") && (
          <TranscriptView
            segments={segments}
            selectedIndices={selectedIndices}
            aiReasoning={aiReasoning}
            burnSubtitles={burnSubtitles}
            aspectRatio={aspectRatio}
            onToggle={toggleSegment}
            onSelectAll={selectAll}
            onClearAll={clearAll}
            onAiAnalyze={handleAiAnalyze}
            onClip={handleClip}
            onSaveSrt={handleSaveSrt}
            onBurnSubtitlesChange={setBurnSubtitles}
            onAspectRatioChange={setAspectRatio}
            loading={loading}
            videoPath={videoPath}
          />
        )}

        {(step === "clipping" || step === "done") && (
          <ClipResults
            result={clipResult}
            loading={step === "clipping"}
            onBack={() => setStep("transcript")}
          />
        )}
      </main>
    </div>
  );
}
