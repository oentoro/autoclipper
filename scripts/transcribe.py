#!/usr/bin/env python3
import sys
import json
import os

def seconds_to_srt_time(seconds: float) -> str:
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = int(seconds % 60)
    millis = int((seconds - int(seconds)) * 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{millis:03d}"

def transcribe(video_path: str, model_dir: str | None = None) -> dict:
    from faster_whisper import WhisperModel

    kwargs = {"device": "cpu", "compute_type": "int8"}
    if model_dir and os.path.isdir(model_dir):
        kwargs["download_root"] = model_dir

    model = WhisperModel("small", **kwargs)

    segments_raw, info = model.transcribe(
        video_path,
        language="id",
        beam_size=5,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=500),
    )

    segments = []
    srt_lines = []
    idx = 1

    for seg in segments_raw:
        start_time = seconds_to_srt_time(seg.start)
        end_time = seconds_to_srt_time(seg.end)
        text = seg.text.strip()

        if not text:
            continue

        segments.append({
            "index": idx,
            "start": seg.start,
            "end": seg.end,
            "text": text,
            "start_time": start_time,
            "end_time": end_time,
        })

        srt_lines.append(str(idx))
        srt_lines.append(f"{start_time} --> {end_time}")
        srt_lines.append(text)
        srt_lines.append("")

        idx += 1

    return {
        "segments": segments,
        "srt_content": "\n".join(srt_lines),
    }

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: transcribe.py <video_path> [--model-dir <dir>]"}))
        sys.exit(1)

    video_path = sys.argv[1]

    model_dir = None
    if "--model-dir" in sys.argv:
        idx = sys.argv.index("--model-dir")
        if idx + 1 < len(sys.argv):
            model_dir = sys.argv[idx + 1]

    if not os.path.exists(video_path):
        print(json.dumps({"error": f"File tidak ditemukan: {video_path}"}))
        sys.exit(1)

    try:
        result = transcribe(video_path, model_dir)
        print(json.dumps(result, ensure_ascii=False))
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)
