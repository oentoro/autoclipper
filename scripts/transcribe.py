#!/usr/bin/env python3
import sys
import json
import os
import platform
import subprocess
import tempfile

PRESETS = {
    "fast":     {"model": "tiny",           "beam_size": 1, "batch": True,  "condition_prev": False},
    "balanced": {"model": "base",           "beam_size": 2, "batch": True,  "condition_prev": False},
    "accurate": {"model": "medium",         "beam_size": 5, "batch": False, "condition_prev": True},
    "best":     {"model": "large-v3-turbo", "beam_size": 5, "batch": False, "condition_prev": True},
}

# MLX model repos for Apple Silicon — quantized (q4) preferred: faster inference,
# ~40% of float16 size, negligible accuracy difference on M-series GPU.
MLX_REPOS = {
    "tiny":           "mlx-community/whisper-tiny-mlx",
    "base":           "mlx-community/whisper-base-mlx",
    "medium":         "mlx-community/whisper-medium-mlx-q4",
    "large-v3-turbo": "mlx-community/whisper-large-v3-turbo-q4",
}

# Float16 fallback — used when q4 repo is not cached (e.g. older downloads)
MLX_REPOS_F16 = {
    "tiny":           "mlx-community/whisper-tiny-mlx",
    "base":           "mlx-community/whisper-base-mlx",
    "medium":         "mlx-community/whisper-medium-mlx",
    "large-v3-turbo": "mlx-community/whisper-large-v3-turbo",
}

# Size-ascending order (smallest → largest) — used to build smart fallback
MLX_SIZE_ORDER = ["tiny", "base", "medium", "large-v3-turbo"]

_WEIGHT_EXTS = ('.safetensors', '.npz', '.bin', '.pt')

def _check_repo_cached(cache_dir: str, repo: str) -> str | None:
    """Return snapshot path if repo is completely cached, else None."""
    cache_name = "models--" + repo.replace("/", "--")
    snaps_dir = os.path.join(cache_dir, cache_name, "snapshots")
    if not os.path.isdir(snaps_dir):
        return None
    for snap_hash in os.listdir(snaps_dir):
        snap_dir = os.path.join(snaps_dir, snap_hash)
        if not os.path.isdir(snap_dir):
            continue
        try:
            files = os.listdir(snap_dir)
        except OSError:
            continue
        has_weights = any(
            os.path.exists(os.path.realpath(os.path.join(snap_dir, f)))
            and f.endswith(_WEIGHT_EXTS)
            for f in files
        )
        if has_weights:
            return snap_dir
    return None

def find_best_cached_mlx_path(preferred_model: str) -> tuple[str | None, str | None]:
    """Return (local_snapshot_path, model_id) for the best complete cached MLX model.

    Fallback order: preferred first, then ascending in size (next larger), then
    descending (smaller). This ensures e.g. 'tiny' falls to 'base' not 'large-v3-turbo'.
    Checks q4 quantized repos before float16 for each candidate.
    Returns (None, None) if no complete model is found.
    """
    cache_dir = os.path.expanduser("~/.cache/huggingface/hub")

    pref_idx = MLX_SIZE_ORDER.index(preferred_model) \
        if preferred_model in MLX_SIZE_ORDER else len(MLX_SIZE_ORDER)
    larger  = MLX_SIZE_ORDER[pref_idx + 1:]          # next larger models
    smaller = MLX_SIZE_ORDER[:pref_idx][::-1]         # smaller models, largest-first
    order   = [preferred_model] + larger + smaller

    for model in order:
        for repo_map in (MLX_REPOS, MLX_REPOS_F16):
            repo = repo_map.get(model, f"mlx-community/whisper-{model}-mlx")
            snap = _check_repo_cached(cache_dir, repo)
            if snap:
                return snap, model
    return None, None

def seconds_to_srt_time(seconds: float) -> str:
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = int(seconds % 60)
    millis = int((seconds - int(seconds)) * 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{millis:03d}"

def is_apple_silicon() -> bool:
    return sys.platform == "darwin" and platform.machine() == "arm64"

# ── Wrappers agar split_segment_by_words bisa handle kedua backend ──────────

class _Word:
    __slots__ = ("word", "start", "end")
    def __init__(self, w):
        if isinstance(w, dict):
            self.word, self.start, self.end = w["word"], w["start"], w["end"]
        else:
            self.word, self.start, self.end = w.word, w.start, w.end

class _Seg:
    __slots__ = ("text", "start", "end", "words")
    def __init__(self, s):
        if isinstance(s, dict):
            self.text  = s["text"]
            self.start = s["start"]
            self.end   = s["end"]
            self.words = [_Word(w) for w in s.get("words") or []] or None
        else:
            self.text  = s.text
            self.start = s.start
            self.end   = s.end
            ws = getattr(s, "words", None)
            self.words = [_Word(w) for w in ws] if ws else None

# ─────────────────────────────────────────────────────────────────────────────

def split_segment_by_words(seg, max_words: int) -> list[dict]:
    text = seg.text.strip()
    words = seg.words

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

def _filter_hallucinations(segments: list, no_speech_threshold: float = 0.6) -> list:
    """Drop silent/hallucinated segments from Whisper output.

    Two passes:
    1. no_speech_prob above threshold → segment is noise/silence, skip.
    2. Text identical to previous kept segment → repetition hallucination, skip.
    Works with both dict (MLX) and object (faster-whisper) segments.
    """
    filtered = []
    prev_text = ""
    for seg in segments:
        nsp = seg.get("no_speech_prob") if isinstance(seg, dict) else getattr(seg, "no_speech_prob", 0.0)
        if nsp is not None and nsp > no_speech_threshold:
            continue
        text = (seg.get("text") if isinstance(seg, dict) else getattr(seg, "text", "")).strip()
        if text and text == prev_text:
            continue
        filtered.append(seg)
        if text:
            prev_text = text
    return filtered


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
    try:
        import ctranslate2
        if ctranslate2.get_cuda_device_count() > 0:
            return "cuda", "float16"
    except Exception:
        pass
    return "cpu", "int8"

def _should_use_directml() -> bool:
    """True jika Windows, tidak ada CUDA, dan ada GPU lewat DirectML (AMD/Intel/NVIDIA)."""
    if sys.platform != "win32":
        return False
    try:
        import ctranslate2
        if ctranslate2.get_cuda_device_count() > 0:
            return False  # CUDA tersedia — faster-whisper+CUDA lebih cepat
    except Exception:
        pass
    try:
        import torch_directml
        return torch_directml.device_count() > 0
    except Exception:
        return False

def get_cpu_threads() -> int:
    cpu_count = os.cpu_count() or 4
    return max(4, cpu_count - 1)

def extract_audio(video_path: str, ffmpeg: str) -> str | None:
    """Pre-extract 16kHz mono WAV — skip video decode overhead."""
    tmp = tempfile.NamedTemporaryFile(suffix=".wav", delete=False)
    tmp.close()
    try:
        result = subprocess.run(
            [ffmpeg, "-y", "-i", video_path,
             "-vn", "-ar", "16000", "-ac", "1", "-sample_fmt", "s16",
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

def get_wav_duration(path: str) -> float:
    """Return WAV duration in seconds."""
    try:
        import wave as _wave
        with _wave.open(path, "rb") as f:
            return f.getnframes() / float(f.getframerate())
    except Exception:
        return 0.0

def emit_progress(pct: int) -> None:
    """Write a PROGRESS line to stderr — picked up by the Rust layer."""
    try:
        os.write(2, f"PROGRESS:{min(100, max(0, pct))}\n".encode("ascii"))
    except OSError:
        pass

# ── MLX backend (Apple Silicon) ──────────────────────────────────────────────

def _transcribe_mlx(audio_path: str, model_name: str, language: str | None,
                    beam_size: int, condition_prev: bool,
                    word_timestamps: bool, model_dir: str | None,
                    total_duration: float = 0.0) -> tuple[list, str]:
    import mlx_whisper, threading, time

    local_path, actual_model = find_best_cached_mlx_path(model_name)
    if local_path is None:
        raise RuntimeError("No complete cached MLX model found — use faster-whisper instead")
    if actual_model != model_name:
        print(f"[transcribe] MLX model '{model_name}' tidak lengkap/tidak ada, pakai '{actual_model}'", file=sys.stderr)

    result_box: list = [None, None]  # [result, exception]
    done = threading.Event()

    def worker():
        import contextlib
        # Redirect mlx-whisper's stdout to stderr so "Detected language: ..."
        # lines don't pollute our JSON output on stdout.
        with contextlib.redirect_stdout(sys.stderr):
            try:
                # Try preset beam_size (mlx-whisper ≥0.5 supports BeamSearchDecoder).
                # beam_size=1 is equivalent to greedy, pass None for that case.
                eff_beam = beam_size if beam_size > 1 else None
                result_box[0] = mlx_whisper.transcribe(
                    audio_path,
                    path_or_hf_repo=local_path,
                    language=language or None,
                    word_timestamps=word_timestamps,
                    condition_on_previous_text=condition_prev,
                    beam_size=eff_beam,
                    temperature=0.0,
                    verbose=False,
                )
            except (NotImplementedError, TypeError):
                # Older mlx-whisper: BeamSearchDecoder not implemented — greedy only.
                print("[transcribe] beam_size tidak didukung mlx-whisper ini, pakai greedy", file=sys.stderr)
                result_box[0] = mlx_whisper.transcribe(
                    audio_path,
                    path_or_hf_repo=local_path,
                    language=language or None,
                    word_timestamps=word_timestamps,
                    condition_on_previous_text=condition_prev,
                    beam_size=None,
                    temperature=0.0,
                    verbose=False,
                )
            except Exception as exc:
                result_box[1] = exc
        done.set()

    t = threading.Thread(target=worker, daemon=True)
    t.start()

    # Emit time-based progress while mlx runs in background thread.
    # Use 6× realtime as conservative floor so estimation stays under actual
    # completion time even for complex/long audio.
    estimated_secs = max(1.0, total_duration / 6) if total_duration > 0 else 300.0
    start = time.monotonic()
    while not done.wait(timeout=0.8):
        elapsed = time.monotonic() - start
        # Hard-cap at 95 — 100% is reserved for after build_output
        pct = min(95, int(elapsed / estimated_secs * 95))
        emit_progress(pct)

    t.join()

    if result_box[1] is not None:
        raise result_box[1]

    r = result_box[0]
    segs = _filter_hallucinations(r["segments"])
    return segs, r.get("language") or "unknown"

# ── faster-whisper backend (CUDA / CPU) ──────────────────────────────────────

def _transcribe_faster_whisper(audio_path: str, model_name: str, language: str | None,
                                beam_size: int, use_batch: bool, condition_prev: bool,
                                word_timestamps: bool, model_dir: str | None,
                                total_duration: float = 0.0) -> tuple[list, str]:
    from faster_whisper import WhisperModel

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

    transcribe_kwargs = dict(
        language=language or None,
        beam_size=beam_size,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=500),
        word_timestamps=word_timestamps,
        condition_on_previous_text=condition_prev,
    )

    if use_batch:
        try:
            from faster_whisper import BatchedInferencePipeline
            batched = BatchedInferencePipeline(model=model)
            segs_raw, info = batched.transcribe(audio_path, batch_size=16, **transcribe_kwargs)
        except Exception:
            segs_raw, info = model.transcribe(audio_path, **transcribe_kwargs)
    else:
        segs_raw, info = model.transcribe(audio_path, **transcribe_kwargs)

    # Consume generator while emitting real segment-based progress.
    # Cap at 98 — 99-100 reserved for build_output + JSON serialization.
    segments_out = []
    for seg in segs_raw:
        segments_out.append(seg)
        if total_duration > 0:
            emit_progress(min(98, int(seg.end / total_duration * 98)))
        else:
            emit_progress(50)  # no duration info — show indeterminate midpoint

    segments_out = _filter_hallucinations(segments_out)
    return segments_out, info.language or "unknown"

# ── DirectML backend (Windows AMD / Intel / NVIDIA tanpa CUDA) ───────────────

def _transcribe_directml(audio_path: str, model_name: str, language: str | None,
                          beam_size: int, condition_prev: bool,
                          word_timestamps: bool,
                          total_duration: float = 0.0) -> tuple[list, str]:
    import whisper, torch_directml, threading, time

    dml = torch_directml.device()
    result_box: list = [None, None]
    done = threading.Event()

    def worker():
        try:
            model = whisper.load_model(model_name, device=dml)
        except Exception:
            # 'large-v3-turbo' dikenal sebagai 'turbo' di openai-whisper lama
            alt = {"large-v3-turbo": "turbo"}.get(model_name, model_name)
            model = whisper.load_model(alt, device=dml)
        try:
            result_box[0] = model.transcribe(
                audio_path,
                language=language or None,
                beam_size=beam_size if beam_size > 1 else None,
                condition_on_previous_text=condition_prev,
                word_timestamps=word_timestamps,
                verbose=False,
            )
        except Exception as exc:
            result_box[1] = exc
        done.set()

    t = threading.Thread(target=worker, daemon=True)
    t.start()

    estimated_secs = max(1.0, total_duration / 4) if total_duration > 0 else 300.0
    start = time.monotonic()
    while not done.wait(timeout=0.8):
        elapsed = time.monotonic() - start
        emit_progress(min(95, int(elapsed / estimated_secs * 95)))

    t.join()
    if result_box[1] is not None:
        raise result_box[1]

    r = result_box[0]
    segs = _filter_hallucinations(r["segments"])
    return segs, r.get("language") or "unknown"

# ─────────────────────────────────────────────────────────────────────────────

def transcribe(video_path: str, model_dir: str | None = None,
               language: str | None = None, preset: str = "balanced",
               max_words: int = 0) -> dict:

    cfg = PRESETS.get(preset, PRESETS["balanced"])
    model_name     = cfg["model"]
    beam_size      = cfg["beam_size"]
    use_batch      = cfg["batch"]
    condition_prev = cfg["condition_prev"]
    need_words     = max_words > 0

    ffmpeg = os.environ.get("AUTOCLIPPER_FFMPEG", "ffmpeg")
    audio_path = extract_audio(video_path, ffmpeg)
    input_path = audio_path if audio_path else video_path
    total_duration = get_wav_duration(input_path) if audio_path else 0.0

    try:
        use_mlx = is_apple_silicon()
        if use_mlx:
            try:
                print(f"[transcribe] Backend: mlx-whisper (Apple Silicon GPU) — model: {model_name}", file=sys.stderr)
                segs_raw, detected_lang = _transcribe_mlx(
                    input_path, model_name, language,
                    beam_size, condition_prev, need_words, model_dir,
                    total_duration=total_duration,
                )
                segs_wrapped = [_Seg(s) for s in segs_raw]
            except Exception as e:
                print(f"[transcribe] mlx-whisper gagal ({e}), fallback ke faster-whisper", file=sys.stderr)
                use_mlx = False

        use_directml = False
        if not use_mlx and _should_use_directml():
            try:
                print(f"[transcribe] Backend: openai-whisper + DirectML (AMD/Intel GPU) — model: {model_name}", file=sys.stderr)
                segs_raw, detected_lang = _transcribe_directml(
                    input_path, model_name, language,
                    beam_size, condition_prev, need_words,
                    total_duration=total_duration,
                )
                segs_wrapped = [_Seg(s) for s in segs_raw]
                use_directml = True
            except Exception as e:
                print(f"[transcribe] DirectML gagal ({e}), fallback ke faster-whisper", file=sys.stderr)

        if not use_mlx and not use_directml:
            device, _ = detect_device()
            backend_label = "CUDA" if device == "cuda" else "CPU"
            print(f"[transcribe] Backend: faster-whisper ({backend_label}) — model: {model_name}", file=sys.stderr)
            segs_raw, detected_lang = _transcribe_faster_whisper(
                input_path, model_name, language,
                beam_size, use_batch, condition_prev, need_words, model_dir,
                total_duration=total_duration,
            )
            segs_wrapped = [_Seg(s) for s in segs_raw]

        sentence_chunks = []
        word_chunks     = []
        for seg in segs_wrapped:
            if not seg.text.strip():
                continue
            sentence_chunks.append({"text": seg.text.strip(), "start": seg.start, "end": seg.end})
            if need_words:
                word_chunks.extend(split_segment_by_words(seg, max_words))

        if need_words:
            segments, srt_content = build_output(word_chunks)
            raw_segments, _       = build_output(sentence_chunks)
        else:
            segments, srt_content = build_output(sentence_chunks)
            raw_segments          = segments  # identical when no splitting

        # Emit 100% only after everything is built — right before printing JSON
        emit_progress(100)

        return {
            "segments":          segments,
            "srt_content":       srt_content,
            "detected_language": detected_lang,
            "raw_segments":      raw_segments,
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
        os.write(1, (json.dumps(result, ensure_ascii=False) + "\n").encode("utf-8"))
    except Exception as e:
        import traceback
        err_json = json.dumps({"error": str(e), "traceback": traceback.format_exc()})
        os.write(1, (err_json + "\n").encode("utf-8"))
        sys.exit(1)
