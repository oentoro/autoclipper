#!/usr/bin/env bash
# Builds a fully self-contained .dmg / .app with all dependencies bundled.
# Run from the project root: ./scripts/build_release.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
VENDOR="$PROJECT_DIR/src-tauri/vendor"

cd "$PROJECT_DIR"

# ── Step 1: Setup vendor directory ───────────────────────────────────────────
echo "==> Setting up bundled dependencies..."
bash "$SCRIPT_DIR/setup_bundle.sh"

# ── Step 2: Build with vendor resources injected into config ─────────────────
echo ""
echo "==> Building Tauri app with bundled deps..."

# Pass extra resource paths via --config override (merges with tauri.conf.json)
EXTRA_CONFIG=$(cat <<'JSONEOF'
{
  "bundle": {
    "resources": [
      "../scripts/transcribe.py",
      "../scripts/burn_subtitles.py",
      "vendor/python/**/*",
      "vendor/bin/ffmpeg",
      "vendor/bin/ffprobe",
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
