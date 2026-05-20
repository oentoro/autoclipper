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
}

export type AppStep = "upload" | "transcribing" | "transcript" | "clipping" | "done";
