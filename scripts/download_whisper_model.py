#!/usr/bin/env python3
"""Download a Whisper model to the HuggingFace cache with progress reporting.

Usage:
  download_whisper_model.py --model <tiny|base|medium|large-v3-turbo>
                            --backend <mlx|faster-whisper>
                            [--cache-dir <dir>]

Progress is emitted as PROGRESS:<0-100> lines to stderr.
Result (success or error) is emitted as JSON to stdout.
"""
import sys
import os
import json
import threading

# Quantized (q4) repos — preferred for M-series: ~40% size, faster inference
MLX_REPOS = {
    "tiny":           "mlx-community/whisper-tiny-mlx",
    "base":           "mlx-community/whisper-base-mlx",
    "medium":         "mlx-community/whisper-medium-mlx-q4",
    "large-v3-turbo": "mlx-community/whisper-large-v3-turbo-q4",
}

# Float16 fallback — used if q4 repo is unavailable on HuggingFace Hub
MLX_REPOS_F16 = {
    "tiny":           "mlx-community/whisper-tiny-mlx",
    "base":           "mlx-community/whisper-base-mlx",
    "medium":         "mlx-community/whisper-medium-mlx",
    "large-v3-turbo": "mlx-community/whisper-large-v3-turbo",
}

FW_REPOS = {
    "tiny":           "Systran/faster-whisper-tiny",
    "base":           "Systran/faster-whisper-base",
    "small":          "Systran/faster-whisper-small",
    "medium":         "Systran/faster-whisper-medium",
    "large-v3-turbo": "mobiuslabsgmbh/faster-whisper-large-v3-turbo",
}

# Approximate download sizes in bytes (weights only)
SIZE_ESTIMATES = {
    "tiny":           40_000_000,
    "base":           75_000_000,
    "small":         250_000_000,
    "medium":        320_000_000,   # q4: ~320MB vs 800MB float16
    "large-v3-turbo": 640_000_000,  # q4: ~640MB vs 1.6GB float16
}

SIZE_ESTIMATES_F16 = {
    "medium":        800_000_000,
    "large-v3-turbo": 1_600_000_000,
}

def emit_progress(pct: int) -> None:
    try:
        os.write(2, f"PROGRESS:{min(100, max(0, pct))}\n".encode("ascii"))
    except OSError:
        pass

def blobs_size(cache_dir: str, repo_id: str) -> int:
    """Return total bytes in the blobs/ dir for this repo."""
    cache_name = "models--" + repo_id.replace("/", "--")
    blobs_path = os.path.join(cache_dir, cache_name, "blobs")
    if not os.path.isdir(blobs_path):
        return 0
    total = 0
    for fname in os.listdir(blobs_path):
        fpath = os.path.join(blobs_path, fname)
        try:
            total += os.path.getsize(fpath)
        except OSError:
            pass
    return total

def download_repo(repo_id: str, cache_dir: str, model_id: str) -> str:
    from huggingface_hub import snapshot_download

    expected = SIZE_ESTIMATES.get(model_id, 500_000_000)
    result: list = [None, None]
    done = threading.Event()

    def worker():
        try:
            result[0] = snapshot_download(
                repo_id=repo_id,
                cache_dir=cache_dir,
                ignore_patterns=["*.msgpack", "*.h5", "flax_model*", "tf_model*", "rust_model*"],
            )
        except Exception as exc:
            result[1] = exc
        finally:
            done.set()

    t = threading.Thread(target=worker, daemon=True)
    t.start()
    emit_progress(0)

    while not done.wait(timeout=0.8):
        current = blobs_size(cache_dir, repo_id)
        pct = min(95, int(current / expected * 100)) if expected > 0 else 50
        emit_progress(pct)

    t.join()
    if result[1] is not None:
        raise result[1]

    emit_progress(100)
    return result[0]

if __name__ == "__main__":
    def get_arg(flag: str) -> str | None:
        if flag in sys.argv:
            i = sys.argv.index(flag)
            if i + 1 < len(sys.argv):
                return sys.argv[i + 1]
        return None

    model_id = get_arg("--model") or "base"
    backend  = get_arg("--backend") or "mlx"
    cache_dir = get_arg("--cache-dir") or os.path.expanduser("~/.cache/huggingface/hub")

    os.makedirs(cache_dir, exist_ok=True)

    try:
        if backend == "mlx":
            repo = MLX_REPOS.get(model_id)
            if not repo:
                raise ValueError(f"Unknown MLX model: {model_id}")
            try:
                path = download_repo(repo, cache_dir, model_id)
            except Exception as q4_err:
                # q4 repo not available on Hub — fall back to float16
                repo_f16 = MLX_REPOS_F16.get(model_id)
                if repo_f16 and repo_f16 != repo:
                    import sys as _sys
                    _sys.stderr.write(f"[download] q4 tidak tersedia ({q4_err}), unduh float16: {repo_f16}\n")
                    # Adjust size estimate for float16
                    global SIZE_ESTIMATES
                    SIZE_ESTIMATES = {**SIZE_ESTIMATES, **SIZE_ESTIMATES_F16}
                    path = download_repo(repo_f16, cache_dir, model_id)
                else:
                    raise
        else:
            repo = FW_REPOS.get(model_id)
            if not repo:
                raise ValueError(f"Unknown faster-whisper model: {model_id}")
            path = download_repo(repo, cache_dir, model_id)

        os.write(1, (json.dumps({"success": True, "path": path}) + "\n").encode("utf-8"))
    except Exception as e:
        os.write(1, (json.dumps({"error": str(e)}) + "\n").encode("utf-8"))
        sys.exit(1)
