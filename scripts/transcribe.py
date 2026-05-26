#!/usr/bin/env python3
import sys
import json
import os
import subprocess
import tempfile

PRESETS = {
    "fast":     {"model": "tiny",           "beam_size": 1, "batch": True,  "condition_prev": False},
    "balanced": {"model": "base",           "beam_size": 2, "batch": True,  "condition_prev": False},
    "accurate": {"model": "medium",         "beam_size": 5, "batch": False, "condition_prev": True},
    "best":     {"model": "large-v3-turbo", "beam_size": 5, "batch": False, "condition_prev": True},
}

def seconds_to_srt_time(seconds: float) -> str:
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = int(seconds % 60)
    millis = int((seconds - int(seconds)) * 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{millis:03d}"

def split_segment_by_words(seg, max_words: int) -> list[dict]:
    words = getattr(seg, "words", None)
    text = seg.text.strip()

    if not words:
        raw_words = text.split()
        if len(raw_words) <= max_words:
            return [{"text": text, "start": seg.start, "end": seg.end}]
        duration = seg.end - seg.start
        secs_per_word = duration / max(len(raw_words), 1)
        chunks = []
        for i in range(0, len(raw_words), max_words):
            chunk_words = raw_words[i:i + max_words]
            chunk_start = seg.start + i * secs_per_word
            chunk_end   = seg.start + min((i + max_words), len(raw_words)) * secs_per_word
            chunks.append({"text": " ".join(chunk_words), "start": chunk_start, "end": chunk_end})
        return chunks

    chunks = []
    for i in range(0, len(words), max_words):
        chunk = words[i:i + max_words]
        chunk_text = "".join(w.word for w in chunk).strip()
        if chunk_text:
            chunks.append({"text": chunk_text, "start": chunk[0].start, "end": chunk[-1].end})
    return chunks

def build_output(raw_chunks: list[dict]) -> tuple:
    segments = []
    srt_lines = []
    idx = 1
    for c in raw_chunks:
        text = c["text"]
        if not text:
            continue
        start_time = seconds_to_srt_time(c["start"])
        end_time   = seconds_to_srt_time(c["end"])
        segments.append({
            "index": idx,
            "start": c["start"],
            "end":   c["end"],
            "text":  text,
            "start_time": start_time,
            "end_time":   end_time,
        })
        srt_lines += [str(idx), f"{start_time} --> {end_time}", text, ""]
        idx += 1
    return segments, "\n".join(srt_lines)

def detect_device() -> tuple[str, str]:
    """Detect best available compute device (CUDA > CPU)."""
    try:
        import ctranslate2
        if ctranslate2.get_cuda_device_count() > 0:
            return "cuda", "float16"
    except Exception:
        pass
    return "cpu", "int8"

def get_cpu_threads() -> int:
    cpu_count = os.cpu_count() or 4
    # Leave 1 core for OS; minimum 4 for performance
    return max(4, cpu_count - 1)

def extract_audio(video_path: str, ffmpeg: str) -> str | None:
    """
    Pre-extract 16kHz mono WAV from video using FFmpeg.
    Whisper only needs 16kHz mono — skipping video decode saves significant time
    on high-bitrate or high-resolution videos.
    Returns temp WAV path, or None if extraction fails (fall back to direct path).
    """
    tmp = tempfile.NamedTemporaryFile(suffix=".wav", delete=False)
    tmp.close()
    try:
        result = subprocess.run(
            [ffmpeg, "-y", "-i", video_path,
             "-vn",                    # no video
             "-ar", "16000",           # 16kHz (Whisper native sample rate)
             "-ac", "1",               # mono
             "-sample_fmt", "s16",     # 16-bit PCM
             "-f", "wav", tmp.name],
            capture_output=True, timeout=300
        )
        if result.returncode == 0:
            return tmp.name
    except Exception:
        pass
    try:
        os.unlink(tmp.name)
    except Exception:
        pass
    return None

def transcribe(video_path: str, model_dir: str | None = None,
               language: str | None = None, preset: str = "balanced",
               max_words: int = 0) -> dict:
    from faster_whisper import WhisperModel

    cfg = PRESETS.get(preset, PRESETS["balanced"])
    model_name     = cfg["model"]
    beam_size      = cfg["beam_size"]
    use_batch      = cfg["batch"]
    condition_prev = cfg["condition_prev"]

    device, compute_type = detect_device()
    cpu_threads = get_cpu_threads()
    num_workers = min(4, max(2, (os.cpu_count() or 4) // 2))

    model_kwargs = {
        "device":       device,
        "compute_type": compute_type,
        "num_workers":  num_workers,
    }
    if device == "cpu":
        model_kwargs["cpu_threads"] = cpu_threads
    if model_dir and os.path.isdir(model_dir):
        model_kwargs["download_root"] = model_dir

    model = WhisperModel(model_name, **model_kwargs)

    # Pre-extract audio for faster inference (skip video decode overhead)
    ffmpeg = os.environ.get("AUTOCLIPPER_FFMPEG", "ffmpeg")
    audio_path = extract_audio(video_path, ffmpeg)
    input_path = audio_path if audio_path else video_path

    need_words = max_words > 0
    transcribe_kwargs = dict(
        language=language or None,
        beam_size=beam_size,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=500),
        word_timestamps=need_words,
        condition_on_previous_text=condition_prev,
    )

    segments_raw = None
    info = None

    try:
        if use_batch:
            try:
                from faster_whisper import BatchedInferencePipeline
                batched = BatchedInferencePipeline(model=model)
                segments_raw, info = batched.transcribe(input_path, batch_size=16, **transcribe_kwargs)
            except Exception:
                segments_raw, info = model.transcribe(input_path, **transcribe_kwargs)
        else:
            segments_raw, info = model.transcribe(input_path, **transcribe_kwargs)

        raw_chunks = []
        for seg in segments_raw:
            if not seg.text.strip():
                continue
            if need_words:
                raw_chunks.extend(split_segment_by_words(seg, max_words))
            else:
                raw_chunks.append({"text": seg.text.strip(), "start": seg.start, "end": seg.end})

        segments, srt_content = build_output(raw_chunks)

        return {
            "segments":          segments,
            "srt_content":       srt_content,
            "detected_language": info.language or "unknown",
        }
    finally:
        if audio_path:
            try:
                os.unlink(audio_path)
            except Exception:
                pass

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: transcribe.py <video_path> [options]"}))
        sys.exit(1)

    video_path = sys.argv[1]

    def get_arg(flag):
        if flag in sys.argv:
            i = sys.argv.index(flag)
            if i + 1 < len(sys.argv):
                return sys.argv[i + 1]
        return None

    model_dir = get_arg("--model-dir")
    language  = get_arg("--language") or None
    preset    = get_arg("--preset") or "balanced"
    max_words = int(get_arg("--max-words") or 0)

    if not os.path.exists(video_path):
        print(json.dumps({"error": f"File tidak ditemukan: {video_path}"}))
        sys.exit(1)

    try:
        result = transcribe(video_path, model_dir, language, preset, max_words)
        print(json.dumps(result, ensure_ascii=False))
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)
