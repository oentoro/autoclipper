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
}

export interface AnalyzeResult {
  important_indices: number[];
  reasoning: string;
}

export interface ClipResult {
  output_path: string;
  success: boolean;
  message: string;
  total_segments: number;
  duration_secs: number;
}

export type AppStep = "upload" | "transcribing" | "transcript" | "clipping" | "done";

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
}
