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

def transcribe(video_path: str, model_dir: str | None = None,
               language: str | None = None, preset: str = "balanced") -> dict:
    from faster_whisper import WhisperModel

    cfg = PRESETS.get(preset, PRESETS["balanced"])
    model_name = cfg["model"]
    beam_size  = cfg["beam_size"]
    use_batch  = cfg["batch"]

    kwargs = {"device": "cpu", "compute_type": "int8"}
    if model_dir and os.path.isdir(model_dir):
        kwargs["download_root"] = model_dir

    model = WhisperModel(model_name, **kwargs)

    transcribe_kwargs = dict(
        language=language or None,
        beam_size=beam_size,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=500),
    )

    if use_batch:
        try:
            from faster_whisper import BatchedInferencePipeline
            batched = BatchedInferencePipeline(model=model)
            segments_raw, info = batched.transcribe(video_path, batch_size=16, **transcribe_kwargs)
        except Exception:
            # Fallback to standard pipeline if batched fails
            segments_raw, info = model.transcribe(video_path, **transcribe_kwargs)
    else:
        segments_raw, info = model.transcribe(video_path, **transcribe_kwargs)

    segments = []
    srt_lines = []
    idx = 1

    for seg in segments_raw:
        start_time = seconds_to_srt_time(seg.start)
        end_time   = seconds_to_srt_time(seg.end)
        text       = seg.text.strip()
        if not text:
            continue

        segments.append({
            "index": idx,
            "start": seg.start,
            "end":   seg.end,
            "text":  text,
            "start_time": start_time,
            "end_time":   end_time,
        })
        srt_lines += [str(idx), f"{start_time} --> {end_time}", text, ""]
        idx += 1

    return {
        "segments": segments,
        "srt_content": "\n".join(srt_lines),
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

    if not os.path.exists(video_path):
        print(json.dumps({"error": f"File tidak ditemukan: {video_path}"}))
        sys.exit(1)

    try:
        result = transcribe(video_path, model_dir, language, preset)
        print(json.dumps(result, ensure_ascii=False))
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)
