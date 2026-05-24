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

export interface DepCheck {
  name: string;
  ok: boolean;
  path?: string;
  error?: string;
  install_cmd?: string;
  optional: boolean;
}

export interface DepsStatus {
  all_required_ok: boolean;
  checks: DepCheck[];
  platform: string;
  build_notes: string[];
}
