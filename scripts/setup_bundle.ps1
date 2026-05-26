# setup_bundle.ps1 - Run once before build_release.ps1
# Requires: Windows 10+, internet connection, winget or manual installs
$ErrorActionPreference = "Stop"

$ScriptDir  = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir = Split-Path -Parent $ScriptDir
$Vendor     = "$ProjectDir\src-tauri\vendor"

# Detect CPU architecture
$CpuArch = (Get-WmiObject -Class Win32_Processor).Architecture
$PythonArch = if ($CpuArch -eq 12) { "aarch64" } else { "x86_64" }

Write-Host "AutoClipper - Bundle Setup (Windows)"
Write-Host "Arch: $PythonArch  |  Vendor: $Vendor"
Write-Host "-------------------------------------------"

New-Item -ItemType Directory -Force -Path "$Vendor\bin"   | Out-Null
New-Item -ItemType Directory -Force -Path "$Vendor\models"| Out-Null

# ── Python (standalone, relocatable) ─────────────────────────────────────────
if (-not (Test-Path "$Vendor\python\python.exe")) {
    Write-Host "[1/4] Downloading Python 3.12 standalone..."
    $PythonRelease = "20250317"
    $PythonVersion = "3.12.9"
    $PythonTriple  = "$PythonArch-pc-windows-msvc"
    $PythonUrl = "https://github.com/astral-sh/python-build-standalone/releases/download/$PythonRelease/cpython-$PythonVersion+$PythonRelease-$PythonTriple-install_only.tar.gz"

    $TmpGz = "$env:TEMP\autoclipper_python.tar.gz"
    Invoke-WebRequest -Uri $PythonUrl -OutFile $TmpGz -UseBasicParsing
    # Windows 10+ has built-in tar
    tar -xzf $TmpGz -C $Vendor
    Remove-Item $TmpGz
    Write-Host "  [OK] Python $PythonVersion installed"
} else {
    Write-Host "[1/4] Python already bundled - skip"
}

$BundledPy = "$Vendor\python\python.exe"

# ── Python packages ───────────────────────────────────────────────────────────
Write-Host "[2/5] Installing faster-whisper + Pillow + opencv-python..."
& $BundledPy -m pip install --quiet --upgrade pip
& $BundledPy -m pip install --quiet faster-whisper Pillow opencv-python
Write-Host "  [OK] Packages installed"

# ── llama-server ──────────────────────────────────────────────────────────────
$LlamaBin = "$Vendor\bin\llama-server.exe"
if (Test-Path $LlamaBin) {
    Write-Host "[3/5] llama-server already bundled - skip"
} else {
    Write-Host "[3/5] Downloading llama-server..."
    & $BundledPy "$ScriptDir\download_llama_server.py"
    Write-Host "  [OK] llama-server installed"
}

# ── Whisper model ─────────────────────────────────────────────────────────────
$ModelMarker = "$Vendor\models\models--Systran--faster-whisper-small"
if (Test-Path $ModelMarker) {
    Write-Host "[4/5] Whisper model already downloaded - skip"
} else {
    Write-Host "[4/5] Downloading Whisper 'small' model (~244 MB)..."
    $VendorModels = "$Vendor\models" -replace "\\", "\\\\"
    & $BundledPy -c @"
from faster_whisper import WhisperModel
WhisperModel('small', device='cpu', compute_type='int8', download_root=r'$Vendor\models')
print('  [OK] Model downloaded')
"@
}

# ── FFmpeg (gyan.dev Windows essentials build) ───────────────────────────────
if ((Test-Path "$Vendor\bin\ffmpeg.exe") -and (Test-Path "$Vendor\bin\ffprobe.exe")) {
    Write-Host "[5/5] FFmpeg already bundled - skip"
} else {
    Write-Host "[5/5] Downloading FFmpeg for Windows..."
    $FfmpegUrl = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"
    $TmpZip    = "$env:TEMP\autoclipper_ffmpeg.zip"
    $TmpExtract = "$env:TEMP\autoclipper_ffmpeg_extract"

    Invoke-WebRequest -Uri $FfmpegUrl -OutFile $TmpZip -UseBasicParsing
    Expand-Archive -Path $TmpZip -DestinationPath $TmpExtract -Force

    # Archive contains: ffmpeg-<version>-essentials_build\bin\ffmpeg.exe
    $BinDir = Get-ChildItem $TmpExtract -Directory | Select-Object -First 1
    Copy-Item "$($BinDir.FullName)\bin\ffmpeg.exe"  "$Vendor\bin\"
    Copy-Item "$($BinDir.FullName)\bin\ffprobe.exe" "$Vendor\bin\"

    Remove-Item $TmpZip -Force
    Remove-Item $TmpExtract -Recurse -Force
    Write-Host "  [OK] FFmpeg installed"
}

Write-Host ""
Write-Host "[OK] Bundle setup complete!"
$VendorSize = (Get-ChildItem $Vendor -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB
Write-Host ("  Total size: {0:N0} MB" -f $VendorSize)
Write-Host "Next: .\scripts\build_release.ps1"
