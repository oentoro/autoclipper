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

def transcribe(video_path: str) -> dict:
    from faster_whisper import WhisperModel

    model = WhisperModel("small", device="cpu", compute_type="int8")

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

    srt_content = "\n".join(srt_lines)

    return {
        "segments": segments,
        "srt_content": srt_content,
    }

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: transcribe.py <video_path>"}))
        sys.exit(1)

    video_path = sys.argv[1]

    if not os.path.exists(video_path):
        print(json.dumps({"error": f"File tidak ditemukan: {video_path}"}))
        sys.exit(1)

    try:
        result = transcribe(video_path)
        print(json.dumps(result, ensure_ascii=False))
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)
