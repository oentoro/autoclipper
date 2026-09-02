#!/usr/bin/env bash
# Builds a fully self-contained app bundle with all dependencies.
# Supports: macOS (.dmg / .app) and Linux (.deb)
# For Windows: use build_release.ps1
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR/.."

OS=$(uname -s)

# Step 1: Setup vendor directory
echo "==> Setting up bundled dependencies..."
bash "$SCRIPT_DIR/setup_bundle.sh"

# Step 2: Build Tauri app with vendor resources injected
echo ""
echo "==> Building Tauri app with bundled deps..."

# Only "deb" is built on Linux, out of tauri.conf.json's default "all":
# - rpm: rpmbuild tries to GPG-sign the package and hangs indefinitely
#   waiting on a pinentry prompt with no terminal/key configured (headless
#   dev machines and CI alike). Build rpm manually with a configured
#   signing key if you need it.
# - appimage: linuxdeploy walks every ELF file under the AppDir (including
#   the whole bundled vendor/ Python distribution — opencv, onnxruntime,
#   mediapipe, ...) to resolve shared library dependencies, and fails on
#   internal .so files it can't resolve outside their package's own RPATH.
#   Not fixable from this project's config; use the .deb or build from
#   source on non-Debian distros.
TARGETS_JSON=""
if [ "$OS" = "Linux" ]; then
  TARGETS_JSON='"targets": ["deb"],'
fi

EXTRA_CONFIG=$(cat <<JSONEOF
{
  "bundle": {
    $TARGETS_JSON
    "resources": [
      "../scripts/transcribe.py",
      "../scripts/burn_subtitles.py",
      "../scripts/smart_crop.py",
      "../scripts/download_llama_server.py",
      "vendor/python/**/*",
      "vendor/bin/*",
      "vendor/models/**/*"
    ]
  }
}
JSONEOF
)

npm run tauri build -- --config "$EXTRA_CONFIG" "$@"

echo ""
echo "✅ Release build complete!"
echo "   Output: src-tauri/target/release/bundle/"
