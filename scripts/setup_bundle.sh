#!/usr/bin/env bash
# Run once before "npm run tauri build" to set up bundled dependencies.
# Result: src-tauri/vendor/ containing Python, packages, FFmpeg, and Whisper model.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VENDOR="$SCRIPT_DIR/../src-tauri/vendor"
ARCH=$(uname -m)  # arm64 or x86_64

echo "AutoClipper — Bundle Setup"
echo "Arch: $ARCH  |  Vendor: $VENDOR"
echo "────────────────────────────────────────"
mkdir -p "$VENDOR/bin" "$VENDOR/models"

# ── Python (standalone, relocatable) ─────────────────────────────────────────
if [ ! -f "$VENDOR/python/bin/python3" ]; then
  echo "[1/4] Downloading Python 3.12 standalone (~30 MB)..."
  PYTHON_RELEASE="20250317"
  PYTHON_VERSION="3.12.9"
  if [ "$ARCH" = "arm64" ]; then
    PYTHON_TRIPLE="aarch64-apple-darwin"
  else
    PYTHON_TRIPLE="x86_64-apple-darwin"
  fi
  PYTHON_URL="https://github.com/astral-sh/python-build-standalone/releases/download/${PYTHON_RELEASE}/cpython-${PYTHON_VERSION}+${PYTHON_RELEASE}-${PYTHON_TRIPLE}-install_only.tar.gz"
  curl -L --progress-bar "$PYTHON_URL" | tar -xz -C "$VENDOR"
  echo "  ✓ Python $PYTHON_VERSION installed"
else
  echo "[1/4] Python already bundled — skip"
fi

BUNDLED_PY="$VENDOR/python/bin/python3"

# ── Python packages ───────────────────────────────────────────────────────────
echo "[2/4] Installing faster-whisper + Pillow..."
"$BUNDLED_PY" -m pip install --quiet --upgrade pip
"$BUNDLED_PY" -m pip install --quiet faster-whisper Pillow
echo "  ✓ Packages installed"

# ── Whisper model ─────────────────────────────────────────────────────────────
MODEL_DIR="$VENDOR/models"
# Check if model already downloaded (faster-whisper stores in models--Systran--faster-whisper-small)
if ls "$MODEL_DIR"/models--Systran--faster-whisper-small 2>/dev/null | grep -q snapshots; then
  echo "[3/4] Whisper model already downloaded — skip"
else
  echo "[3/4] Downloading Whisper 'small' model (~244 MB)..."
  "$BUNDLED_PY" - <<PYEOF
import sys
sys.stdout.flush()
from faster_whisper import WhisperModel
WhisperModel("small", device="cpu", compute_type="int8", download_root="$MODEL_DIR")
print("  ✓ Model downloaded")
PYEOF
fi

# ── FFmpeg static build ───────────────────────────────────────────────────────
if [ -f "$VENDOR/bin/ffmpeg" ] && [ -f "$VENDOR/bin/ffprobe" ]; then
  echo "[4/4] FFmpeg already bundled — skip"
else
  echo "[4/4] Downloading FFmpeg static build..."
  TMP=$(mktemp -d)
  trap "rm -rf $TMP" EXIT

  # evermeet.cx provides static macOS builds (no Homebrew dylib deps, only system frameworks)
  FFMPEG_OK=false
  if curl -fL --progress-bar "https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip" -o "$TMP/ffmpeg.zip" 2>/dev/null; then
    if curl -fL --progress-bar "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip" -o "$TMP/ffprobe.zip" 2>/dev/null; then
      unzip -q -o "$TMP/ffmpeg.zip" -d "$VENDOR/bin/"
      unzip -q -o "$TMP/ffprobe.zip" -d "$VENDOR/bin/"
      chmod +x "$VENDOR/bin/ffmpeg" "$VENDOR/bin/ffprobe"
      FFMPEG_OK=true
      echo "  ✓ FFmpeg static build installed"
    fi
  fi

  # Fallback: copy from Homebrew (dynamic — needs Homebrew on end user machine)
  if [ "$FFMPEG_OK" = false ]; then
    echo "  ⚠ Static download failed — falling back to Homebrew copy"
    for CANDIDATE in /opt/homebrew/bin/ffmpeg /usr/local/bin/ffmpeg; do
      if [ -f "$CANDIDATE" ]; then
        cp "$CANDIDATE" "$VENDOR/bin/ffmpeg"
        cp "${CANDIDATE/ffmpeg/ffprobe}" "$VENDOR/bin/ffprobe" 2>/dev/null || true
        chmod +x "$VENDOR/bin/ffmpeg" "$VENDOR/bin/ffprobe"
        echo "  ⚠ Copied $CANDIDATE — app may require Homebrew on target Mac"
        FFMPEG_OK=true
        break
      fi
    done
  fi

  if [ "$FFMPEG_OK" = false ]; then
    echo "  ✗ FFmpeg not found — install via: brew install ffmpeg"
    exit 1
  fi
fi

echo ""
echo "✅ Bundle ready!"
du -sh "$VENDOR"
echo ""
echo "Next: npm run tauri build"
