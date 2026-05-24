# build_release.ps1 - Builds a self-contained AutoClipper installer for Windows
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# Step 1: Setup vendor
Write-Host "==> Setting up bundled dependencies..."
& "$ScriptDir\setup_bundle.ps1"

# Step 2: Write config override to temp file (avoids PowerShell quoting issues)
Write-Host ""
Write-Host "==> Building Tauri app with bundled deps..."

$ExtraConfig = @{
    bundle = @{
        resources = @(
            "../scripts/transcribe.py"
            "../scripts/burn_subtitles.py"
            "vendor/python/**/*"
            "vendor/bin/ffmpeg.exe"
            "vendor/bin/ffprobe.exe"
            "vendor/models/**/*"
        )
    }
} | ConvertTo-Json -Depth 5

$TempConfig = [System.IO.Path]::GetTempFileName() + ".json"
$ExtraConfig | Set-Content -Path $TempConfig -Encoding UTF8

try {
    npm run tauri build -- --config $TempConfig
} finally {
    Remove-Item $TempConfig -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "[OK] Release build complete!"
Write-Host "  Output: src-tauri\target\release\bundle\"
