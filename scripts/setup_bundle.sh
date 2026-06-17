#!/usr/bin/env bash
# Run once before build_release.sh to bundle all dependencies.
# Supports: macOS (ARM + x86) and Linux (ARM + x86)
# For Windows: use setup_bundle.ps1
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VENDOR="$SCRIPT_DIR/../src-tauri/vendor"
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Darwin) ;;
  Linux)  ;;
  *)
    echo "Unsupported OS: $OS"
    echo "Use scripts/setup_bundle.ps1 on Windows."
    exit 1
    ;;
esac

echo "AutoClipper — Bundle Setup"
echo "OS: $OS  |  Arch: $ARCH  |  Vendor: $VENDOR"
echo "────────────────────────────────────────────────"
mkdir -p "$VENDOR/bin" "$VENDOR/models"

# ── Python (standalone, relocatable) ─────────────────────────────────────────
if [ ! -f "$VENDOR/python/bin/python3" ]; then
  echo "[1/4] Downloading Python 3.12 standalone..."
  PYTHON_RELEASE="20250317"
  PYTHON_VERSION="3.12.9"

  if [ "$OS" = "Darwin" ]; then
    case "$ARCH" in
      arm64|aarch64) PYTHON_TRIPLE="aarch64-apple-darwin" ;;
      *) PYTHON_TRIPLE="x86_64-apple-darwin" ;;
    esac
  else
    case "$ARCH" in
      aarch64|arm64) PYTHON_TRIPLE="aarch64-unknown-linux-gnu" ;;
      *) PYTHON_TRIPLE="x86_64-unknown-linux-gnu" ;;
    esac
  fi

  PYTHON_URL="https://github.com/astral-sh/python-build-standalone/releases/download/${PYTHON_RELEASE}/cpython-${PYTHON_VERSION}+${PYTHON_RELEASE}-${PYTHON_TRIPLE}-install_only.tar.gz"
  echo "  Downloading from: $PYTHON_URL"
  curl -L --progress-bar "$PYTHON_URL" | tar -xz -C "$VENDOR"
  echo "  ✓ Python $PYTHON_VERSION installed"
else
  echo "[1/4] Python already bundled — skip"
fi

BUNDLED_PY="$VENDOR/python/bin/python3"

# ── Python packages ───────────────────────────────────────────────────────────
echo "[2/5] Installing Python packages..."
"$BUNDLED_PY" -m pip install --quiet --upgrade pip
"$BUNDLED_PY" -m pip install --quiet faster-whisper Pillow opencv-python mediapipe insightface onnxruntime

# On Apple Silicon: install mlx-whisper for 5-15x faster transcription
if [ "$OS" = "Darwin" ] && [ "$ARCH" = "arm64" ]; then
  echo "  [Apple Silicon] Installing mlx-whisper..."
  "$BUNDLED_PY" -m pip install --quiet mlx-whisper
  echo "  ✓ mlx-whisper installed (GPU/Neural Engine acceleration)"
fi
echo "  ✓ Packages installed"

# ── llama-server ─────────────────────────────────────────────────────────────
LLAMA_BIN="$VENDOR/bin/llama-server"
if [ -f "$LLAMA_BIN" ]; then
  echo "[3/5] llama-server already bundled — skip"
else
  echo "[3/5] Downloading llama-server..."
  "$BUNDLED_PY" "$SCRIPT_DIR/download_llama_server.py"
  echo "  ✓ llama-server installed"
fi

# ── Whisper model ─────────────────────────────────────────────────────────────
MODEL_MARKER="$VENDOR/models/models--Systran--faster-whisper-small"
if [ -d "$MODEL_MARKER" ]; then
  echo "[4/5] Whisper model already downloaded — skip"
else
  echo "[4/5] Downloading Whisper 'small' model (~244 MB)..."
  "$BUNDLED_PY" - <<PYEOF
from faster_whisper import WhisperModel
WhisperModel("small", device="cpu", compute_type="int8", download_root="$VENDOR/models")
print("  ✓ Model downloaded")
PYEOF
fi

# ── FFmpeg ────────────────────────────────────────────────────────────────────
if [ -f "$VENDOR/bin/ffmpeg" ] && [ -f "$VENDOR/bin/ffprobe" ]; then
  echo "[5/5] FFmpeg already bundled — skip"
else
  echo "[5/5] Downloading FFmpeg static build..."
  TMP=$(mktemp -d)
  trap "rm -rf '$TMP'" EXIT

  FFMPEG_OK=false

  if [ "$OS" = "Darwin" ]; then
    # evermeet.cx — static macOS builds (no Homebrew dylib deps)
    if curl -fsSL "https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip" -o "$TMP/ffmpeg.zip" && \
       curl -fsSL "https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip" -o "$TMP/ffprobe.zip"; then
      unzip -q -o "$TMP/ffmpeg.zip"  -d "$VENDOR/bin/"
      unzip -q -o "$TMP/ffprobe.zip" -d "$VENDOR/bin/"
      chmod +x "$VENDOR/bin/ffmpeg" "$VENDOR/bin/ffprobe"
      FFMPEG_OK=true
      echo "  ✓ FFmpeg (evermeet.cx) installed"
    fi

  elif [ "$OS" = "Linux" ]; then
    # BtbN/FFmpeg-Builds — static Linux builds
    case "$ARCH" in
      aarch64|arm64) FFMPEG_FILE="ffmpeg-master-latest-linuxarm64-gpl.tar.xz" ;;
      *) FFMPEG_FILE="ffmpeg-master-latest-linux64-gpl.tar.xz" ;;
    esac
    FFMPEG_URL="https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/$FFMPEG_FILE"

    if curl -fsSL "$FFMPEG_URL" -o "$TMP/ffmpeg.tar.xz"; then
      tar -xf "$TMP/ffmpeg.tar.xz" -C "$TMP/"
      # Archive structure: ffmpeg-master-latest-linux64-gpl/bin/ffmpeg
      find "$TMP" -maxdepth 3 -name "ffmpeg"  -type f -exec cp {} "$VENDOR/bin/" \;
      find "$TMP" -maxdepth 3 -name "ffprobe" -type f -exec cp {} "$VENDOR/bin/" \;
      chmod +x "$VENDOR/bin/ffmpeg" "$VENDOR/bin/ffprobe"
      FFMPEG_OK=true
      echo "  ✓ FFmpeg (BtbN) installed"
    fi
  fi

  # Fallback: copy from system (might have Homebrew/system dylib deps)
  if [ "$FFMPEG_OK" = false ]; then
    echo "  ⚠ Download failed — falling back to system FFmpeg copy"
    for CANDIDATE in /opt/homebrew/bin/ffmpeg /usr/local/bin/ffmpeg /usr/bin/ffmpeg; do
      if [ -f "$CANDIDATE" ]; then
        cp "$CANDIDATE" "$VENDOR/bin/ffmpeg"
        cp "${CANDIDATE/ffmpeg/ffprobe}" "$VENDOR/bin/ffprobe" 2>/dev/null || true
        chmod +x "$VENDOR/bin/ffmpeg" "$VENDOR/bin/ffprobe" 2>/dev/null || true
        echo "  ⚠ Copied $CANDIDATE (may require system libs on target machine)"
        FFMPEG_OK=true
        break
      fi
    done
  fi

  if [ "$FFMPEG_OK" = false ]; then
    echo "  ✗ FFmpeg not found — install: brew install ffmpeg  (macOS) or  sudo apt install ffmpeg  (Linux)"
    exit 1
  fi
fi

echo ""
echo "✅ Bundle setup complete!"
du -sh "$VENDOR"
echo "Next: ./scripts/build_release.sh"
