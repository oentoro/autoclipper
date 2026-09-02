# build_release.ps1 - Builds a self-contained AutoClipper installer for Windows
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# Step 1: Setup vendor
Write-Host "==> Setting up bundled dependencies..."
& "$ScriptDir\setup_bundle.ps1"

# Step 2: Write config override to temp file (avoids PowerShell quoting issues)
Write-Host ""
Write-Host "==> Building Tauri app with bundled deps..."

# Only "msi" is built, out of tauri.conf.json's default "all": NSIS uses
# solid LZMA compression over the whole resource tree, which is dramatically
# slower than WiX/MSI for a payload this size (bundled Python + opencv +
# onnxruntime + mediapipe + models) -- over 50 minutes with no output before
# timing out in CI, while MSI packages the same resources in well under a
# minute. MSI already covers the "just double-click to install" use case.
$ExtraConfig = @{
    bundle = @{
        targets = @("msi")
        resources = @{
            "../scripts/transcribe.py" = "scripts/transcribe.py"
            "../scripts/burn_subtitles.py" = "scripts/burn_subtitles.py"
            "../scripts/smart_crop.py" = "scripts/smart_crop.py"
            "../scripts/face_censor.py" = "scripts/face_censor.py"
            "../scripts/download_whisper_model.py" = "scripts/download_whisper_model.py"
            "../scripts/download_llama_server.py" = "scripts/download_llama_server.py"
            "vendor/python/**/*" = "vendor/python/"
            "vendor/bin/*" = "vendor/bin/"
            "vendor/models/**/*" = "vendor/models/"
        }
    }
} | ConvertTo-Json -Depth 5

$TempConfig = [System.IO.Path]::GetTempFileName() + ".json"
$ExtraConfig | Set-Content -Path $TempConfig -Encoding UTF8

try {
    $TauriCli = Resolve-Path "$ScriptDir\..\node_modules\.bin\tauri.cmd"
    cmd /c "$TauriCli build --config `"$TempConfig`""
} finally {
    Remove-Item $TempConfig -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "[OK] Release build complete!"
Write-Host "  Output: src-tauri\target\release\bundle\"
