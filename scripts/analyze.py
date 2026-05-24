#!/usr/bin/env python3
"""
LLM inference for AutoClipper.
Backends (in priority order):
  1. llama-cpp-python  — if --model-path points to a .gguf file
  2. Ollama HTTP API   — fallback (requires Ollama running)
"""
import sys
import json
import os
import argparse


# ── Prompt builders ────────────────────────────────────────────────────────────

def build_analyze_prompt(segments: list) -> str:
    text = "\n".join(f"[{s['index']}] {s['start_time']}: {s['text']}" for s in segments)
    return (
        f"Transkrip video:\n\n{text}\n\n"
        "Pilih segmen-segmen paling penting dari video ini. "
        "Balas HANYA dengan JSON (tanpa teks lain):\n"
        '{"important_indices": [1, 3, 5], "reasoning": "alasan singkat"}'
    )


def build_classify_prompt(segments: list) -> str:
    text  = "\n".join(f"[{s['index']}] {s['start_time']}: {s['text']}" for s in segments)
    first = segments[0]["index"]
    last  = segments[-1]["index"]
    mins  = int(segments[-1]["end"] / 60)
    return (
        f"Transkrip video (~{mins} menit, segmen {first}–{last}):\n\n{text}\n\n"
        "Bagi menjadi 3–7 bagian berurutan berdasarkan topik. "
        "Balas HANYA dengan JSON (tanpa teks lain):\n"
        f'{{"sections":['
        f'{{"name":"Pembukaan","summary":"Ringkasan satu kalimat.","start_index":{first},"end_index":10}},'
        f'{{"name":"Isi Utama","summary":"Ringkasan satu kalimat.","start_index":11,"end_index":{last}}}'
        f']}}\n'
        "Aturan: bagian berurutan, mencakup semua segmen, nama max 4 kata."
    )


# ── Backends ───────────────────────────────────────────────────────────────────

def infer_llamacpp(model_path: str, prompt: str) -> str:
    from llama_cpp import Llama  # type: ignore
    llm = Llama(
        model_path=model_path,
        n_ctx=32768,
        n_gpu_layers=-1,   # use all available GPU layers (Metal on Mac, CUDA on NVIDIA)
        verbose=False,
    )
    out = llm.create_chat_completion(
        messages=[{"role": "user", "content": prompt}],
        response_format={"type": "json_object"},
        temperature=0.1,
        max_tokens=2048,
    )
    return out["choices"][0]["message"]["content"]


def infer_ollama(base_url: str, model_name: str, prompt: str) -> str:
    import urllib.request
    body = json.dumps({
        "model": model_name,
        "messages": [{"role": "user", "content": prompt}],
        "stream": False,
        "format": "json",
    }).encode()
    req = urllib.request.Request(
        f"{base_url}/api/chat",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=180) as resp:
        return json.loads(resp.read())["message"]["content"]


# ── Entry point ────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("task", choices=["analyze", "classify"])
    parser.add_argument("--segments-file", required=True, help="Path to JSON file with segments")
    parser.add_argument("--model-path",    default="",   help="Path to local .gguf model file")
    parser.add_argument("--ollama-url",    default="http://localhost:11434")
    parser.add_argument("--ollama-model",  default="gemma4:latest")
    args = parser.parse_args()

    with open(args.segments_file, encoding="utf-8") as f:
        segments = json.load(f)

    if not segments:
        print(json.dumps({"error": "Tidak ada segmen"}), file=sys.stderr)
        sys.exit(1)

    prompt = build_analyze_prompt(segments) if args.task == "analyze" \
             else build_classify_prompt(segments)

    try:
        if args.model_path and os.path.isfile(args.model_path):
            try:
                result = infer_llamacpp(args.model_path, prompt)
            except ImportError:
                # llama-cpp-python not installed — fall back to Ollama
                print("[analyze.py] llama-cpp-python tidak ditemukan, mencoba Ollama...", file=sys.stderr)
                result = infer_ollama(args.ollama_url, args.ollama_model, prompt)
        else:
            result = infer_ollama(args.ollama_url, args.ollama_model, prompt)

        print(result)
    except Exception as e:
        msg = str(e)
        if "llama_cpp" in msg or "llama-cpp" in msg:
            hint = (
                "llama-cpp-python belum terinstall. "
                "Jalankan: pip install llama-cpp-python"
            )
            print(json.dumps({"error": hint}), file=sys.stderr)
        elif "Connection refused" in msg or "URLError" in msg or "RemoteDisconnected" in msg:
            print(json.dumps({"error":
                "Tidak bisa terhubung ke Ollama dan llama-cpp-python tidak tersedia. "
                "Pilihan: (1) install llama-cpp-python: pip install llama-cpp-python  "
                "atau (2) jalankan Ollama: ollama serve"
            }), file=sys.stderr)
        else:
            print(json.dumps({"error": msg}), file=sys.stderr)
        sys.exit(1)
