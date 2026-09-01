import { useState, useEffect, useRef } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import VideoUpload from "./components/VideoUpload";
import TranscriptView from "./components/TranscriptView";
import ClipResults from "./components/ClipResults";
import DepsCheck from "./components/DepsCheck";
import ModelManager from "./components/ModelManager";
import LicenseGate from "./components/LicenseGate";
import { useLang } from "./i18n";
import type { SrtSegment, TranscribeResult, TranslateResult, AnalyzeResult, ClassifyResult, Section, ClipResult, AppStep, DepsStatus, FontInfo, LlmModel, DownloadProgress, SubtitleStyle, LicenseInfo, YtDownloadProgress, ManualClip } from "./types";
import { DEFAULT_SUBTITLE_STYLE } from "./types";
import { secondsToSrtTime } from "./lib/time";

const WHISPER_LANGS = [
  { code: "",   label: "Auto-detect" },
  { code: "id", label: "Indonesia" },
  { code: "en", label: "English" },
  { code: "ja", label: "日本語" },
  { code: "zh", label: "中文" },
  { code: "ko", label: "한국어" },
  { code: "es", label: "Español" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "pt", label: "Português" },
  { code: "ar", label: "العربية" },
];

const TRANSLATE_LANGS = [
  { code: "id", label: "Indonesia" },
  { code: "en", label: "English" },
  { code: "ja", label: "日本語" },
  { code: "zh", label: "中文" },
  { code: "ko", label: "한국어" },
  { code: "es", label: "Español" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "pt", label: "Português" },
  { code: "ar", label: "العربية" },
];

const DEV_MODE = false;

export default function App() {
  const [licenseInfo, setLicenseInfo] = useState<LicenseInfo | null>(
    DEV_MODE ? { key: "DEV-MODE", instance_id: "", product_name: "AutoClipper", customer_name: "", customer_email: "", platform: "lemonsqueezy" } : null
  );

  if (!licenseInfo) {
    return <LicenseGate onLicensed={setLicenseInfo} />;
  }

  return <AppContent licenseInfo={licenseInfo} />;
}

function AppContent({ licenseInfo }: { licenseInfo: LicenseInfo }) {
  const { lang, setLang, t } = useLang();
  const [showLicenseInfo, setShowLicenseInfo] = useState(false);
  const [depsStatus, setDepsStatus] = useState<DepsStatus | null>(null);
  const [depsChecking, setDepsChecking] = useState(true);
  const [showDeps, setShowDeps] = useState(false);

  function refreshModels() {
    invoke<LlmModel[]>("list_llm_models").then(models => {
      setLlmModels(models);
    }).catch(() => {});
  }

  useEffect(() => {
    invoke<DepsStatus>("check_dependencies")
      .then((status) => {
        setDepsStatus(status);
        setShowDeps(!status.all_required_ok);
      })
      .finally(() => setDepsChecking(false));
    invoke<FontInfo[]>("get_system_fonts").then(setSystemFonts);
    invoke<LlmModel[]>("list_llm_models").then((models) => {
      setLlmModels(models);
      if (models.length > 0) setSelectedLlm(models[0]);
    }).catch(() => {});

    // Always-active download progress listener
    const unlistenPromise = listen<DownloadProgress>("llm-download-progress", event => {
      const p = event.payload;
      setDownloads(prev => ({ ...prev, [p.filename]: p }));
      if (p.done) {
        invoke<LlmModel[]>("list_llm_models").then(models => {
          setLlmModels(models);
          // Auto-select the newly downloaded model if nothing is selected yet
          setSelectedLlm(prev => {
            if (prev) return prev;
            return models.find(m => m.source === "local" && m.path.includes(p.filename)) ?? models[0] ?? null;
          });
          setDownloads(prev => { const n = { ...prev }; delete n[p.filename]; return n; });
        }).catch(() => {});
      }
    });
    return () => { unlistenPromise.then(fn => fn()); };
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

  function handleInstallYtdlp() {
    const ytdlpCheck = depsStatus?.checks.find(c => c.name.startsWith("yt-dlp"));
    const installCmd = ytdlpCheck?.install_cmd;
    if (!installCmd) return;
    setYtdlpInstalling(true);
    invoke("install_dependency", { installCmd })
      .finally(() => {
        setTimeout(() => {
          invoke<DepsStatus>("check_dependencies").then(setDepsStatus).finally(() => setYtdlpInstalling(false));
        }, 3000);
      });
  }

  const [step, setStep] = useState<AppStep>("upload");
  const [videoPath, setVideoPath] = useState<string>("");
  const [segments, setSegments] = useState<SrtSegment[]>([]);
  const [rawSegments, setRawSegments] = useState<SrtSegment[]>([]);
  const [srtContent, setSrtContent] = useState<string>("");
  const [selectedIndices, setSelectedIndices] = useState<Set<number>>(new Set());
  const [sections, setSections] = useState<Section[]>([]);
  const [detectedLanguage, setDetectedLanguage] = useState<string>("");
  const [clipResult, setClipResult] = useState<ClipResult | null>(null);
  const [burnSubtitles, setBurnSubtitles] = useState<boolean>(true);
  const [subtitleMode, setSubtitleMode] = useState<"translated_only" | "bilingual" | "original_only">("translated_only");
  const [originalSegments, setOriginalSegments] = useState<SrtSegment[]>([]);
  const [aspectRatio, setAspectRatio] = useState<string>("original");
  const [smartCrop, setSmartCrop] = useState<boolean>(false);
  const [smartCropTransition, setSmartCropTransition] = useState<"smooth" | "aggressive">("smooth");
  const [censorFaces, setCensorFaces] = useState<boolean>(false);
  const [censorMode, setCensorMode] = useState<"mosaic" | "image">("mosaic");
  const [censorImagePath, setCensorImagePath] = useState<string>("");
  const [censorTarget, setCensorTarget] = useState<"face" | "head">("face");
  const [subtitleFontSize, setSubtitleFontSize] = useState<number>(0);
  const [subtitleFont, setSubtitleFont] = useState<string>("");
  const [subtitleStyle, setSubtitleStyle] = useState<SubtitleStyle>(DEFAULT_SUBTITLE_STYLE);
  const [systemFonts, setSystemFonts] = useState<FontInfo[]>([]);
  const [llmModels, setLlmModels] = useState<LlmModel[]>([]);
  const [selectedLlm, setSelectedLlm] = useState<LlmModel | null>(null);
  const [showModelManager, setShowModelManager] = useState(false);
  const [downloads, setDownloads] = useState<Record<string, DownloadProgress>>({});
  const [sourceLanguage, setSourceLanguage] = useState<string>("");
  const [transcribePreset, setTranscribePreset] = useState<string>("balanced");
  const [maxWordsPerSub, setMaxWordsPerSub] = useState<number>(0);
  const [translateTarget, setTranslateTarget] = useState<string>("id");
  const [maxSectionSecs, setMaxSectionSecs] = useState<number>(60);
  const [manualClips, setManualClips] = useState<ManualClip[]>([]);
  const [verticalTitle, setVerticalTitle] = useState<string>("");
  const [verticalTitleFontSize, setVerticalTitleFontSize] = useState<number>(48);
  const [verticalTitleColor, setVerticalTitleColor] = useState<string>("#ffffff");
  const [ytdlpInstalling, setYtdlpInstalling] = useState<boolean>(false);
  const [error, setError] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(false);
  const [loadingMsg, setLoadingMsg] = useState<string>("");
  const [transcribeElapsed, setTranscribeElapsed] = useState<number>(0);
  const [transcribeDuration, setTranscribeDuration] = useState<number>(0);
  const transcribeTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const transcribeStartRef = useRef<number>(0);
  const [cancelling, setCancelling] = useState(false);
  const cancellingRef = useRef(false);
  const [youtubeDownloading, setYoutubeDownloading] = useState(false);
  const [youtubeProgress, setYoutubeProgress] = useState<YtDownloadProgress | null>(null);
  const youtubeCancelRef = useRef(false);

  function handleVideoSelect(path: string) {
    setVideoPath(path);
    setError("");
    setSegments([]);
    setRawSegments([]);
    setSrtContent("");
    setSelectedIndices(new Set());
    setSections([]);
    setClipResult(null);
    setOriginalSegments([]);
    setTranscribeDuration(0);
    setStep("ready");
  }

  async function handleYoutubeDownload(url: string) {
    youtubeCancelRef.current = false;
    setYoutubeDownloading(true);
    setYoutubeProgress(null);
    setError("");

    const unlisten = await listen<YtDownloadProgress>("yt-download-progress", event => {
      setYoutubeProgress(event.payload);
    });

    try {
      const filepath = await invoke<string>("download_youtube", { url });
      handleVideoSelect(filepath);
    } catch (e) {
      if (!youtubeCancelRef.current) setError(String(e));
    } finally {
      unlisten();
      setYoutubeDownloading(false);
      setYoutubeProgress(null);
      youtubeCancelRef.current = false;
    }
  }

  async function handleCancelYoutube() {
    youtubeCancelRef.current = true;
    await invoke("cancel_youtube_download").catch(() => {});
  }

  async function handleCancel() {
    cancellingRef.current = true;
    setCancelling(true);
    if (step === "transcribing") {
      await invoke("cancel_transcription").catch(() => {});
    } else if (step === "clipping") {
      await invoke("cancel_clipping").catch(() => {});
    }
  }

  async function startTranscription() {
    cancellingRef.current = false;
    setCancelling(false);
    setStep("transcribing");
    setLoading(true);
    setLoadingMsg(t("transcribePrepare"));
    setTranscribeElapsed(0);
    setTranscribeDuration(0);
    transcribeStartRef.current = Date.now();
    transcribeTimerRef.current = setInterval(() => setTranscribeElapsed(s => s + 1), 1000);

    const unlistenProgress = await listen<string>("transcribe-progress", event => {
      const line = event.payload;
      if (line.includes("mlx-whisper")) {
        setLoadingMsg(t("transcribeGPU"));
      } else if (line.includes("faster-whisper")) {
        setLoadingMsg(t("transcribeCPU"));
      } else if (line.includes("Selesai")) {
        setLoadingMsg(t("transcribeFinish"));
      }
    });

    try {
      const result = await invoke<TranscribeResult>("transcribe_video", {
        videoPath,
        sourceLanguage,
        preset: transcribePreset,
        maxWordsPerSub,
      });
      unlistenProgress();
      setSegments(result.segments);
      setRawSegments(result.raw_segments?.length > 0 ? result.raw_segments : result.segments);
      setSrtContent(result.srt_content);
      setDetectedLanguage(result.detected_language);
      setStep("transcript");
    } catch (e) {
      unlistenProgress();
      if (!cancellingRef.current) setError(String(e));
      setStep("ready");
    } finally {
      const elapsed = Math.round((Date.now() - transcribeStartRef.current) / 1000);
      if (transcribeTimerRef.current) {
        clearInterval(transcribeTimerRef.current);
        transcribeTimerRef.current = null;
      }
      setTranscribeDuration(elapsed);
      setLoading(false);
      setLoadingMsg("");
      setTranscribeElapsed(0);
      setCancelling(false);
      cancellingRef.current = false;
    }
  }

  async function handleTranslate() {
    if (segments.length === 0) return;
    setLoading(true);
    setLoadingMsg(t("translating", { lang: TRANSLATE_LANGS.find(l => l.code === translateTarget)?.label ?? translateTarget }));
    setError("");

    try {
      const preTranslateSegments = segments;
      const result = await invoke<TranslateResult>("translate_transcript", {
        segments,
        sourceLanguage: detectedLanguage || "auto",
        targetLanguage: translateTarget,
        modelPath: selectedLlm?.source === "local" ? selectedLlm.path : "",
        ollamaModel: selectedLlm?.source === "ollama" ? selectedLlm.ollama_model : "",
      });
      setOriginalSegments(preTranslateSegments);
      setSubtitleMode("translated_only");
      setSegments(result.segments);
      setSrtContent(result.srt_content);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setLoadingMsg("");
    }
  }

  async function handleClassify() {
    if (segments.length === 0) return;
    setLoading(true);
    setLoadingMsg(t("classifyDetecting"));
    setError("");
    setSections([]);
    setSelectedIndices(new Set());

    const unlistenClassify = await listen<string>("classify-progress", event => {
      setLoadingMsg(event.payload);
    });

    try {
      const modelPath = selectedLlm?.source === "local" ? selectedLlm.path : "";
      const ollamaModel = selectedLlm?.source === "ollama" ? selectedLlm.ollama_model : "";
      const classifySegs = rawSegments.length > 0 ? rawSegments : segments;
      const result = await invoke<ClassifyResult>("classify_transcript", {
        segments: classifySegs,
        modelPath,
        ollamaModel,
        maxSectionSecs: Math.max(20, maxSectionSecs),
      });
      setSections(result.sections);
    } catch (e) {
      setError(String(e));
    } finally {
      unlistenClassify();
      setLoading(false);
      setLoadingMsg("");
    }
  }

  async function handleAnalyze() {
    if (segments.length === 0) return;
    setLoading(true);
    setLoadingMsg(t("analyzeSelecting"));
    setError("");
    try {
      const modelPath = selectedLlm?.source === "local" ? selectedLlm.path : "";
      const ollamaModel = selectedLlm?.source === "ollama" ? selectedLlm.ollama_model : "";
      const result = await invoke<AnalyzeResult>("analyze_transcript", {
        segments,
        modelPath,
        ollamaModel,
      });
      setSelectedIndices(new Set(result.important_indices));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      setLoadingMsg("");
    }
  }

  function toggleSection(sectionIdx: number) {
    const section = sections[sectionIdx];
    if (!section) return;
    const inSection = segments
      .filter(s => s.index >= section.start_index && s.index <= section.end_index)
      .map(s => s.index);
    setSelectedIndices(prev => {
      const next = new Set(prev);
      const allSelected = inSection.every(i => next.has(i));
      if (allSelected) {
        inSection.forEach(i => next.delete(i));
      } else {
        inSection.forEach(i => next.add(i));
      }
      return next;
    });
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

  function addManualClip(startSec: number, endSec: number) {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const n = manualClips.length + 1;
    setManualClips(prev => [...prev, { id, startSec, endSec, label: `Klip ${n}` }]);
  }

  function removeManualClip(id: string) {
    setManualClips(prev => prev.filter(c => c.id !== id));
  }

  async function handleClip() {
    if (selectedIndices.size === 0 && manualClips.length === 0) {
      setError(t("clippingError"));
      return;
    }

    const videoName = videoPath.split("/").pop()?.replace(/\.[^.]+$/, "") ?? "output";
    const outputPath = await save({
      filters: [{ name: "Video MP4", extensions: ["mp4"] }],
      defaultPath: `${videoName}_short.mp4`,
      title: "Simpan video hasil gabungan",
    });

    if (!outputPath) return;

    cancellingRef.current = false;
    setCancelling(false);
    setStep("clipping");
    setLoading(true);
    const totalItems = selectedIndices.size + manualClips.length;
    setLoadingMsg(t("clippingMerging", { n: totalItems }));
    setError("");

    const unlistenConcat = await listen<number>("clip-concat-percent", event => {
      const pct = event.payload;
      if (pct < 100) setLoadingMsg(t("clippingConcat", { pct }));
    });

    const unlistenSmart = await listen<number>("clip-smart-percent", event => {
      const pct = event.payload;
      if (pct < 56) {
        setLoadingMsg(t("clippingFaces", { pct: Math.round(pct / 55 * 100) }));
      } else if (pct < 100) {
        setLoadingMsg(t("clippingSmartCrop", { pct: Math.round((pct - 56) / 43 * 100) }));
      } else {
        setLoadingMsg(t("clippingFinalizing"));
      }
    });

    const unlistenBurn = await listen<number>("clip-burn-percent", event => {
      const pct = event.payload;
      if (pct >= 100) {
        setLoadingMsg(t("clippingFinalizing"));
      } else {
        setLoadingMsg(t("clippingBurn", { pct }));
      }
    });

    const unlistenCensor = await listen<number>("clip-censor-percent", event => {
      const pct = event.payload;
      if (pct >= 100) {
        setLoadingMsg(t("clippingFinalizing"));
      } else {
        setLoadingMsg(t("clippingCensor", { pct }));
      }
    });

    try {
      const result = await invoke<ClipResult>("clip_video", {
        videoPath,
        segments,
        selectedIndices: Array.from(selectedIndices),
        outputPath: outputPath as string,
        burnSubtitles,
        subtitleMode,
        originalSegments,
        aspectRatio,
        smartCrop,
        smartCropTransition,
        censorFaces,
        censorImagePath: censorFaces && censorMode === "image" ? censorImagePath : "",
        censorTarget,
        fontSize: subtitleFontSize,
        fontPath: subtitleFont,
        subtitleStyleJson: JSON.stringify(subtitleStyle),
        manualClips: manualClips.map(c => ({ start_sec: c.startSec, end_sec: c.endSec, label: c.label })),
        verticalTitle,
        verticalTitleFontSize,
        verticalTitleColor,
      });
      setClipResult(result);
      setStep("done");
    } catch (e) {
      if (!cancellingRef.current) setError(String(e));
      setStep("transcript");
    } finally {
      unlistenConcat();
      unlistenSmart();
      unlistenBurn();
      unlistenCensor();
      setLoading(false);
      setLoadingMsg("");
      setCancelling(false);
      cancellingRef.current = false;
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

  function handleSegmentEdit(index: number, newText: string) {
    const updated = segments.map(s =>
      s.index === index ? { ...s, text: newText } : s
    );
    setSegments(updated);
    setSrtContent(
      updated.map(s => `${s.index}\n${s.start_time} --> ${s.end_time}\n${s.text}\n`).join("\n")
    );
  }

  function handleSegmentTimeChange(index: number, newStart: number, newEnd: number) {
    const updated = segments.map(s =>
      s.index === index
        ? { ...s, start: newStart, end: newEnd, start_time: secondsToSrtTime(newStart), end_time: secondsToSrtTime(newEnd) }
        : s
    );
    setSegments(updated);
    setSrtContent(
      updated.map(s => `${s.index}\n${s.start_time} --> ${s.end_time}\n${s.text}\n`).join("\n")
    );
  }

  function handleReset() {
    if (segments.length > 0 && !confirm(t("resetConfirm"))) return;
    setStep("upload");
    setVideoPath("");
    setSegments([]);
    setRawSegments([]);
    setSrtContent("");
    setSelectedIndices(new Set());
    setSections([]);
    setManualClips([]);
    setClipResult(null);
    setError("");
    setSourceLanguage("");
    setDetectedLanguage("");
    setTranscribeDuration(0);
  }

  if (depsChecking) {
    return (
      <div className="loading-overlay" style={{ position: "fixed" }}>
        <div className="loading-box">
          <div className="spinner" />
          <p>{t("checkingDeps")}</p>
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
          <p className="app-subtitle">{t("appSubtitle")}</p>
        </div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => setShowModelManager(true)}
          >
            {t("btnModels")}
            {Object.keys(downloads).length > 0 && (
              <span className="dl-badge">{Object.keys(downloads).length}</span>
            )}
          </button>
          <button className="btn btn-ghost btn-sm" onClick={() => setShowDeps(true)}>
            {t("btnDeps")}
          </button>
          {licenseInfo && licenseInfo.key !== "DEV-MODE" && (
            <button className="btn btn-ghost btn-sm" onClick={() => setShowLicenseInfo(true)}>
              {t("btnLicense")}
            </button>
          )}
          {(step !== "upload") && (
            <button className="btn btn-ghost" onClick={handleReset}>
              {t("btnRestart")}
            </button>
          )}
          <select
            className="lang-select"
            value={lang}
            onChange={e => setLang(e.target.value as "id" | "en")}
          >
            <option value="id">🇮🇩 Indonesia</option>
            <option value="en">🇬🇧 English</option>
          </select>
        </div>
      </header>

      <div className="step-bar">
        {["upload", "transcript", "done"].map((s, i) => {
          const labels = [t("stepUpload"), t("stepTranscript"), t("stepDone")];
          const current = (step === "upload" || step === "ready") ? 0 : step === "transcribing" ? 0 : step === "clipping" ? 2 : ["upload", "transcript", "done"].indexOf(step);
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
              {step === "transcribing" && (
                <div className="transcribe-timer">
                  {String(Math.floor(transcribeElapsed / 60)).padStart(2, "0")}:{String(transcribeElapsed % 60).padStart(2, "0")}
                </div>
              )}
              {(step === "transcribing" || step === "clipping") && (
                cancelling
                  ? <p className="cancel-hint">{t("cancelling")}</p>
                  : <button className="btn btn-cancel" onClick={handleCancel}>{t("btnCancel")}</button>
              )}
            </div>
          </div>
        )}

        {(step === "upload" || step === "transcribing") && (
          <VideoUpload
            onSelect={handleVideoSelect}
            disabled={step === "transcribing"}
            onYoutubeDownload={handleYoutubeDownload}
            onCancelYoutube={handleCancelYoutube}
            youtubeDownloading={youtubeDownloading}
            youtubeProgress={youtubeProgress}
            ytdlpOk={depsStatus?.checks.some(c => c.name.startsWith("yt-dlp") && c.ok) ?? true}
            ytdlpInstalling={ytdlpInstalling}
            onInstallYtdlp={handleInstallYtdlp}
          />
        )}

        {step === "ready" && (
          <div className="ready-layout">
            <div className="ready-preview">
              <video
                key={videoPath}
                src={convertFileSrc(videoPath)}
                controls
                className="ready-video-player"
              />
              <p className="ready-preview-name" title={videoPath}>
                {videoPath.split("/").pop()}
              </p>
            </div>

            <div className="ready-panel">
              <div className="ready-video-row">
                <span className="ready-video-icon">🎬</span>
                <span className="ready-video-name" title={videoPath}>
                  {videoPath.split("/").pop()}
                </span>
                <button className="btn btn-ghost btn-sm" onClick={() => setStep("upload")}>
                  {t("btnChange")}
                </button>
              </div>

              <div className="ready-options">
                <div className="ready-lang-row">
                  <label className="ready-lang-label">{t("labelSourceLang")}</label>
                  <select
                    className="ready-lang-select"
                    value={sourceLanguage}
                    onChange={(e) => setSourceLanguage(e.target.value)}
                  >
                    {WHISPER_LANGS.map((l) => (
                      <option key={l.code} value={l.code}>{l.label}</option>
                    ))}
                  </select>
                </div>

                <div className="ready-lang-row">
                  <label className="ready-lang-label">{t("labelSpeed")}</label>
                  <div className="preset-group">
                    {([
                      { value: "fast",     labelKey: "presetFast",     hintKey: "presetFastHint"     },
                      { value: "balanced", labelKey: "presetBalanced", hintKey: "presetBalancedHint" },
                      { value: "accurate", labelKey: "presetAccurate", hintKey: "presetAccurateHint" },
                      { value: "best",     labelKey: "presetBest",     hintKey: "presetBestHint"     },
                    ] as const).map(p => (
                      <button
                        key={p.value}
                        className={`preset-btn ${transcribePreset === p.value ? "active" : ""}`}
                        onClick={() => setTranscribePreset(p.value)}
                        title={t(p.hintKey)}
                      >
                        {t(p.labelKey)}
                      </button>
                    ))}
                  </div>
                </div>

                <div className="ready-lang-row">
                  <label className="ready-lang-label">{t("labelWordsPerSub")}</label>
                  <div className="preset-group">
                    {([
                      { value: 0, labelKey: "wordsAuto", hintKey: "wordsAutoHint" },
                      { value: 2, labelKey: "words2",    hintKey: "words2Hint"    },
                      { value: 3, labelKey: "words3",    hintKey: "words3Hint"    },
                      { value: 5, labelKey: "words5",    hintKey: "words5Hint"    },
                    ] as const).map(p => (
                      <button
                        key={p.value}
                        className={`preset-btn ${maxWordsPerSub === p.value ? "active" : ""}`}
                        onClick={() => setMaxWordsPerSub(p.value)}
                        title={t(p.hintKey)}
                      >
                        {t(p.labelKey)}
                      </button>
                    ))}
                  </div>
                </div>
              </div>

              <button
                className="btn btn-primary ready-generate-btn"
                onClick={startTranscription}
              >
                {t("btnGenerate")}
              </button>
            </div>
          </div>
        )}

        {(step === "transcript") && (
          <TranscriptView
            segments={segments}
            selectedIndices={selectedIndices}
            sections={sections}
            burnSubtitles={burnSubtitles}
            aspectRatio={aspectRatio}
            transcribeDuration={transcribeDuration}
            onToggle={toggleSegment}
            onSelectAll={selectAll}
            onClearAll={clearAll}
            onClassify={handleClassify}
            onAnalyze={handleAnalyze}
            onToggleSection={toggleSection}
            onClip={handleClip}
            onSaveSrt={handleSaveSrt}
            onSegmentEdit={handleSegmentEdit}
            onSegmentTimeChange={handleSegmentTimeChange}
            onBurnSubtitlesChange={setBurnSubtitles}
            subtitleMode={subtitleMode}
            onSubtitleModeChange={setSubtitleMode}
            hasTranslation={originalSegments.length > 0}
            onAspectRatioChange={(v) => { setAspectRatio(v); if (v === "original") setSmartCrop(false); }}
            smartCrop={smartCrop}
            onSmartCropChange={setSmartCrop}
            smartCropTransition={smartCropTransition}
            onSmartCropTransitionChange={setSmartCropTransition}
            censorFaces={censorFaces}
            onCensorFacesChange={setCensorFaces}
            censorMode={censorMode}
            onCensorModeChange={setCensorMode}
            censorImagePath={censorImagePath}
            onCensorImagePathChange={setCensorImagePath}
            censorTarget={censorTarget}
            onCensorTargetChange={setCensorTarget}
            subtitleFontSize={subtitleFontSize}
            subtitleFont={subtitleFont}
            systemFonts={systemFonts}
            onSubtitleFontSizeChange={setSubtitleFontSize}
            onSubtitleFontChange={setSubtitleFont}
            subtitleStyle={subtitleStyle}
            onSubtitleStyleChange={setSubtitleStyle}
            loading={loading}
            videoPath={videoPath}
            detectedLanguage={detectedLanguage}
            translateTarget={translateTarget}
            translateLangs={TRANSLATE_LANGS}
            onTranslate={handleTranslate}
            onTranslateTargetChange={setTranslateTarget}
            llmModels={llmModels}
            selectedLlm={selectedLlm}
            onLlmChange={setSelectedLlm}
            onManageModels={() => setShowModelManager(true)}
            maxSectionSecs={maxSectionSecs}
            onMaxSectionSecsChange={setMaxSectionSecs}
            manualClips={manualClips}
            onAddManualClip={addManualClip}
            onRemoveManualClip={removeManualClip}
            verticalTitle={verticalTitle}
            verticalTitleFontSize={verticalTitleFontSize}
            verticalTitleColor={verticalTitleColor}
            onVerticalTitleChange={setVerticalTitle}
            onVerticalTitleFontSizeChange={setVerticalTitleFontSize}
            onVerticalTitleColorChange={setVerticalTitleColor}
          />
        )}

        {(step === "clipping" || step === "done") && (() => {
          // Gabungkan segmen dari checkbox + segmen dalam rentang manual clip,
          // deduplikasi berdasarkan index, urutkan berdasarkan waktu.
          const seen = new Set<number>();
          const captionSegments = [
            ...segments.filter(s => selectedIndices.has(s.index)),
            ...segments.filter(s =>
              manualClips.some(mc => s.start >= mc.startSec && s.end <= mc.endSec)
            ),
          ].filter(s => {
            if (seen.has(s.index)) return false;
            seen.add(s.index);
            return true;
          }).sort((a, b) => a.start - b.start);
          return (
            <ClipResults
              result={clipResult}
              loading={step === "clipping"}
              onBack={() => setStep("transcript")}
              selectedSegments={captionSegments}
              detectedLanguage={detectedLanguage}
              modelPath={selectedLlm?.source === "local" ? selectedLlm.path : ""}
              ollamaModel={selectedLlm?.source === "ollama" ? selectedLlm.ollama_model : ""}
            />
          );
        })()}
      </main>

      {showModelManager && (
        <ModelManager
          llmModels={llmModels}
          selectedLlm={selectedLlm}
          downloads={downloads}
          onLlmChange={(m) => { setSelectedLlm(m); setShowModelManager(false); }}
          onClose={() => setShowModelManager(false)}
          onRefresh={refreshModels}
        />
      )}

      {showLicenseInfo && licenseInfo && (
        <div className="modal-overlay" onClick={() => setShowLicenseInfo(false)}>
          <div className="modal-box license-info-modal" onClick={(e) => e.stopPropagation()}>
            <h3 className="modal-title">{t("licenseTitle")}</h3>
            <div className="license-info-rows">
              <div className="license-info-row">
                <span className="license-info-label">{t("licenseProduct")}</span>
                <span className="license-info-value">{licenseInfo.product_name}</span>
              </div>
              {licenseInfo.customer_name && (
                <div className="license-info-row">
                  <span className="license-info-label">{t("licenseName")}</span>
                  <span className="license-info-value">{licenseInfo.customer_name}</span>
                </div>
              )}
              {licenseInfo.customer_email && (
                <div className="license-info-row">
                  <span className="license-info-label">{t("licenseEmail")}</span>
                  <span className="license-info-value">{licenseInfo.customer_email}</span>
                </div>
              )}
              <div className="license-info-row">
                <span className="license-info-label">{t("licenseKey")}</span>
                <span className="license-info-value license-key-display">{licenseInfo.key}</span>
              </div>
            </div>
            <div className="license-info-actions">
              <button className="btn btn-ghost" onClick={() => setShowLicenseInfo(false)}>{t("btnClose")}</button>
              <button
                className="btn btn-danger"
                onClick={async () => {
                  if (!confirm(t("licenseDeactivateConfirm"))) return;
                  try {
                    await invoke("deactivate_license");
                    window.location.reload();
                  } catch (e) {
                    alert(String(e));
                  }
                }}
              >
                {t("btnDeactivate")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

