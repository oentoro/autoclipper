export interface SrtSegment {
  index: number;
  start: number;
  end: number;
  text: string;
  start_time: string;
  end_time: string;
}

export interface TranscribeResult {
  segments: SrtSegment[];
  srt_content: string;
  detected_language: string;
  raw_segments: SrtSegment[];
}

export interface TranslateResult {
  segments: SrtSegment[];
  srt_content: string;
}

export interface AnalyzeResult {
  important_indices: number[];
  reasoning: string;
}

export interface Section {
  name: string;
  summary: string;
  start_index: number;
  end_index: number;
}

export interface ClassifyResult {
  sections: Section[];
}

export interface CaptionResult {
  caption_short: string;
  caption_long: string;
  hashtags_short: string[];
  hashtags_long: string[];
}

export interface YtDownloadProgress {
  percent: number;
  speed: string;
  eta: string;
  phase: "downloading" | "merging" | "done";
  downloaded: string;
  total: string;
}

export interface DownloadProgress {
  filename: string;
  downloaded: number;
  total: number;
  percent: number;
  done: boolean;
}

export interface LlmModel {
  name: string;
  path: string;
  ollama_model: string;
  size_mb: number;
  source: "local" | "ollama";
}

export interface ClipResult {
  output_path: string;
  success: boolean;
  message: string;
  total_segments: number;
  duration_secs: number;
}

export type AppStep = "upload" | "ready" | "transcribing" | "transcript" | "clipping" | "done";

export interface FontInfo {
  name: string;
  path: string;
}

export interface SubtitleStyle {
  textColor: string;
  outlineColor: string;
  outlineWidth: number;
  boxEnabled: boolean;
  boxColor: string;
  boxOpacity: number;
  position: "top" | "center" | "bottom";
  allCaps: boolean;
}

export const DEFAULT_SUBTITLE_STYLE: SubtitleStyle = {
  textColor: "#ffffff",
  outlineColor: "#000000",
  outlineWidth: 2,
  boxEnabled: false,
  boxColor: "#000000",
  boxOpacity: 70,
  position: "bottom",
  allCaps: false,
};

export interface ManualClip {
  id: string;
  startSec: number;
  endSec: number;
  label: string;
}

export interface DepCheck {
  name: string;
  ok: boolean;
  path?: string;
  error?: string;
  install_cmd?: string;
  download_url?: string;
  optional: boolean;
}

export interface DepsStatus {
  all_required_ok: boolean;
  checks: DepCheck[];
  platform: string;
  build_notes: string[];
}

export interface WhisperModelInfo {
  id: string;           // "tiny" | "base" | "medium" | "large-v3-turbo"
  name: string;         // "Whisper Tiny"
  backend: string;      // "mlx" | "faster-whisper"
  preset: string;       // "fast" | "balanced" | "accurate" | "best"
  preset_label: string; // "Cepat" | "Seimbang" | "Akurat" | "Terbaik"
  description: string;
  size_mb: number;
  cached: boolean;
  cache_path: string | null;
}

export interface WhisperDownloadProgress {
  model_key: string;  // "{id}-{backend}"
  percent: number;
  done: boolean;
  error: string | null;
}
