#!/usr/bin/env python3
"""
Download llama-server binary (+ required dylibs) from the latest llama.cpp release.
Places all files in src-tauri/vendor/bin/ so llama-server can find its libraries
via @loader_path rpath.

Usage:
    python3 scripts/download_llama_server.py
"""
import urllib.request
import json
import platform
import zipfile
import os
import stat
import shutil
import sys

DEST_DIR = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..", "src-tauri", "vendor", "bin"
)

def progress(count, block_size, total_size):
    if total_size > 0:
        pct = min(count * block_size / total_size * 100, 100)
        mb  = min(count * block_size, total_size) / 1024 / 1024
        tot = total_size / 1024 / 1024
        print(f"\r  {pct:.0f}%  {mb:.1f} / {tot:.1f} MB", end="", flush=True)


def main():
    system  = platform.system().lower()
    machine = platform.machine().lower()

    print("Mengambil informasi rilis llama.cpp terbaru...")
    req = urllib.request.Request(
        "https://api.github.com/repos/ggerganov/llama.cpp/releases/latest",
        headers={"User-Agent": "autoclipper-setup"}
    )
    with urllib.request.urlopen(req) as r:
        data = json.loads(r.read())

    tag = data["tag_name"]
    print(f"Versi terbaru: {tag}")

    all_assets = {a["name"]: a["browser_download_url"] for a in data.get("assets", [])}

    # Build candidate asset names in priority order
    if system == "darwin":
        arch_str = "arm64" if machine == "arm64" else "x64"
        candidates = [
            f"llama-{tag}-bin-macos-{arch_str}.tar.gz",
            f"llama-{tag}-bin-macos-{arch_str}.zip",
        ]
        bin_name = "llama-server"
    elif system == "linux":
        candidates = [
            f"llama-{tag}-bin-ubuntu-x64.tar.gz",
            f"llama-{tag}-bin-ubuntu-x64.zip",
        ]
        bin_name = "llama-server"
    elif system == "windows":
        candidates = [
            f"llama-{tag}-bin-win-cpu-x64.zip",
            f"llama-{tag}-bin-win-avx2-x64.zip",
        ]
        bin_name = "llama-server.exe"
    else:
        print(f"Platform tidak didukung: {system}", file=sys.stderr)
        sys.exit(1)

    asset_pat, url = None, None
    for c in candidates:
        if c in all_assets:
            asset_pat, url = c, all_assets[c]
            break

    if not url:
        print(f"\nTidak menemukan asset untuk platform ini di rilis {tag}.")
        print("Asset yang tersedia:")
        for name in sorted(all_assets):
            print(f"  {name}")
        print("\nDownload manual: https://github.com/ggerganov/llama.cpp/releases")
        sys.exit(1)

    # Download
    ext     = ".tar.gz" if asset_pat.endswith(".tar.gz") else ".zip"
    tmp_arc = os.path.join(os.path.dirname(os.path.abspath(__file__)), f"_llama_tmp_{tag}{ext}")
    print(f"Mengunduh {asset_pat}...")
    try:
        urllib.request.urlretrieve(url, tmp_arc, progress)
        print()
    except Exception as e:
        print(f"\nGagal mengunduh: {e}", file=sys.stderr)
        if os.path.exists(tmp_arc): os.remove(tmp_arc)
        sys.exit(1)

    # Extract all files (binary + dylibs) to DEST_DIR
    # The archive has a top-level directory (e.g. llama-b9283/) — we strip it.
    print(f"Mengekstrak semua file ke {DEST_DIR}...")
    os.makedirs(DEST_DIR, exist_ok=True)

    extracted_count = 0
    if ext == ".tar.gz":
        import tarfile
        with tarfile.open(tmp_arc, "r:gz") as tf:
            for member in tf.getmembers():
                parts = member.name.split("/", 1)
                bare  = parts[1] if len(parts) == 2 else parts[0]
                if not bare:
                    continue
                dest_path = os.path.join(DEST_DIR, bare)
                if member.issym():
                    # Recreate symlink (e.g. libfoo.0.dylib -> libfoo.0.12.0.dylib)
                    if os.path.exists(dest_path) or os.path.islink(dest_path):
                        os.remove(dest_path)
                    os.symlink(member.linkname, dest_path)
                elif member.isfile():
                    with tf.extractfile(member) as src, open(dest_path, "wb") as dst:
                        shutil.copyfileobj(src, dst)
                    if member.mode & 0o111:
                        os.chmod(dest_path, os.stat(dest_path).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
                else:
                    continue
                extracted_count += 1
    else:
        with zipfile.ZipFile(tmp_arc) as zf:
            for name in zf.namelist():
                if name.endswith("/"):
                    continue
                parts = name.split("/", 1)
                bare  = parts[1] if len(parts) == 2 else parts[0]
                if not bare:
                    continue
                dest_path = os.path.join(DEST_DIR, bare)
                with zf.open(name) as src, open(dest_path, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                extracted_count += 1
            # Set executable bit on Unix (Windows zip may not carry it)
            if system != "windows":
                server_path = os.path.join(DEST_DIR, bin_name)
                if os.path.exists(server_path):
                    os.chmod(server_path, os.stat(server_path).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    os.remove(tmp_arc)

    dest = os.path.join(DEST_DIR, bin_name)
    if not os.path.exists(dest):
        print(f"\n{bin_name} tidak ditemukan setelah ekstraksi.", file=sys.stderr)
        sys.exit(1)

    size_mb = os.path.getsize(dest) / 1024 / 1024
    total_mb = sum(os.path.getsize(os.path.join(DEST_DIR, f)) for f in os.listdir(DEST_DIR)) / 1024 / 1024
    print(f"\nSelesai! {extracted_count} file diekstrak ({total_mb:.0f} MB total) ke:")
    print(f"  {DEST_DIR}")
    print(f"\nllama-server: {size_mb:.2f} MB")
    print("\nSekarang Anda bisa menggunakan fitur AI tanpa Ollama atau llama-cpp-python.")


if __name__ == "__main__":
    main()
