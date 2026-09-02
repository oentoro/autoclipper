# build_release.ps1 - Builds a self-contained AutoClipper distribution for Windows
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir = Split-Path -Parent $ScriptDir

# Step 1: Setup vendor
Write-Host "==> Setting up bundled dependencies..."
& "$ScriptDir\setup_bundle.ps1"

# Step 2: Build the exe only -- no installer bundling. WiX's light.exe hangs
# 50+ minutes (and NSIS's LZMA compression is worse) trying to package this
# payload's file count (bundled Python + opencv + onnxruntime + mediapipe +
# models). Tauri's own resource_dir() resolves to the exe's own directory on
# Windows (tauri-utils platform::resource_dir_from: `cfg!(target_os =
# "windows") => exe_dir`), so we stage scripts/ and vendor/ next to the exe
# ourselves and ship a zip -- same effective layout an installer would leave,
# built with plain DEFLATE instead of WiX/NSIS.
Write-Host ""
Write-Host "==> Building Tauri app (no installer bundling)..."
$TauriCli = Resolve-Path "$ScriptDir\..\node_modules\.bin\tauri.cmd"
cmd /c "$TauriCli build --no-bundle"
if ($LASTEXITCODE -ne 0) { throw "tauri build failed with exit code $LASTEXITCODE" }

# Step 3: Stage the 6 runtime scripts find_script() looks up (not the whole
# scripts/ dir -- that also has build tooling, tests, __pycache__).
Write-Host ""
Write-Host "==> Staging portable distribution..."
$ReleaseDir = "$ProjectDir\src-tauri\target\release"
$ScriptStage = "$env:TEMP\autoclipper_win_stage\scripts"
if (Test-Path (Split-Path $ScriptStage)) { Remove-Item (Split-Path $ScriptStage) -Recurse -Force }
New-Item -ItemType Directory -Force -Path $ScriptStage | Out-Null
foreach ($f in @("transcribe.py", "burn_subtitles.py", "smart_crop.py", "face_censor.py", "download_whisper_model.py", "download_llama_server.py")) {
    Copy-Item "$ProjectDir\scripts\$f" "$ScriptStage\$f"
}

# Step 4: Zip the exe + staged scripts + vendor (read in place -- vendor/ can
# be multiple GB across many files, no need to copy it before compressing).
# Fastest compression: this payload is mostly already-dense binary data
# (ONNX models, DLLs, ffmpeg), so higher levels buy little size for a lot
# more time -- and time is exactly what bit us with WiX/NSIS.
$BundleDir = "$ReleaseDir\bundle\zip"
New-Item -ItemType Directory -Force -Path $BundleDir | Out-Null
$ZipPath = "$BundleDir\AutoClipper_Windows_x64.zip"
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Compress-Archive -Path @(
    "$ReleaseDir\autoclipper.exe",
    $ScriptStage,
    "$ProjectDir\src-tauri\vendor"
) -DestinationPath $ZipPath -CompressionLevel Fastest

Write-Host ""
Write-Host "[OK] Release build complete!"
Write-Host "  Output: $ZipPath"
