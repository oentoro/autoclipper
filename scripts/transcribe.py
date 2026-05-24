#!/usr/bin/env python3
import sys
import json
import os

PRESETS = {
    "fast":     {"model": "tiny",            "beam_size": 1, "batch": True},
    "balanced": {"model": "base",            "beam_size": 2, "batch": True},
    "accurate": {"model": "medium",          "beam_size": 5, "batch": False},
    "best":     {"model": "large-v3-turbo",  "beam_size": 5, "batch": False},
}

def seconds_to_srt_time(seconds: float) -> str:
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = int(seconds % 60)
    millis = int((seconds - int(seconds)) * 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{millis:03d}"

def split_segment_by_words(seg, max_words: int) -> list[dict]:
    """
    Split a Whisper segment into chunks of at most max_words words.
    Requires the segment to have .words (word-level timestamps).
    Falls back to the whole segment if words are unavailable.
    """
    words = getattr(seg, "words", None)
    text = seg.text.strip()

    if not words:
        # No word timestamps — linear interpolation fallback
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

    # Use accurate word timestamps
    chunks = []
    for i in range(0, len(words), max_words):
        chunk = words[i:i + max_words]
        chunk_text = "".join(w.word for w in chunk).strip()
        if chunk_text:
            chunks.append({"text": chunk_text, "start": chunk[0].start, "end": chunk[-1].end})
    return chunks

def build_output(raw_chunks: list[dict]) -> dict:
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

def transcribe(video_path: str, model_dir: str | None = None,
               language: str | None = None, preset: str = "balanced",
               max_words: int = 0) -> dict:
    from faster_whisper import WhisperModel

    cfg = PRESETS.get(preset, PRESETS["balanced"])
    model_name = cfg["model"]
    beam_size  = cfg["beam_size"]
    use_batch  = cfg["batch"]

    kwargs = {"device": "cpu", "compute_type": "int8"}
    if model_dir and os.path.isdir(model_dir):
        kwargs["download_root"] = model_dir

    model = WhisperModel(model_name, **kwargs)

    need_words = max_words > 0
    transcribe_kwargs = dict(
        language=language or None,
        beam_size=beam_size,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=500),
        word_timestamps=need_words,
    )

    segments_raw = None
    info = None

    if use_batch:
        try:
            from faster_whisper import BatchedInferencePipeline
            batched = BatchedInferencePipeline(model=model)
            segments_raw, info = batched.transcribe(video_path, batch_size=16, **transcribe_kwargs)
        except Exception:
            segments_raw, info = model.transcribe(video_path, **transcribe_kwargs)
    else:
        segments_raw, info = model.transcribe(video_path, **transcribe_kwargs)

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
        "segments": segments,
        "srt_content": srt_content,
        "detected_language": info.language or "unknown",
    }

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
