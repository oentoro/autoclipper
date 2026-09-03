# AutoClipper

AI-powered desktop app that turns a long video into short, ready-to-post clips — automatic transcription, AI segment selection, smart vertical cropping, styled burned-in subtitles, and privacy face/head blurring, all running locally.

Built with [Tauri](https://tauri.app) (Rust + React/TypeScript), open source under the MIT license.

## Download

Prebuilt installers (macOS `.dmg`, Linux `.deb`, Windows `.zip`) are on the [Releases page](https://github.com/oentoro/autoclipper/releases/latest).

## Features

- **Transcription** — local Whisper transcription (`faster-whisper`, with `mlx-whisper` GPU/Neural Engine acceleration on Apple Silicon), multi-language source detection, speed presets (fast/balanced/accurate/best)
- **Subtitle chunking** — group transcript into subtitle lines by word count (auto / 1 / 2 / 3 words), including karaoke-style one-word captions
- **Translation** — translate the transcript to another language, with original-only / translated-only / bilingual subtitle modes
- **AI segment selection** — a local LLM (Ollama or a bundled GGUF model) classifies the video into sections and picks the most important segments, or select segments manually
- **Draggable subtitle timeline** — trim each subtitle's start/end time by dragging directly on a video-synced timeline, like CapCut/DaVinci Resolve
- **Smart crop** — automatically follow the speaker's face when converting to vertical (9:16), with smooth or aggressive tracking, plus fixed aspect ratios (16:9, 1:1, 4:5)
- **Burned-in subtitles** — customizable color, outline, background box, position, font, size, and ALL CAPS
- **Face/head censoring** — pixelate or overlay-image privacy blur, detecting faces or full heads (MediaPipe), for anyone who needs to stay off camera
- **Caption generation** — AI-generated short/long captions and hashtags per clip
- **YouTube import** — download a source video directly by URL (`yt-dlp[default]` + Deno as the JS-challenge runtime)
- **Bilingual UI** — Indonesian and English

## Tech Stack

- **App shell:** Tauri 2.x (Rust)
- **Frontend:** React + TypeScript, Vite
- **Processing:** Python (`faster-whisper`/`mlx-whisper`, OpenCV, MediaPipe, InsightFace), FFmpeg
- **Local AI:** Ollama or a bundled GGUF model for segment classification and captions

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) + Cargo
- [Node.js](https://nodejs.org) 18+ and npm
- [Tauri CLI prerequisites](https://tauri.app/start/prerequisites/) for your OS
- Python 3 (the app can set up its own bundled Python environment — see below)
- FFmpeg + FFprobe (bundled automatically on first run if not found on your system)
- Optional: [Ollama](https://ollama.com) for local LLM-powered segment selection and captions
- Optional: [Deno](https://deno.land) for YouTube import — yt-dlp needs a JS runtime to solve YouTube's signature challenge; without it, downloads fail with HTTP 403

## Development

```bash
npm install
npm run tauri dev
```

The app checks its Python/FFmpeg/model dependencies on first launch and offers to install what's missing (see `scripts/setup_bundle.sh` / `scripts/setup_bundle.ps1`).

Run the frontend test suite:

```bash
npm test
```

## Building a Release

```bash
# macOS / Linux
./scripts/build_release.sh

# Windows
./scripts/build_release.ps1
```

This bundles Python, FFmpeg, and the required models into a self-contained app under `src-tauri/target/release/bundle/`.

## Project Structure

```
src/              React/TypeScript frontend
src-tauri/        Rust backend (Tauri commands, process orchestration)
scripts/          Python processing scripts (transcribe, censor, crop, burn subtitles)
docs/superpowers/ Design specs and implementation plans for past features
```

## License

MIT — see [LICENSE](LICENSE).
