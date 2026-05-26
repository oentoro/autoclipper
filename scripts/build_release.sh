#!/usr/bin/env bash
# Builds a fully self-contained app bundle with all dependencies.
# Supports: macOS (.dmg / .app) and Linux (.deb / .rpm / .AppImage)
# For Windows: use build_release.ps1
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR/.."

# Step 1: Setup vendor directory
echo "==> Setting up bundled dependencies..."
bash "$SCRIPT_DIR/setup_bundle.sh"

# Step 2: Build Tauri app with vendor resources injected
echo ""
echo "==> Building Tauri app with bundled deps..."

EXTRA_CONFIG=$(cat <<'JSONEOF'
{
  "bundle": {
    "resources": [
      "../scripts/transcribe.py",
      "../scripts/burn_subtitles.py",
      "../scripts/smart_crop.py",
      "../scripts/download_llama_server.py",
      "vendor/python/**/*",
      "vendor/bin/**",
      "vendor/models/**/*"
    ]
  }
}
JSONEOF
)

npm run tauri build -- --config "$EXTRA_CONFIG"

echo ""
echo "✅ Release build complete!"
echo "   Output: src-tauri/target/release/bundle/"
