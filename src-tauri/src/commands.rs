use serde::{Deserialize, Serialize};
use std::process::Command;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Manager, Emitter};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SrtSegment {
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub segments: Vec<SrtSegment>,
    pub srt_content: String,
    #[serde(default)]
    pub detected_language: String,
    #[serde(default)]
    pub raw_segments: Vec<SrtSegment>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManualClip {
    pub start_sec: f64,
    pub end_sec: f64,
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranslateResult {
    pub segments: Vec<SrtSegment>,
    pub srt_content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeResult {
    pub important_indices: Vec<usize>,
    pub reasoning: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Section {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub start_index: usize,
    #[serde(default)]
    pub end_index: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassifyResult {
    pub sections: Vec<Section>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlmModel {
    pub name: String,
    pub path: String,
    pub ollama_model: String,
    pub size_mb: u64,
    pub source: String, // "local" | "ollama"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClipResult {
    pub output_path: String,
    pub success: bool,
    pub message: String,
    pub total_segments: usize,
    pub duration_secs: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DepCheck {
    pub name: String,
    pub ok: bool,
    pub path: Option<String>,
    pub error: Option<String>,
    pub install_cmd: Option<String>,
    pub download_url: Option<String>,
    pub optional: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DepsStatus {
    pub all_required_ok: bool,
    pub checks: Vec<DepCheck>,
    pub platform: String,
    pub build_notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FontInfo {
    pub name: String,
    pub path: String,
}

#[derive(Default)]
pub struct DownloadState(pub Mutex<std::collections::HashSet<String>>);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WhisperModelInfo {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub preset: String,
    pub preset_label: String,
    pub description: String,
    pub size_mb: u64,
    pub cached: bool,
    pub cache_path: Option<String>,
}

#[derive(Default)]
pub struct WhisperDownloadState {
    pub cancel_keys: Mutex<std::collections::HashSet<String>>,
    pub pids:        Mutex<std::collections::HashMap<String, u32>>,
}

#[derive(Default)]
pub struct ProcessCancelState {
    pub transcribe_pid: Mutex<Option<u32>>,
    pub clip_pid:       Mutex<Option<u32>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct YtDownloadProgress {
    pub percent:   f32,
    pub speed:     String,
    pub eta:       String,
    pub phase:     String,     // "downloading" | "merging" | "done"
    pub downloaded: String,    // e.g. "10.45MiB"
    pub total:     String,     // e.g. "45.67MiB"
}

#[derive(Default)]
pub struct YtDownloadState {
    pub pid: Mutex<Option<u32>>,
}

fn kill_pid(pid: u32) {
    #[cfg(unix)]
    unsafe { libc::kill(pid as i32, libc::SIGTERM); }
    #[cfg(windows)]
    { let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output(); }
}

#[tauri::command]
pub fn cancel_transcription(state: tauri::State<'_, ProcessCancelState>) {
    if let Ok(mut g) = state.transcribe_pid.lock() {
        if let Some(pid) = g.take() { kill_pid(pid); }
    }
}

#[tauri::command]
pub fn cancel_clipping(state: tauri::State<'_, ProcessCancelState>) {
    if let Ok(mut g) = state.clip_pid.lock() {
        if let Some(pid) = g.take() { kill_pid(pid); }
    }
}

#[tauri::command]
pub fn cancel_youtube_download(state: tauri::State<'_, YtDownloadState>) {
    if let Ok(mut g) = state.pid.lock() {
        if let Some(pid) = g.take() { kill_pid(pid); }
    }
}

pub struct LlamaServerState {
    process:      std::sync::Mutex<Option<std::process::Child>>,
    current_model: std::sync::Mutex<String>,
    startup_lock: tokio::sync::Mutex<()>,
}

impl Default for LlamaServerState {
    fn default() -> Self {
        Self {
            process:       std::sync::Mutex::new(None),
            current_model: std::sync::Mutex::new(String::new()),
            startup_lock:  tokio::sync::Mutex::new(()),
        }
    }
}

impl Drop for LlamaServerState {
    fn drop(&mut self) {
        if let Ok(mut g) = self.process.lock() {
            if let Some(mut child) = g.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

// ─── Virtual environment helpers ──────────────────────────────────────────────

pub fn venv_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    #[cfg(not(target_os = "windows"))]
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".autoclipper").join("venv")
}

fn venv_python() -> Option<String> {
    let venv = venv_dir();
    #[cfg(target_os = "windows")]
    let p = venv.join("Scripts").join("python.exe");
    #[cfg(not(target_os = "windows"))]
    let p = venv.join("bin").join("python3");
    if p.exists() { Some(p.to_string_lossy().to_string()) } else { None }
}

#[allow(dead_code)]
fn venv_pip() -> Option<String> {
    let venv = venv_dir();
    #[cfg(target_os = "windows")]
    let p = venv.join("Scripts").join("pip.exe");
    #[cfg(not(target_os = "windows"))]
    let p = venv.join("bin").join("pip");
    if p.exists() { Some(p.to_string_lossy().to_string()) } else { None }
}

// ─── Platform helpers ─────────────────────────────────────────────────────────

fn which(bin: &str) -> Option<String> {
    #[cfg(target_os = "windows")]
    let cmd = "where";
    #[cfg(not(target_os = "windows"))]
    let cmd = "which";

    let out = Command::new(cmd).arg(bin).output().ok()?;
    if out.status.success() {
        let line = String::from_utf8_lossy(&out.stdout);
        let p = line.lines().next()?.trim().to_string();
        if !p.is_empty() { return Some(p); }
    }
    None
}

fn vendor_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?.join("vendor");
    if dir.exists() { Some(dir) } else { None }
}

// ─── Cross-platform path finders ─────────────────────────────────────────────

fn find_python(vendor: Option<&Path>) -> String {
    // Prefer managed venv — isolated, no PEP 668 issues, has all packages
    if let Some(vp) = venv_python() {
        return vp;
    }
    if let Some(v) = vendor {
        #[cfg(target_os = "windows")]
        let p = v.join("python").join("python.exe");
        #[cfg(not(target_os = "windows"))]
        let p = v.join("python").join("bin").join("python3");
        if p.exists() { return p.to_string_lossy().to_string(); }
    }

    #[cfg(target_os = "macos")]
    {
        for c in ["/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"] {
            if Path::new(c).exists() { return c.to_string(); }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for c in ["/usr/bin/python3", "/usr/local/bin/python3"] {
            if Path::new(c).exists() { return c.to_string(); }
        }
    }

    #[cfg(target_os = "windows")]
    {
        fn python_works(path: &str) -> bool {
            Command::new(path).arg("--version").output()
                .map(|o| o.status.success()).unwrap_or(false)
        }

        // `py` launcher installed by official Python / Chocolatey — most reliable
        if let Some(py) = which("py") {
            if python_works(&py) { return py; }
        }

        // Scan all `where python` results, skip Windows Store stub
        if let Ok(out) = Command::new("where").arg("python").output() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let p = line.trim();
                if p.is_empty() || p.contains("WindowsApps") { continue; }
                if python_works(p) { return p.to_string(); }
            }
        }

        // Common Chocolatey / official installer paths
        for ver in ["314", "313", "312", "311", "310", "39", "38"] {
            let path = format!("C:\\Python{}\\python.exe", ver);
            if python_works(&path) { return path; }
        }
        if python_works("C:\\ProgramData\\chocolatey\\bin\\python.exe") {
            return "C:\\ProgramData\\chocolatey\\bin\\python.exe".to_string();
        }

        "python.exe".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    { which("python3").unwrap_or_else(|| "python3".to_string()) }
}

fn find_ffmpeg(vendor: Option<&Path>) -> String {
    if let Some(v) = vendor {
        #[cfg(target_os = "windows")]
        let p = v.join("bin").join("ffmpeg.exe");
        #[cfg(not(target_os = "windows"))]
        let p = v.join("bin").join("ffmpeg");
        if p.exists() { return p.to_string_lossy().to_string(); }
    }

    #[cfg(target_os = "macos")]
    {
        for c in ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"] {
            if Path::new(c).exists() { return c.to_string(); }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for c in ["/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
            if Path::new(c).exists() { return c.to_string(); }
        }
    }

    #[cfg(target_os = "windows")]
    {
        for c in ["C:\\ffmpeg\\bin\\ffmpeg.exe", "C:\\ProgramData\\chocolatey\\bin\\ffmpeg.exe"] {
            if Path::new(c).exists() { return c.to_string(); }
        }
        which("ffmpeg").unwrap_or_else(|| "ffmpeg.exe".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    { which("ffmpeg").unwrap_or_else(|| "ffmpeg".to_string()) }
}

fn find_ffprobe(vendor: Option<&Path>) -> String {
    if let Some(v) = vendor {
        #[cfg(target_os = "windows")]
        let p = v.join("bin").join("ffprobe.exe");
        #[cfg(not(target_os = "windows"))]
        let p = v.join("bin").join("ffprobe");
        if p.exists() { return p.to_string_lossy().to_string(); }
    }

    // Derive from ffmpeg path (replace binary name)
    let ffmpeg = find_ffmpeg(vendor);
    #[cfg(target_os = "windows")]
    let probe_derived = ffmpeg.replace("ffmpeg.exe", "ffprobe.exe");
    #[cfg(not(target_os = "windows"))]
    let probe_derived = ffmpeg.replace("ffmpeg", "ffprobe");
    if probe_derived != ffmpeg && Path::new(&probe_derived).exists() {
        return probe_derived;
    }

    #[cfg(target_os = "macos")]
    {
        for c in ["/opt/homebrew/bin/ffprobe", "/usr/local/bin/ffprobe", "/usr/bin/ffprobe"] {
            if Path::new(c).exists() { return c.to_string(); }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for c in ["/usr/bin/ffprobe", "/usr/local/bin/ffprobe"] {
            if Path::new(c).exists() { return c.to_string(); }
        }
    }

    #[cfg(target_os = "windows")]
    { which("ffprobe").unwrap_or_else(|| "ffprobe.exe".to_string()) }
    #[cfg(not(target_os = "windows"))]
    { which("ffprobe").unwrap_or_else(|| "ffprobe".to_string()) }
}

fn find_script(app: &tauri::AppHandle, name: &str) -> String {
    // In dev mode the exe lives at src-tauri/target/debug/<bin>; going up 4 levels
    // reaches the project root where scripts/ is the live source — always prefer this
    // so edits to scripts take effect without clearing Tauri's resource cache.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent().and_then(|p| p.parent())
            .and_then(|p| p.parent()).and_then(|p| p.parent())
        {
            let p = root.join("scripts").join(name);
            if p.exists() { return p.to_string_lossy().to_string(); }
        }
    }
    // Production: scripts are bundled into the resource dir
    if let Ok(resource_dir) = app.path().resource_dir() {
        let p = resource_dir.join("scripts").join(name);
        if p.exists() { return p.to_string_lossy().to_string(); }
    }
    let cwd_path = format!("scripts/{name}");
    if Path::new(&cwd_path).exists() { return cwd_path; }
    format!("scripts/{name}")
}

fn find_model_dir(vendor: Option<&Path>) -> Option<String> {
    let dir = vendor?.join("models");
    if dir.exists() { Some(dir.to_string_lossy().to_string()) } else { None }
}

/// Directories to scan for local .gguf models (vendor, dev source, app data).
fn llm_search_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(v) = vendor_dir(app) { dirs.push(v.join("llm")); }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent().and_then(|p| p.parent())
            .and_then(|p| p.parent()).and_then(|p| p.parent())
        {
            dirs.push(root.join("src-tauri").join("vendor").join("llm"));
        }
    }
    if let Ok(data) = app.path().app_data_dir() { dirs.push(data.join("models")); }
    dirs
}

/// Find the first .gguf file (used when no explicit model is selected).
fn find_llm_model(app: &tauri::AppHandle) -> Option<String> {
    for dir in llm_search_dirs(app) {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map(|e| e == "gguf").unwrap_or(false) {
                    return Some(p.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

fn find_llama_server(app: &tauri::AppHandle) -> Option<String> {
    let name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };

    // Bundled vendor/bin/
    if let Some(v) = vendor_dir(app) {
        let p = v.join("bin").join(name);
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    // Dev source: src-tauri/vendor/bin/
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent().and_then(|p| p.parent())
            .and_then(|p| p.parent()).and_then(|p| p.parent())
        {
            let p = root.join("src-tauri").join("vendor").join("bin").join(name);
            if p.exists() { return Some(p.to_string_lossy().to_string()); }
        }
    }
    // System PATH
    which(if cfg!(windows) { "llama-server.exe" } else { "llama-server" })
}

fn find_ytdlp() -> Option<String> {
    // Check venv first (where user likely installed it via pip)
    let venv = venv_dir();
    #[cfg(not(target_os = "windows"))]
    let venv_bin = venv.join("bin").join("yt-dlp");
    #[cfg(target_os = "windows")]
    let venv_bin = venv.join("Scripts").join("yt-dlp.exe");
    if venv_bin.exists() { return Some(venv_bin.to_string_lossy().to_string()); }
    // System PATH
    which(if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" })
}

fn parse_yt_percent(line: &str) -> Option<f32> {
    let pct_pos = line.find('%')?;
    let before = line[..pct_pos].trim_end();
    let num_start = before.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
    before[num_start..].parse::<f32>().ok()
}

fn parse_yt_speed(line: &str) -> String {
    line.find(" at ").map(|pos| {
        line[pos + 4..].split_whitespace().next().unwrap_or("").to_string()
    }).unwrap_or_default()
}

fn parse_yt_eta(line: &str) -> String {
    line.find("ETA ").map(|pos| {
        line[pos + 4..].split_whitespace().next().unwrap_or("").to_string()
    }).unwrap_or_default()
}

fn parse_yt_total(line: &str) -> String {
    // "[download]  23.4% of 45.67MiB at ..."  →  "45.67MiB"
    line.find(" of ").map(|pos| {
        line[pos + 4..].split_whitespace().next().unwrap_or("").trim_start_matches('~').to_string()
    }).unwrap_or_default()
}

fn parse_size_bytes(s: &str) -> Option<f64> {
    let table: &[(&str, f64)] = &[
        ("GiB", 1024.0_f64 * 1024.0 * 1024.0),
        ("MiB", 1024.0_f64 * 1024.0),
        ("KiB", 1024.0_f64),
        ("GB",  1_000_000_000.0),
        ("MB",  1_000_000.0),
        ("KB",  1_000.0),
        ("B",   1.0),
    ];
    for (suffix, mult) in table {
        if s.ends_with(suffix) {
            let num = s[..s.len() - suffix.len()].parse::<f64>().ok()?;
            return Some(num * mult);
        }
    }
    None
}

fn format_bytes(bytes: f64) -> String {
    if bytes >= 1024.0_f64 * 1024.0 * 1024.0 {
        format!("{:.2}GiB", bytes / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024.0_f64 * 1024.0 {
        format!("{:.2}MiB", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0_f64 {
        format!("{:.2}KiB", bytes / 1024.0)
    } else {
        format!("{:.0}B", bytes)
    }
}

/// Turn a GGUF filename stem into a readable display name.
/// e.g. "gemma-3-4b-it-Q4_K_M" → "Gemma 3 4B (Q4_K_M)"
fn format_gguf_name(stem: &str) -> String {
    let parts: Vec<&str> = stem.split('-').collect();
    // Find quantization suffix (starts with Q followed by a digit)
    let quant_pos = parts.iter().position(|p| {
        p.starts_with('Q') && p.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false)
    });
    let (name_parts, quant) = match quant_pos {
        Some(i) => (&parts[..i], Some(parts[i..].join("-"))),
        None    => (parts.as_slice(), None),
    };
    let name: String = name_parts.iter()
        .filter(|p| !matches!(p.to_lowercase().as_str(), "it" | "instruct" | "chat"))
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                None    => String::new(),
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    match quant {
        Some(q) => format!("{name} ({q})"),
        None    => name,
    }
}

#[tauri::command]
pub async fn list_llm_models(app: tauri::AppHandle) -> Vec<LlmModel> {
    let mut models: Vec<LlmModel> = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // ── Local GGUF files ──────────────────────────────────────────────────
    for dir in llm_search_dirs(&app) {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut file_models: Vec<LlmModel> = entries.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().map(|x| x == "gguf").unwrap_or(false) {
                        let canonical = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                        if !seen_paths.insert(canonical) { return None; } // dedup
                        let size_mb = std::fs::metadata(&p).map(|m| m.len() / (1024 * 1024)).unwrap_or(0);
                        let stem    = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                        Some(LlmModel {
                            name:         format_gguf_name(&stem),
                            path:         p.to_string_lossy().to_string(),
                            ollama_model: String::new(),
                            size_mb,
                            source:       "local".to_string(),
                        })
                    } else { None }
                })
                .collect();
            file_models.sort_by(|a, b| a.name.cmp(&b.name));
            models.extend(file_models);
        }
    }

    // ── Ollama installed models ───────────────────────────────────────────
    let client = reqwest::Client::new();
    if let Ok(resp) = client.get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(2)).send().await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(arr) = json["models"].as_array() {
                for m in arr {
                    if let Some(name) = m["name"].as_str() {
                        models.push(LlmModel {
                            name:         name.to_string(),
                            path:         String::new(),
                            ollama_model: name.to_string(),
                            size_mb:      m["size"].as_u64().map(|s| s / (1024 * 1024)).unwrap_or(0),
                            source:       "ollama".to_string(),
                        });
                    }
                }
            }
        }
    }

    models
}

// ─── LLM inference (Rust-native, no Python required) ─────────────────────────

const LLAMA_PORT: u16 = 38765;

// Keep at most `max` segments, sampling evenly to fit within context.
// Always includes first and last segments so boundaries are preserved.
fn sample_segments(segments: &[SrtSegment], max: usize) -> Vec<&SrtSegment> {
    if segments.len() <= max {
        return segments.iter().collect();
    }
    let mut out = Vec::with_capacity(max);
    out.push(&segments[0]);
    let inner_slots = max - 2;
    let step = (segments.len() - 2) as f64 / inner_slots as f64;
    for i in 0..inner_slots {
        let idx = 1 + (i as f64 * step).round() as usize;
        let idx = idx.min(segments.len() - 2);
        out.push(&segments[idx]);
    }
    out.push(&segments[segments.len() - 1]);
    out
}

// ~25 tokens per segment (index + timestamp + ~15 words). 900 segs ≈ 22K tokens,
// leaving ~10K for prompt boilerplate + JSON output within the 32768 ctx window.
const CLASSIFY_SEG_BUDGET: usize = 900;

fn build_classify_prompt(segments: &[SrtSegment], max_secs: u64) -> String {
    let sampled = sample_segments(segments, CLASSIFY_SEG_BUDGET);
    let text: String = sampled.iter()
        .map(|s| format!("[{}] {}: {}", s.index, s.start_time, s.text))
        .collect::<Vec<_>>().join("\n");
    let first = segments.first().map(|s| s.index).unwrap_or(1);
    let last  = segments.last().map(|s| s.index).unwrap_or(1);
    let mins  = segments.last().map(|s| s.end as u64 / 60).unwrap_or(0);
    let sampled_note = if sampled.len() < segments.len() {
        format!(" (sampel {} dari {} segmen)", sampled.len(), segments.len())
    } else { String::new() };
    format!(
        "Transkrip video (~{mins} menit, segmen {first}\u{2013}{last}{sampled_note}):\n\n{text}\n\n\
Pecah video ini menjadi bagian-bagian topik spesifik, masing-masing 10\u{2013}{max_secs} detik. \
Balas HANYA dengan JSON (tanpa teks lain):\n\
{{\"sections\":[\
{{\"name\":\"Nama Spesifik\",\"summary\":\"Ringkasan satu kalimat.\",\"start_index\":{first},\"end_index\":{first}}}\
]}}\n\
Aturan: nama UNIK dan sangat spesifik (max 4 kata), bagian berurutan, mencakup semua segmen dari {first} hingga {last}."
    )
}

fn split_long_sections(sections: Vec<Section>, segments: &[SrtSegment], max_secs: f64) -> Vec<Section> {
    let seg_map: std::collections::HashMap<usize, &SrtSegment> =
        segments.iter().map(|s| (s.index, s)).collect();
    let seg_time = |idx: usize| -> f64 {
        seg_map.get(&idx).map(|s| s.start).unwrap_or(0.0)
    };
    let seg_end = |idx: usize| -> f64 {
        seg_map.get(&idx).map(|s| s.end).unwrap_or(0.0)
    };

    let mut out: Vec<Section> = Vec::new();
    for sec in sections {
        let duration = seg_end(sec.end_index) - seg_time(sec.start_index);
        if duration <= max_secs {
            out.push(sec);
            continue;
        }
        // Split into N parts
        let segs_in: Vec<&SrtSegment> = segments.iter()
            .filter(|s| s.index >= sec.start_index && s.index <= sec.end_index)
            .collect();
        if segs_in.is_empty() { out.push(sec); continue; }

        let n_parts = ((duration / max_secs).ceil() as usize).max(2);
        let chunk = (segs_in.len() + n_parts - 1) / n_parts;
        let total = (segs_in.len() + chunk - 1) / chunk;

        for (i, chunk_segs) in segs_in.chunks(chunk).enumerate() {
            let part_num = i + 1;
            out.push(Section {
                name: format!("{} {}/{}", sec.name, part_num, total),
                summary: sec.summary.clone(),
                start_index: chunk_segs.first().unwrap().index,
                end_index: chunk_segs.last().unwrap().index,
            });
        }
    }
    out
}

fn build_analyze_prompt(segments: &[SrtSegment]) -> String {
    let segs = sample_segments(segments, 300);
    let text: String = segs.iter()
        .map(|s| format!("[{}] {}: {}", s.index, s.start_time, s.text))
        .collect::<Vec<_>>().join("\n");
    format!(
        "Transkrip video:\n\n{text}\n\n\
Pilih segmen-segmen paling penting dari video ini. \
Balas HANYA dengan JSON (tanpa teks lain):\n\
{{\"important_indices\": [1, 3, 5], \"reasoning\": \"alasan singkat\"}}"
    )
}

async fn infer_ollama_api(model: &str, prompt: &str, max_tokens: u32) -> Result<String, String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
        "format": "json",
        "options": { "num_predict": max_tokens },
    });
    let resp = reqwest::Client::new()
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .timeout(std::time::Duration::from_secs(180))
        .send().await
        .map_err(|e| format!("Gagal terhubung ke Ollama: {e}. Pastikan Ollama berjalan."))?;
    if !resp.status().is_success() {
        return Err(format!("Ollama error: HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Gagal parse response Ollama: {e}"))?;
    json["message"]["content"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Response Ollama kosong".to_string())
}

fn kill_port_process(port: u16) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("sh")
            .args(["-c", &format!("lsof -ti tcp:{port} | xargs kill -9 2>/dev/null")])
            .output();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", &format!(
                "for /f \"tokens=5\" %a in ('netstat -aon ^| findstr :{port}') do taskkill /F /PID %a"
            )])
            .output();
    }
}

async fn ensure_llama_server(
    app: &tauri::AppHandle,
    state: &LlamaServerState,
    model_path: &str,
) -> Result<(), String> {
    let _lock = state.startup_lock.lock().await; // serialize concurrent starts

    let already_running = state.current_model.lock().unwrap().as_str() == model_path;
    if already_running { return Ok(()); }

    // Kill our tracked child and any stale external process on the same port
    {
        let mut g = state.process.lock().unwrap();
        if let Some(mut child) = g.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    kill_port_process(LLAMA_PORT);
    { state.current_model.lock().unwrap().clear(); }

    let binary = find_llama_server(app).ok_or_else(||
        "llama-server binary tidak ditemukan. \
        Jalankan: python3 scripts/download_llama_server.py".to_string()
    )?;

    // Use all physical threads (leave 1 for OS), capped at 16
    let cpu_threads = std::thread::available_parallelism()
        .map(|n| (n.get().saturating_sub(1)).max(1).min(16))
        .unwrap_or(4)
        .to_string();

    let mut args = vec![
        "--model".to_string(), model_path.to_string(),
        "--port".to_string(), LLAMA_PORT.to_string(),
        "--ctx-size".to_string(), "8192".to_string(),
        "--threads".to_string(), cpu_threads,
        "--parallel".to_string(), "1".to_string(),
        "--log-disable".to_string(),
    ];
    #[cfg(target_os = "macos")]
    { args.push("-ngl".to_string()); args.push("999".to_string()); } // Metal GPU offload

    // Pipe stderr so we can include it in the timeout error message
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf_clone = Arc::clone(&stderr_buf);

    let mut child = std::process::Command::new(&binary)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan llama-server: {e}"))?;

    // Drain stderr in background thread so the pipe never blocks
    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let mut g = stderr_buf_clone.lock().unwrap();
                g.push_str(&line);
                g.push('\n');
                // Keep only the last 4 KB to avoid unbounded growth
                if g.len() > 4096 {
                    let trim = g.len() - 4096;
                    *g = g[trim..].to_string();
                }
            }
        })
    });

    { *state.process.lock().unwrap() = Some(child); }

    // Poll health endpoint — up to 180 s (large models + GPU layer loading can be slow)
    let health = format!("http://localhost:{LLAMA_PORT}/health");
    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..180 {
        if client.get(&health).timeout(std::time::Duration::from_secs(1))
            .send().await.map(|r| r.status().is_success()).unwrap_or(false)
        {
            ready = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    if !ready {
        let mut g = state.process.lock().unwrap();
        if let Some(mut c) = g.take() { let _ = c.kill(); let _ = c.wait(); }
        drop(g);
        if let Some(h) = stderr_handle { let _ = h.join(); }
        let log = stderr_buf.lock().unwrap();
        let log_tail: String = log.lines().rev().take(6).collect::<Vec<_>>()
            .into_iter().rev().collect::<Vec<_>>().join("\n");
        let detail = if log_tail.is_empty() {
            String::new()
        } else {
            format!("\n\nLog llama-server:\n{log_tail}")
        };
        return Err(format!(
            "llama-server gagal start dalam 180 detik. \
            Coba model yang lebih kecil atau periksa apakah file .gguf valid.{detail}"
        ));
    }

    *state.current_model.lock().unwrap() = model_path.to_string();
    Ok(())
}

async fn infer_llama_server(prompt: &str, max_tokens: u32) -> Result<String, String> {
    let body = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "response_format": {"type": "json_object"},
        "temperature": 0.1,
        "max_tokens": max_tokens,
    });
    let resp = reqwest::Client::new()
        .post(format!("http://localhost:{LLAMA_PORT}/v1/chat/completions"))
        .json(&body)
        .timeout(std::time::Duration::from_secs(180))
        .send().await
        .map_err(|e| format!("Gagal menghubungi llama-server: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("llama-server error: HTTP {status} — {body}"));
    }
    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Gagal parse response: {e}"))?;
    json["choices"][0]["message"]["content"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Response tidak valid: {json}"))
}

async fn run_llm_prompt(
    app: &tauri::AppHandle,
    server_state: &LlamaServerState,
    prompt: &str,
    model_path: &str,
    ollama_model: &str,
    max_tokens: u32,
) -> Result<String, String> {
    let resolved_path = if !model_path.is_empty() && Path::new(model_path).exists() {
        Some(model_path.to_string())
    } else if model_path.is_empty() {
        find_llm_model(app)
    } else {
        None
    };

    if let Some(mp) = resolved_path {
        ensure_llama_server(app, server_state, &mp).await?;
        infer_llama_server(prompt, max_tokens).await
    } else {
        let om = if ollama_model.is_empty() { "gemma4:latest" } else { ollama_model };
        infer_ollama_api(om, prompt, max_tokens).await.map_err(|e|
            format!("{e}\n\nUntuk menggunakan model lokal: unduh model GGUF via menu '🤖 Model AI'.")
        )
    }
}

// ─── Video helpers ────────────────────────────────────────────────────────────

fn get_video_dims(video_path: &str, vendor: Option<&Path>) -> Result<(u32, u32), String> {
    let ffprobe = find_ffprobe(vendor);
    let out = Command::new(&ffprobe)
        .args(["-v", "quiet", "-print_format", "json",
               "-show_streams", "-select_streams", "v:0", video_path])
        .output()
        .map_err(|e| format!("ffprobe gagal: {e}"))?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("ffprobe parse error: {e}"))?;
    let w = json["streams"][0]["width"].as_u64().ok_or("no width")? as u32;
    let h = json["streams"][0]["height"].as_u64().ok_or("no height")? as u32;
    Ok((w, h))
}

fn build_crop_filter(ratio: &str, src_w: u32, src_h: u32) -> Option<String> {
    if ratio == "9:16-fit" {
        // Scale source video to fit inside portrait 9:16 frame, centered (letterbox)
        let long_side = src_w.max(src_h);
        let out_h = long_side & !1;
        let out_w = ((long_side as f64 * 9.0 / 16.0) as u32) & !1;
        return Some(format!(
            "scale={out_w}:{out_h}:force_original_aspect_ratio=decrease,pad={out_w}:{out_h}:(ow-iw)/2:(oh-ih)/2,setsar=1"
        ));
    }
    let (tgt_aw, tgt_ah): (f64, f64) = match ratio {
        "9:16" => (9.0, 16.0), "16:9" => (16.0, 9.0),
        "1:1"  => (1.0, 1.0),  "4:5"  => (4.0, 5.0),
        _ => return None,
    };
    let tgt_ar = tgt_aw / tgt_ah;
    let src_ar = src_w as f64 / src_h as f64;
    let (crop_w, crop_h, cx, cy) = if src_ar > tgt_ar {
        let cw = ((src_h as f64 * tgt_ar) as u32) & !1;
        let ch = src_h & !1;
        (cw, ch, (src_w - cw) / 2, 0)
    } else {
        let cw = src_w & !1;
        let ch = ((src_w as f64 / tgt_ar) as u32) & !1;
        (cw, ch, 0, (src_h - ch) / 2)
    };
    Some(format!("crop={crop_w}:{crop_h}:{cx}:{cy}"))
}


// ─── Clip grouping ────────────────────────────────────────────────────────────
//
// Consecutive segments (gap ≤ 0.5 s) are merged into a single extraction range.
// Timestamps are taken directly from the SRT data (exact float seconds) — no
// floor/ceil rounding — so FFmpeg cuts precisely at the transcribed boundaries
// and no content can repeat across group boundaries.

struct ClipGroup<'a> {
    segs: Vec<&'a SrtSegment>,
    start_sec: f64, // exact start of first segment
    end_sec: f64,   // exact end of last segment
}

impl ClipGroup<'_> {
    fn duration(&self) -> f64 { self.end_sec - self.start_sec }
}

fn group_segments<'a>(selected: &[&'a SrtSegment]) -> Vec<ClipGroup<'a>> {
    if selected.is_empty() { return Vec::new(); }

    let mut groups: Vec<ClipGroup<'a>> = Vec::new();
    let mut cur: Vec<&'a SrtSegment> = vec![selected[0]];

    // Group by consecutive SRT index, not by time gap.
    // Segments 10–53 (all selected from one "bagian") → ONE clip from
    // first.start to last.end regardless of silence gaps between segments.
    // Non-adjacent indices (e.g. user picks seg 5 and seg 50) → separate clips.
    for seg in &selected[1..] {
        if seg.index == cur.last().unwrap().index + 1 {
            cur.push(seg);
        } else {
            let s = cur.first().unwrap().start;
            let e = cur.last().unwrap().end;
            groups.push(ClipGroup { segs: cur, start_sec: s, end_sec: e });
            cur = vec![seg];
        }
    }
    let s = cur.first().unwrap().start;
    let e = cur.last().unwrap().end;
    groups.push(ClipGroup { segs: cur, start_sec: s, end_sec: e });
    groups
}

// Subtitle entries retimed to the merged clip timeline.
// subtitle_mode: "translated_only" | "bilingual" | "original_only"
// original_by_index: map from segment.index → original text, used for bilingual / original_only
fn build_retimed_entries(
    groups: &[ClipGroup],
    subtitle_mode: &str,
    original_by_index: &std::collections::HashMap<usize, String>,
) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();
    let mut cursor = 0.0_f64;
    for group in groups {
        let g_start = group.start_sec;
        let g_dur   = group.duration();
        for seg in &group.segs {
            let sub_start = (cursor + (seg.start - g_start)).max(0.0);
            let sub_end   = (cursor + (seg.end   - g_start)).min(cursor + g_dur);
            let text = match subtitle_mode {
                "original_only" => original_by_index
                    .get(&seg.index)
                    .map(|s| s.as_str())
                    .unwrap_or(&seg.text)
                    .to_string(),
                "bilingual" => {
                    let orig = original_by_index
                        .get(&seg.index)
                        .map(|s| s.as_str())
                        .unwrap_or(&seg.text);
                    if orig == seg.text {
                        // Not actually translated — show once
                        seg.text.clone()
                    } else {
                        format!("{}\n{}", orig, seg.text)
                    }
                }
                _ => seg.text.clone(), // "translated_only" or default
            };
            entries.push(serde_json::json!({ "start": sub_start, "end": sub_end, "text": text }));
        }
        cursor += g_dur;
    }
    entries
}

// Single group — always transcode so that the seek is frame-accurate.
// Stream copy with fast input seek snaps to the nearest keyframe which can
// include a few frames of content before the intended start, causing repeats
// when the clip is played back after concatenation.
async fn encode_one_group(
    app: &tauri::AppHandle,
    ffmpeg: &str, video_path: &str, group: &ClipGroup<'_>,
    crop_filter: Option<&str>, dest: &str,
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let ss  = format!("{:.6}", group.start_sec);
    let dur = format!("{:.6}", group.duration());
    let total_us = (group.duration() * 1_000_000.0) as i64;

    let mut args = vec![
        "-y".to_string(),
        "-ss".to_string(), ss,
        "-i".to_string(),  video_path.to_string(),
        "-t".to_string(),  dur,
    ];
    if let Some(crop) = crop_filter {
        args.extend(["-vf".to_string(), crop.to_string()]);
    }
    args.extend([
        "-map".to_string(), "0:v:0".to_string(),
        "-map".to_string(), "0:a:0?".to_string(),
        "-c:v".to_string(), "libx264".to_string(),
        "-preset".to_string(), "fast".to_string(),
        "-crf".to_string(), "23".to_string(),
        "-threads".to_string(), "0".to_string(),
        "-c:a".to_string(), "aac".to_string(),
        "-b:a".to_string(), "128k".to_string(),
        "-progress".to_string(), "pipe:1".to_string(),
        "-nostats".to_string(),
        dest.to_string(),
    ]);

    let mut child = TokioCommand::new(ffmpeg)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan FFmpeg: {e}"))?;
    if let Some(pid) = child.id() { *pid_cell.lock().unwrap() = Some(pid); }

    // Collect stderr for error reporting
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut s = String::new();
            let _ = tokio::io::BufReader::new(stderr).read_to_string(&mut s).await;
            s
        })
    });

    if let Some(stdout) = child.stdout.take() {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(val) = line.strip_prefix("out_time_ms=") {
                    if let Ok(us) = val.trim().parse::<i64>() {
                        if total_us > 0 && us >= 0 {
                            let pct = ((us as f64 / total_us as f64) * 100.0).clamp(0.0, 99.0) as u8;
                            let _ = app_clone.emit("clip-concat-percent", pct);
                        }
                    }
                }
            }
        });
    }

    let status = child.wait().await
        .map_err(|e| format!("Gagal menunggu FFmpeg: {e}"))?;
    *pid_cell.lock().unwrap() = None;
    let _ = app.emit("clip-concat-percent", 100u8);

    if status.success() {
        Ok(())
    } else {
        let stderr_out = if let Some(t) = stderr_task {
            t.await.unwrap_or_default()
        } else { String::new() };
        let tail: String = stderr_out.lines()
            .filter(|l| !l.trim().is_empty())
            .rev().take(6).collect::<Vec<_>>()
            .into_iter().rev().collect::<Vec<_>>().join("\n");
        Err(format!("FFmpeg gagal mengekstrak klip.\n{tail}"))
    }
}

// Multiple groups: build ONE FFmpeg command with N inputs (each fast-seeked) and
// a filter_complex concat. This produces correct timestamps without temp files
// and avoids the 2x-speed bug caused by stream-copy + concat demuxer.
async fn concat_groups(
    app: &tauri::AppHandle,
    ffmpeg: &str, video_path: &str, groups: &[ClipGroup<'_>],
    crop_filter: Option<&str>, dest: &str,
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
    if groups.len() == 1 {
        return encode_one_group(app, ffmpeg, video_path, &groups[0], crop_filter, dest, pid_cell).await;
    }

    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let total_us = (groups.iter().map(|g| g.duration()).sum::<f64>() * 1_000_000.0) as i64;
    let n = groups.len();
    let mut args: Vec<String> = vec!["-y".to_string()];

    // One input per group: fast seek to start, then limit by duration.
    // Using -t (duration) instead of -to avoids accumulated rounding drift
    // across groups when filter_complex stitches them together.
    for group in groups {
        args.extend([
            "-ss".to_string(), format!("{:.6}", group.start_sec),
            "-t".to_string(),  format!("{:.6}", group.duration()),
            "-i".to_string(),  video_path.to_string(),
        ]);
    }

    // Build filter_complex: optionally crop each video stream, then concat all
    let filter = if let Some(crop) = crop_filter {
        let crops: String = (0..n)
            .map(|i| format!("[{i}:v]{crop}[v{i}]"))
            .collect::<Vec<_>>().join("; ");
        let inputs: String = (0..n)
            .map(|i| format!("[v{i}][{i}:a]"))
            .collect::<Vec<_>>().join("");
        format!("{crops}; {inputs}concat=n={n}:v=1:a=1[v][a]")
    } else {
        let inputs: String = (0..n)
            .map(|i| format!("[{i}:v][{i}:a]"))
            .collect::<Vec<_>>().join("");
        format!("{inputs}concat=n={n}:v=1:a=1[v][a]")
    };

    args.extend([
        "-filter_complex".to_string(), filter,
        "-map".to_string(), "[v]".to_string(),
        "-map".to_string(), "[a]".to_string(),
        "-c:v".to_string(), "libx264".to_string(),
        "-preset".to_string(), "fast".to_string(),
        "-crf".to_string(), "23".to_string(),
        "-threads".to_string(), "0".to_string(),
        "-c:a".to_string(), "aac".to_string(),
        "-b:a".to_string(), "128k".to_string(),
        "-progress".to_string(), "pipe:1".to_string(),
        "-nostats".to_string(),
        dest.to_string(),
    ]);

    let mut child = TokioCommand::new(ffmpeg)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan FFmpeg: {e}"))?;
    if let Some(pid) = child.id() { *pid_cell.lock().unwrap() = Some(pid); }

    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut s = String::new();
            let _ = tokio::io::BufReader::new(stderr).read_to_string(&mut s).await;
            s
        })
    });

    if let Some(stdout) = child.stdout.take() {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(val) = line.strip_prefix("out_time_ms=") {
                    if let Ok(us) = val.trim().parse::<i64>() {
                        if total_us > 0 && us >= 0 {
                            let pct = ((us as f64 / total_us as f64) * 100.0).clamp(0.0, 99.0) as u8;
                            let _ = app_clone.emit("clip-concat-percent", pct);
                        }
                    }
                }
            }
        });
    }

    let status = child.wait().await
        .map_err(|e| format!("Gagal menunggu FFmpeg: {e}"))?;
    *pid_cell.lock().unwrap() = None;
    let _ = app.emit("clip-concat-percent", 100u8);

    if status.success() {
        Ok(())
    } else {
        let stderr_out = if let Some(t) = stderr_task {
            t.await.unwrap_or_default()
        } else { String::new() };
        let tail: String = stderr_out.lines()
            .filter(|l| !l.trim().is_empty())
            .rev().take(6).collect::<Vec<_>>()
            .into_iter().rev().collect::<Vec<_>>().join("\n");
        Err(format!("FFmpeg gagal menggabungkan klip.\n{tail}"))
    }
}

// ─── Translation helpers ──────────────────────────────────────────────────────

fn lang_display_name(code: &str) -> &str {
    match code {
        "id" => "Indonesian",  "en" => "English",    "ja" => "Japanese",
        "zh" => "Chinese",     "ko" => "Korean",      "es" => "Spanish",
        "fr" => "French",      "de" => "German",      "ar" => "Arabic",
        "pt" => "Portuguese",  "ru" => "Russian",     "hi" => "Hindi",
        "th" => "Thai",        "vi" => "Vietnamese",  "it" => "Italian",
        "nl" => "Dutch",       "tr" => "Turkish",     "pl" => "Polish",
        _ => code,
    }
}

fn build_srt(segments: &[SrtSegment]) -> String {
    segments.iter().map(|s| {
        format!("{}\n{} --> {}\n{}\n", s.index, s.start_time, s.end_time, s.text)
    }).collect::<Vec<_>>().join("\n")
}

fn build_translate_prompt(input_json: &str, src: &str, tgt: &str) -> String {
    format!(
        "Translate the following subtitle texts from {src} to {tgt}.\n\
        Return ONLY a JSON array of translated strings, same count and order as input.\n\
        No explanation, no extra text — only the JSON array.\n\n\
        Input: {input_json}\n\nOutput:"
    )
}

fn extract_json_object(s: &str) -> &str {
    let start = s.find('{').unwrap_or(0);
    let end   = s.rfind('}').map(|i| i + 1).unwrap_or(s.len());
    &s[start..end]
}

/// Repair a truncated classify JSON by discarding the last incomplete section
/// and closing the `{"sections":[...]}` structure properly.
fn repair_truncated_classify_json(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    // Track position right after the last complete section object closes.
    // JSON structure: depth 1 = outer {}, depth 2 = sections [], depth 3 = section {}
    let mut last_complete_end: usize = 0;

    for (i, &b) in bytes.iter().enumerate() {
        if escaped { escaped = false; continue; }
        if b == b'\\' && in_string { escaped = true; continue; }
        if b == b'"' { in_string = !in_string; continue; }
        if in_string { continue; }
        match b {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                let prev = depth;
                depth -= 1;
                // depth was 3→2: just closed a section object inside the array
                if prev == 3 && b == b'}' {
                    last_complete_end = i + 1;
                }
            }
            _ => {}
        }
    }

    if last_complete_end == 0 {
        return s.to_string();
    }
    // Trim trailing comma/whitespace after the last complete section, then close
    let trimmed = s[..last_complete_end].trim_end_matches(|c: char| c == ',' || c.is_whitespace());
    format!("{}]}}", trimmed)
}

fn extract_first_json_array(content: &str) -> Option<&str> {
    let start = content.find('[')?;
    let bytes = content.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes[start..].iter().enumerate() {
        if escaped { escaped = false; continue; }
        if b == b'\\' && in_string { escaped = true; continue; }
        if b == b'"' { in_string = !in_string; continue; }
        if in_string { continue; }
        match b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&content[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_translate_array(content: &str, originals: &[String]) -> Result<Vec<String>, String> {
    let expected_len = originals.len();
    let json_slice = extract_first_json_array(content)
        .ok_or_else(|| format!("JSON array tidak ditemukan di response: {content}"))?;
    let mut arr: Vec<String> = serde_json::from_str(json_slice)
        .map_err(|e| format!("Gagal parse array terjemahan: {e}\nContent: {content}"))?;

    if arr.is_empty() {
        return Err(format!("LLM mengembalikan array kosong.\nContent: {content}"));
    }

    // Gracefully handle count mismatch: pad with originals or truncate
    match arr.len().cmp(&expected_len) {
        std::cmp::Ordering::Less => {
            // Pad missing entries with original (untranslated) text
            arr.extend(originals[arr.len()..].iter().cloned());
        }
        std::cmp::Ordering::Greater => {
            arr.truncate(expected_len);
        }
        std::cmp::Ordering::Equal => {}
    }
    Ok(arr)
}

#[tauri::command]
pub async fn translate_transcript(
    app: tauri::AppHandle,
    server: tauri::State<'_, LlamaServerState>,
    segments: Vec<SrtSegment>,
    source_language: String,
    target_language: String,
    model_path: String,
    ollama_model: String,
) -> Result<TranslateResult, String> {
    // Resolve backend — same logic as classify/analyze
    let use_llama = {
        let resolved = if !model_path.is_empty() && Path::new(&model_path).exists() {
            Some(model_path.clone())
        } else if model_path.is_empty() {
            find_llm_model(&app)
        } else {
            None
        };
        if let Some(mp) = resolved {
            ensure_llama_server(&app, &*server, &mp).await?;
            true
        } else {
            false
        }
    };
    let ollama_model_str = if ollama_model.is_empty() { "gemma4:latest" } else { &ollama_model };

    let all_texts: Vec<String> = segments.iter().map(|s| s.text.clone()).collect();
    let mut translated: Vec<String> = Vec::new();

    for chunk in all_texts.chunks(15) {
        let src = lang_display_name(&source_language);
        let tgt = lang_display_name(&target_language);
        let prompt = build_translate_prompt(&serde_json::to_string(chunk).unwrap(), src, tgt);

        let content = if use_llama {
            infer_llama_server(&prompt, 2048).await?
        } else {
            infer_ollama_api(ollama_model_str, &prompt, 2048).await.map_err(|e|
                format!("{e}\n\nUntuk menggunakan model lokal: unduh model GGUF via menu '🤖 Model AI'.")
            )?
        };

        translated.extend(parse_translate_array(&content, chunk)?);
    }

    let mut new_segments = segments;
    for (seg, text) in new_segments.iter_mut().zip(translated) {
        seg.text = text;
    }

    let srt_content = build_srt(&new_segments);
    Ok(TranslateResult { segments: new_segments, srt_content })
}

// ─── System font discovery ────────────────────────────────────────────────────

fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/Library/Fonts")));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/.fonts")));
            dirs.push(PathBuf::from(format!("{home}/.local/share/fonts")));
        }
    }
    #[cfg(target_os = "windows")]
    {
        dirs.push(PathBuf::from("C:\\Windows\\Fonts"));
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(format!("{appdata}\\Microsoft\\Windows\\Fonts")));
        }
    }

    dirs
}

fn scan_font_dir(dir: &Path, fonts: &mut Vec<FontInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_font_dir(&path, fonts);
        } else if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_lowercase();
            if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
                let name = path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !name.is_empty() {
                    fonts.push(FontInfo { name, path: path.to_string_lossy().to_string() });
                }
            }
        }
    }
}

#[tauri::command]
pub async fn get_system_fonts() -> Vec<FontInfo> {
    let mut fonts: Vec<FontInfo> = Vec::new();
    for dir in system_font_dirs() {
        scan_font_dir(&dir, &mut fonts);
    }
    // Deduplicate by name (keep first occurrence), then sort
    let mut seen = std::collections::HashSet::new();
    fonts.retain(|f| seen.insert(f.name.clone()));
    fonts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    fonts
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn read_font_base64(path: String) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = tokio::fs::read(&path).await
        .map_err(|e| format!("Tidak dapat membaca file font: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

#[tauri::command]
pub async fn check_dependencies(app: tauri::AppHandle) -> DepsStatus {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let bundled = vendor.is_some();
    let source = if bundled { "bundled" } else { "system" };

    // Platform-specific install commands
    #[cfg(target_os = "macos")]
    let (py_install, ff_install) = ("brew install python", "brew install ffmpeg");
    #[cfg(target_os = "linux")]
    let (py_install, ff_install) = (
        "sudo apt install python3  # Ubuntu\nsudo dnf install python3  # Fedora",
        "sudo apt install ffmpeg   # Ubuntu\nsudo dnf install ffmpeg   # Fedora",
    );
    #[cfg(target_os = "windows")]
    let has_choco = which("choco").is_some();
    #[cfg(target_os = "windows")]
    let (py_install, ff_install) = if has_choco {
        ("choco install python -y", "choco install ffmpeg -y")
    } else {
        ("winget install Python.Python.3 -e --source winget", "winget install Gyan.FFmpeg -e --source winget")
    };

    // Virtual environment — pip inside venv never has PEP 668 issues
    let venv = venv_dir();
    let venv_str = venv.to_string_lossy().to_string();
    let venv_exists = venv_python().is_some();
    #[cfg(target_os = "windows")]
    let pip_in_venv = format!("{}\\Scripts\\pip.exe", venv_str);
    #[cfg(not(target_os = "windows"))]
    let pip_in_venv = format!("{}/bin/pip", venv_str);

    // Helper: install a package. If venv is ready, use its pip directly.
    // If not, prepend venv creation so the user gets both in one terminal run.
    let pip_install = |pkg: &str| -> String {
        if venv_exists {
            format!("{pip_in_venv} install {pkg}")
        } else {
            #[cfg(not(target_os = "windows"))]
            return format!("python3 -m venv \"{venv_str}\" && \"{pip_in_venv}\" install {pkg}");
            #[cfg(target_os = "windows")]
            return format!("python -m venv \"{venv_str}\" && \"{pip_in_venv}\" install {pkg}");
        }
    };

    let mut checks: Vec<DepCheck> = Vec::new();

    // Venv status — shown first so user knows to create it before installing packages
    checks.push(DepCheck {
        name: "Python Environment (venv)".to_string(),
        ok: venv_exists,
        path: if venv_exists { Some(venv_str.clone()) } else { None },
        error: if !venv_exists {
            Some(format!("Virtual environment belum dibuat. Klik 'Buat Environment' agar instalasi package tidak bertabrakan dengan Python sistem."))
        } else { None },
        install_cmd: if !venv_exists { Some("__create_venv__".to_string()) } else { None },
        download_url: None,
        optional: true,
    });

    let python = find_python(v);
    let python_ok = Command::new(&python).arg("--version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("Python 3 ({source})"),
        ok: python_ok,
        path: if python_ok { Some(python.clone()) } else { None },
        error: if !python_ok { Some("Python 3 tidak ditemukan".to_string()) } else { None },
        install_cmd: if !python_ok { Some(py_install.to_string()) } else { None },
        download_url: if !python_ok { Some("https://www.python.org/downloads/".to_string()) } else { None },
        optional: false,
    });

    // When venv exists, use `pip show` — reliable regardless of Python ABI/compat.
    // When no venv, fall back to `python -c "import pkg"`.
    let pkg_check = |import_name: &str, pip_name: &str| -> bool {
        if venv_exists {
            Command::new(&pip_in_venv).args(["show", pip_name]).output()
                .map(|o| o.status.success()).unwrap_or(false)
        } else {
            python_ok && Command::new(&python)
                .args(["-c", &format!("import {import_name}")])
                .output().map(|o| o.status.success()).unwrap_or(false)
        }
    };

    let ffmpeg = find_ffmpeg(v);
    let ffmpeg_ok = Command::new(&ffmpeg).arg("-version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("FFmpeg ({source})"),
        ok: ffmpeg_ok,
        path: if ffmpeg_ok { Some(ffmpeg.clone()) } else { None },
        error: if !ffmpeg_ok { Some("FFmpeg tidak ditemukan".to_string()) } else { None },
        install_cmd: if !ffmpeg_ok { Some(ff_install.to_string()) } else { None },
        download_url: None,
        optional: false,
    });

    let ffprobe = find_ffprobe(v);
    let ffprobe_ok = Command::new(&ffprobe).arg("-version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("ffprobe ({source})"),
        ok: ffprobe_ok,
        path: if ffprobe_ok { Some(ffprobe) } else { None },
        error: if !ffprobe_ok { Some("ffprobe tidak ditemukan".to_string()) } else { None },
        install_cmd: if !ffprobe_ok { Some(ff_install.to_string()) } else { None },
        download_url: None,
        optional: false,
    });

    let whisper_ok = pkg_check("faster_whisper", "faster-whisper");
    checks.push(DepCheck {
        name: format!("faster-whisper ({source})"),
        ok: whisper_ok,
        path: None,
        error: if !whisper_ok { Some("Package faster-whisper belum terinstall".to_string()) } else { None },
        install_cmd: if !whisper_ok { Some(pip_install("faster-whisper")) } else { None },
        download_url: None,
        optional: false,
    });

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let mlx_ok = pkg_check("mlx_whisper", "mlx-whisper");
        checks.push(DepCheck {
            name: format!("mlx-whisper / GPU Apple Silicon ({source})"),
            ok: mlx_ok,
            path: None,
            error: if !mlx_ok {
                Some("mlx-whisper belum terinstall — transkrip akan pakai CPU, bukan GPU Apple Silicon. Install untuk akselerasi 5-15×.".to_string())
            } else { None },
            install_cmd: if !mlx_ok { Some(pip_install("mlx-whisper")) } else { None },
            download_url: None,
            optional: true,
        });
    }

    #[cfg(target_os = "windows")]
    {
        let dml_ok = pkg_check("torch_directml", "torch-directml");
        checks.push(DepCheck {
            name: "torch-directml / GPU AMD·Intel·NVIDIA (Windows)".to_string(),
            ok: dml_ok,
            path: None,
            error: if !dml_ok {
                Some("torch-directml belum terinstall — transkrip akan pakai CPU. Install untuk akselerasi GPU AMD/Intel/NVIDIA via DirectML.".to_string())
            } else { None },
            install_cmd: if !dml_ok { Some(pip_install("openai-whisper torch-directml")) } else { None },
            download_url: None,
            optional: true,
        });
    }

    let hf_hub_ok = pkg_check("huggingface_hub", "huggingface-hub");
    checks.push(DepCheck {
        name: format!("huggingface-hub ({source})"),
        ok: hf_hub_ok,
        path: None,
        error: if !hf_hub_ok { Some("Package huggingface-hub belum terinstall — diperlukan untuk download model Whisper".to_string()) } else { None },
        install_cmd: if !hf_hub_ok { Some(pip_install("huggingface_hub")) } else { None },
        download_url: None,
        optional: false,
    });

    let pillow_ok = pkg_check("PIL", "Pillow");
    checks.push(DepCheck {
        name: format!("Pillow ({source})"),
        ok: pillow_ok,
        path: None,
        error: if !pillow_ok { Some("Package Pillow belum terinstall".to_string()) } else { None },
        install_cmd: if !pillow_ok { Some(pip_install("Pillow")) } else { None },
        download_url: None,
        optional: false,
    });

    let transcribe_path = find_script(&app, "transcribe.py");
    let burn_path = find_script(&app, "burn_subtitles.py");
    let scripts_ok = Path::new(&transcribe_path).exists() && Path::new(&burn_path).exists();
    checks.push(DepCheck {
        name: "Scripts".to_string(),
        ok: scripts_ok,
        path: if scripts_ok { Some(transcribe_path) } else { None },
        error: if !scripts_ok { Some("Script files tidak ditemukan".to_string()) } else { None },
        install_cmd: None,
        download_url: None,
        optional: false,
    });

    let llama_server_path = find_llama_server(&app);
    let llama_server_ok = llama_server_path.is_some();
    let llama_script = find_script(&app, "download_llama_server.py");
    checks.push(DepCheck {
        name: "llama-server (AI Lokal)".to_string(),
        ok: llama_server_ok,
        path: llama_server_path,
        error: if !llama_server_ok {
            Some(format!("Binary llama-server tidak ditemukan. Jalankan: {python} \"{llama_script}\""))
        } else { None },
        install_cmd: if !llama_server_ok {
            Some(format!("{python} \"{llama_script}\""))
        } else { None },
        download_url: None,
        optional: true,
    });

    let opencv_ok = pkg_check("cv2", "opencv-python");
    checks.push(DepCheck {
        name: format!("opencv-python (Smart Crop)"),
        ok: opencv_ok,
        path: None,
        error: if !opencv_ok { Some("opencv-python belum terinstall — diperlukan untuk Smart Crop".to_string()) } else { None },
        install_cmd: if !opencv_ok { Some(pip_install("opencv-python")) } else { None },
        download_url: None,
        optional: true,
    });

    let insightface_ok = pkg_check("insightface", "insightface");
    checks.push(DepCheck {
        name: "insightface (Smart Crop — deteksi wajah profil/samping)".to_string(),
        ok: insightface_ok,
        path: None,
        error: if !insightface_ok {
            Some("insightface belum terinstall — Smart Crop tidak bisa mendeteksi wajah dari samping. Install untuk akurasi terbaik.".to_string())
        } else { None },
        install_cmd: if !insightface_ok { Some(pip_install("insightface onnxruntime")) } else { None },
        download_url: None,
        optional: true,
    });

    let mediapipe_ok = pkg_check("mediapipe", "mediapipe");
    checks.push(DepCheck {
        name: "mediapipe (Smart Crop presisi tinggi)".to_string(),
        ok: mediapipe_ok,
        path: None,
        error: if !mediapipe_ok {
            Some("mediapipe belum terinstall — Smart Crop akan pakai detektor cadangan (YuNet/Haar) yang kurang presisi".to_string())
        } else { None },
        install_cmd: if !mediapipe_ok { Some(pip_install("mediapipe")) } else { None },
        download_url: None,
        optional: true,
    });

    let ytdlp_path = find_ytdlp();
    let ytdlp_ok = ytdlp_path.is_some();
    let ytdlp_install = pip_install("yt-dlp");
    checks.push(DepCheck {
        name: "yt-dlp (YouTube Download)".to_string(),
        ok: ytdlp_ok,
        path: ytdlp_path,
        error: if !ytdlp_ok {
            Some("yt-dlp belum terinstall — diperlukan untuk download video dari YouTube".to_string())
        } else { None },
        install_cmd: if !ytdlp_ok { Some(ytdlp_install) } else { None },
        download_url: None,
        optional: true,
    });

    let ollama_ok = reqwest::Client::new()
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(3))
        .send().await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    checks.push(DepCheck {
        name: "Ollama (AI — alternatif)".to_string(),
        ok: ollama_ok,
        path: if ollama_ok { Some("http://localhost:11434".to_string()) } else { None },
        error: if !ollama_ok && !llama_server_ok {
            Some("Tidak ada backend AI aktif. Unduh llama-server (direkomendasikan) atau jalankan Ollama.".to_string())
        } else if !ollama_ok {
            Some("Ollama tidak berjalan (opsional — llama-server sudah tersedia)".to_string())
        } else { None },
        install_cmd: if !ollama_ok && !bundled { Some("ollama serve".to_string()) } else { None },
        download_url: if !ollama_ok && !bundled { Some("https://ollama.com/download".to_string()) } else { None },
        optional: true,
    });

    let all_required_ok = checks.iter().filter(|c| !c.optional).all(|c| c.ok);

    #[cfg(target_os = "macos")]
    let (platform, build_notes) = ("macos".to_string(), vec![]);
    #[cfg(target_os = "windows")]
    let (platform, build_notes) = ("windows".to_string(), vec![]);
    #[cfg(target_os = "linux")]
    let (platform, build_notes) = (
        "linux".to_string(),
        vec![
            "Untuk build di Linux, pastikan system library berikut sudah terinstall:".to_string(),
            "Ubuntu/Debian:  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libssl-dev".to_string(),
            "Fedora/RHEL:    sudo dnf install webkit2gtk4.1-devel gtk3-devel openssl-devel".to_string(),
            "Arch Linux:     sudo pacman -S webkit2gtk-4.1 gtk3 openssl".to_string(),
        ],
    );

    DepsStatus { all_required_ok, checks, platform, build_notes }
}

#[tauri::command]
pub async fn transcribe_video(
    app: tauri::AppHandle,
    cancel: tauri::State<'_, ProcessCancelState>,
    video_path: String,
    source_language: String,
    preset: String,
    max_words_per_sub: u32,
) -> Result<TranscribeResult, String> {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let python = find_python(v);
    let ffmpeg = find_ffmpeg(v);
    let script = find_script(&app, "transcribe.py");

    let mut args = vec![script, video_path];
    if !source_language.is_empty() {
        args.push("--language".to_string());
        args.push(source_language);
    }
    let preset = if preset.is_empty() { "balanced".to_string() } else { preset };
    args.push("--preset".to_string());
    args.push(preset);
    if max_words_per_sub > 0 {
        args.push("--max-words".to_string());
        args.push(max_words_per_sub.to_string());
    }
    if let Some(model_dir) = find_model_dir(v) {
        args.push("--model-dir".to_string());
        args.push(model_dir);
    }

    // Use tokio::process to stream stderr as progress events while waiting for JSON on stdout
    use tokio::process::Command as TokioCommand;
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut child = TokioCommand::new(&python)
        .args(&args)
        .env("AUTOCLIPPER_FFMPEG", &ffmpeg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan Whisper: {e}"))?;
    if let Some(pid) = child.id() { *cancel.transcribe_pid.lock().unwrap() = Some(pid); }

    // Stream stderr: PROGRESS:N → "transcribe-percent" event, other lines → "transcribe-progress"
    if let Some(stderr) = child.stderr.take() {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(pct_str) = line.strip_prefix("PROGRESS:") {
                    if let Ok(pct) = pct_str.trim().parse::<u8>() {
                        let _ = app_clone.emit("transcribe-percent", pct);
                    }
                } else {
                    let _ = app_clone.emit("transcribe-progress", &line);
                }
            }
        });
    }

    let output = child.wait_with_output().await
        .map_err(|e| format!("Whisper process error: {e}"))?;
    *cancel.transcribe_pid.lock().unwrap() = None;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let err: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
        let error = err["error"].as_str().unwrap_or("Transkripsi dibatalkan").to_string();
        let tb = err["traceback"].as_str().unwrap_or("").trim().to_string();
        let msg = if tb.is_empty() { error } else { format!("{error}\n\n{tb}") };
        return Err(msg);
    }
    serde_json::from_str(&stdout)
        .map_err(|e| format!("Gagal parse hasil transkripsi: {e}\nOutput: {stdout}"))
}

#[tauri::command]
pub async fn analyze_transcript(
    app: tauri::AppHandle,
    server: tauri::State<'_, LlamaServerState>,
    segments: Vec<SrtSegment>,
    model_path: String,
    ollama_model: String,
) -> Result<AnalyzeResult, String> {
    let prompt  = build_analyze_prompt(&segments);
    let content = run_llm_prompt(&app, &*server, &prompt, &model_path, &ollama_model, 2048).await?;
    serde_json::from_str(extract_json_object(&content))
        .map_err(|e| format!("Gagal parse JSON dari AI: {e}\nContent: {content}"))
}


#[tauri::command]
pub async fn classify_transcript(
    app: tauri::AppHandle,
    server: tauri::State<'_, LlamaServerState>,
    segments: Vec<SrtSegment>,
    model_path: String,
    ollama_model: String,
    max_section_secs: u64,
) -> Result<ClassifyResult, String> {
    if segments.is_empty() {
        return Err("Tidak ada segmen untuk diklasifikasikan".to_string());
    }
    let first_idx = segments.first().map(|s| s.index).unwrap_or(1);
    let last_idx  = segments.last().map(|s| s.index).unwrap_or(1);
    let max_secs  = max_section_secs.max(20);

    let _ = app.emit("classify-progress", "Mengklasifikasikan bagian video...");
    let prompt  = build_classify_prompt(&segments, max_secs);
    let content = run_llm_prompt(&app, &*server, &prompt, &model_path, &ollama_model, 8192).await?;

    let extracted = extract_json_object(&content);
    let mut result: ClassifyResult = serde_json::from_str(extracted)
        .or_else(|_| {
            // JSON truncated — repair by discarding the last incomplete section
            let repaired = repair_truncated_classify_json(extracted);
            serde_json::from_str(&repaired)
                .map_err(|e| format!("Gagal parse hasil klasifikasi: {e}\nContent: {content}"))
        })?;

    if result.sections.iter().all(|s| s.start_index == 0 && s.end_index == 0) {
        let n = result.sections.len().max(1);
        let step = (segments.len() + n - 1) / n;
        for (i, sec) in result.sections.iter_mut().enumerate() {
            let a = (i * step).min(segments.len() - 1);
            let b = ((i + 1) * step).saturating_sub(1).min(segments.len() - 1);
            sec.start_index = segments[a].index;
            sec.end_index   = segments[b].index;
        }
    }
    for sec in &mut result.sections {
        sec.start_index = sec.start_index.max(first_idx);
        sec.end_index   = sec.end_index.min(last_idx).max(sec.start_index);
    }
    if let Some(s) = result.sections.first_mut() { s.start_index = first_idx; }
    if let Some(s) = result.sections.last_mut()  { s.end_index   = last_idx;  }

    let sections = split_long_sections(result.sections, &segments, max_secs as f64);
    Ok(ClassifyResult { sections })
}

async fn exec_smart_crop(
    app: &tauri::AppHandle,
    python: &str,
    ffmpeg: &str,
    input: &str,
    output: &str,
    aspect_ratio: &str,
    transition: &str,
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let script = find_script(app, "smart_crop.py");
    let mut cmd = TokioCommand::new(python);
    cmd.args([&script, input, output, "--ratio", aspect_ratio, "--transition", transition])
        .env("AUTOCLIPPER_FFMPEG", ffmpeg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn()
        .map_err(|e| format!("Gagal menjalankan smart_crop.py: {e}"))?;
    if let Some(pid) = child.id() { *pid_cell.lock().unwrap() = Some(pid); }

    // Read stderr: forward PROGRESS lines as events, buffer the rest for error reporting
    let stderr_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let app_clone = app.clone();
        let buf = stderr_buf.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(pct_str) = line.strip_prefix("PROGRESS:") {
                    if let Ok(pct) = pct_str.trim().parse::<u8>() {
                        let _ = app_clone.emit("clip-smart-percent", pct);
                    }
                } else {
                    let mut g = buf.lock().unwrap();
                    g.push_str(&line);
                    g.push('\n');
                }
            }
        });
    }

    let output_r = child.wait_with_output().await
        .map_err(|e| format!("Gagal menunggu smart_crop.py: {e}"))?;
    *pid_cell.lock().unwrap() = None;

    if output_r.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output_r.stdout);
        let stderr_captured = stderr_buf.lock().unwrap().clone();
        let src = if !stdout.trim().is_empty() {
            stdout.to_string()
        } else {
            stderr_captured
        };
        let err: serde_json::Value = serde_json::from_str(&src).unwrap_or_default();
        let msg = err["error"].as_str().map(|s| s.to_string())
            .unwrap_or_else(|| src.trim().to_string());
        Err(format!("Smart crop gagal: {msg}"))
    }
}

async fn exec_burn_subs(
    app: &tauri::AppHandle,
    python: &str,
    ffmpeg: &str,
    ffprobe: &str,
    input: &str,
    output_path: &str,
    entries: Vec<serde_json::Value>,
    font_size: u32,
    font_path: &str,
    subtitle_style_json: &str,
    title: &str,
    title_font_size: u32,
    title_color: &str,
    pid_cell: &Mutex<Option<u32>>,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let entries_path = std::env::temp_dir().join("autoclipper_entries.json");
    std::fs::write(&entries_path, serde_json::to_string(&entries).unwrap())
        .map_err(|e| format!("Gagal menulis entries JSON: {e}"))?;

    let script = find_script(app, "burn_subtitles.py");
    let entries_str = entries_path.to_string_lossy().to_string();
    let mut burn_args = vec![script, input.to_string(), entries_str, output_path.to_string()];
    if font_size > 0 {
        burn_args.push("--font-size".to_string());
        burn_args.push(font_size.to_string());
    }
    if !font_path.is_empty() {
        burn_args.push("--font".to_string());
        burn_args.push(font_path.to_string());
    }
    if !subtitle_style_json.is_empty() && subtitle_style_json != "{}" {
        burn_args.push("--style".to_string());
        burn_args.push(subtitle_style_json.to_string());
    }
    if !title.is_empty() {
        burn_args.extend(["--title".to_string(), title.to_string()]);
        burn_args.extend(["--title-font-size".to_string(), title_font_size.to_string()]);
        if !title_color.is_empty() {
            burn_args.extend(["--title-color".to_string(), title_color.to_string()]);
        }
    }

    let mut cmd = TokioCommand::new(python);
    cmd.args(&burn_args)
        .env("AUTOCLIPPER_FFMPEG", ffmpeg)
        .env("AUTOCLIPPER_FFPROBE", ffprobe)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn()
        .map_err(|e| format!("Gagal menjalankan burn_subtitles.py: {e}"))?;
    if let Some(pid) = child.id() { *pid_cell.lock().unwrap() = Some(pid); }

    if let Some(stderr) = child.stderr.take() {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(pct_str) = line.strip_prefix("PROGRESS:") {
                    if let Ok(pct) = pct_str.trim().parse::<u8>() {
                        let _ = app_clone.emit("clip-burn-percent", pct);
                    }
                }
            }
        });
    }

    let output = child.wait_with_output().await
        .map_err(|e| format!("Gagal menunggu burn_subtitles.py: {e}"))?;
    *pid_cell.lock().unwrap() = None;
    let _ = std::fs::remove_file(&entries_path);

    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        let src = if !stdout.trim().is_empty() { stdout } else { stderr_str };
        let err: serde_json::Value = serde_json::from_str(&src).unwrap_or_default();
        let msg = err["error"].as_str().map(|s| s.to_string())
            .unwrap_or_else(|| src.trim().to_string());
        Err(format!("Subtitle burn gagal: {msg}"))
    }
}

#[tauri::command]
pub async fn clip_video(
    app: tauri::AppHandle,
    cancel: tauri::State<'_, ProcessCancelState>,
    video_path: String,
    segments: Vec<SrtSegment>,
    selected_indices: Vec<usize>,
    manual_clips: Vec<ManualClip>,
    output_path: String,
    burn_subtitles: bool,
    subtitle_mode: String,
    original_segments: Vec<SrtSegment>,
    aspect_ratio: String,
    smart_crop: bool,
    smart_crop_transition: String,
    font_size: u32,
    font_path: String,
    subtitle_style_json: String,
    vertical_title: String,
    vertical_title_font_size: u32,
    vertical_title_color: String,
) -> Result<ClipResult, String> {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let ffmpeg = find_ffmpeg(v);
    let ffprobe = find_ffprobe(v);
    let python = find_python(v);

    let idx_set: std::collections::HashSet<usize> = selected_indices.iter().cloned().collect();
    let mut selected: Vec<&SrtSegment> = segments.iter()
        .filter(|s| idx_set.contains(&s.index)).collect();
    selected.sort_by_key(|s| s.index);

    // Build groups from selected segments
    let mut groups: Vec<ClipGroup> = group_segments(&selected);

    // Add manual clips as extra groups
    for mc in &manual_clips {
        if mc.end_sec <= mc.start_sec { continue; }
        let mc_segs: Vec<&SrtSegment> = segments.iter()
            .filter(|s| s.start >= mc.start_sec && s.end <= mc.end_sec)
            .collect();
        groups.push(ClipGroup {
            segs: mc_segs,
            start_sec: mc.start_sec,
            end_sec: mc.end_sec,
        });
    }

    if groups.is_empty() { return Err("Tidak ada segmen atau klip manual yang dipilih".to_string()); }
    groups.sort_by(|a, b| a.start_sec.partial_cmp(&b.start_sec).unwrap());

    let total_segments = selected.len() + manual_clips.len();
    let total_duration: f64 = groups.iter().map(|g| g.duration()).sum();

    // Build original-text lookup for bilingual / original_only modes
    let eff_subtitle_mode = if original_segments.is_empty() { "translated_only" } else { subtitle_mode.as_str() };
    let original_by_index: std::collections::HashMap<usize, String> = original_segments
        .iter().map(|s| (s.index, s.text.clone())).collect();

    // Smart crop: skip FFmpeg center crop; let smart_crop.py handle it instead.
    // "9:16-fit" uses scale+pad (no crop), so smart crop is irrelevant there too.
    let needs_smart = smart_crop && aspect_ratio != "original" && aspect_ratio != "9:16-fit";
    let has_title = !vertical_title.trim().is_empty() && aspect_ratio == "9:16-fit";
    let title_fs = if vertical_title_font_size == 0 { 48 } else { vertical_title_font_size };

    let ffmpeg_crop: Option<String> = if needs_smart || aspect_ratio == "original" {
        None
    } else {
        let (w, h) = get_video_dims(&video_path, v)?;
        build_crop_filter(&aspect_ratio, w, h)
    };

    let pid_cell = &cancel.clip_pid;

    // Only pass title to burn pipeline when the selected ratio actually supports it
    let eff_title       = if has_title { vertical_title.as_str() } else { "" };
    let eff_title_fs    = if has_title { title_fs } else { 0 };
    let eff_title_color = if has_title { vertical_title_color.as_str() } else { "" };

    let needs_burn = burn_subtitles || has_title;

    match (needs_burn, needs_smart) {
        // ── Case 1: no burn, no smart crop ────────────────────────────────
        (false, false) => {
            concat_groups(&app, &ffmpeg, &video_path, &groups, ffmpeg_crop.as_deref(), &output_path, pid_cell).await?;
        }

        // ── Case 2: burn (subtitle and/or title), no smart crop ───────────
        (true, false) => {
            let tmp = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            concat_groups(&app, &ffmpeg, &video_path, &groups, ffmpeg_crop.as_deref(), tmp.to_str().unwrap(), pid_cell).await?;
            let entries = if burn_subtitles { build_retimed_entries(&groups, eff_subtitle_mode, &original_by_index) } else { vec![] };
            let r = exec_burn_subs(&app, &python, &ffmpeg, &ffprobe, tmp.to_str().unwrap(), &output_path, entries, font_size, &font_path, &subtitle_style_json, eff_title, eff_title_fs, eff_title_color, pid_cell).await;
            let _ = std::fs::remove_file(&tmp);
            r?;
        }

        // ── Case 3: smart crop only, no burn ──────────────────────────────
        (false, true) => {
            let tmp = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            concat_groups(&app, &ffmpeg, &video_path, &groups, None, tmp.to_str().unwrap(), pid_cell).await?;
            let r = exec_smart_crop(&app, &python, &ffmpeg, tmp.to_str().unwrap(), &output_path, &aspect_ratio, &smart_crop_transition, pid_cell).await;
            let _ = std::fs::remove_file(&tmp);
            r?;
        }

        // ── Case 4: smart crop + burn ─────────────────────────────────────
        (true, true) => {
            let tmp_concat = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            let tmp_smart  = std::env::temp_dir().join("autoclipper_smart_tmp.mp4");
            concat_groups(&app, &ffmpeg, &video_path, &groups, None, tmp_concat.to_str().unwrap(), pid_cell).await?;
            let r = exec_smart_crop(&app, &python, &ffmpeg, tmp_concat.to_str().unwrap(), tmp_smart.to_str().unwrap(), &aspect_ratio, &smart_crop_transition, pid_cell).await;
            let _ = std::fs::remove_file(&tmp_concat);
            r?;
            let entries = if burn_subtitles { build_retimed_entries(&groups, eff_subtitle_mode, &original_by_index) } else { vec![] };
            let r = exec_burn_subs(&app, &python, &ffmpeg, &ffprobe, tmp_smart.to_str().unwrap(), &output_path, entries, font_size, &font_path, &subtitle_style_json, eff_title, eff_title_fs, eff_title_color, pid_cell).await;
            let _ = std::fs::remove_file(&tmp_smart);
            r?;
        }
    }
    *pid_cell.lock().unwrap() = None;

    let ar_note = if aspect_ratio != "original" {
        if needs_smart { format!(" [{aspect_ratio} smart]") } else { format!(" [{aspect_ratio}]") }
    } else { String::new() };
    let sub_note = if burn_subtitles { " + subtitle" } else { "" };
    let group_note = if groups.len() < total_segments {
        format!(" ({} grup)", groups.len())
    } else { String::new() };
    Ok(ClipResult {
        output_path,
        success: true,
        message: format!(
            "Berhasil menggabungkan {total_segments} segmen{group_note} ({:.1}s){sub_note}{ar_note}",
            total_duration
        ),
        total_segments,
        duration_secs: total_duration,
    })
}

// ─── YouTube download ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn download_youtube(
    app: tauri::AppHandle,
    cancel: tauri::State<'_, YtDownloadState>,
    url: String,
) -> Result<String, String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let ytdlp = find_ytdlp().ok_or_else(|| {
        "yt-dlp tidak ditemukan.\n\
         Install: pip install yt-dlp  (atau: brew install yt-dlp di macOS)".to_string()
    })?;

    // Save to ~/Downloads; fall back to temp if unavailable
    let out_dir = app.path().download_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let out_template = out_dir
        .join("%(title)s [%(id)s].%(ext)s")
        .to_string_lossy()
        .to_string();

    let mut child = TokioCommand::new(&ytdlp)
        .args([
            "--newline", "--no-colors", "--progress",
            "-f", "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]/best",
            "--merge-output-format", "mp4",
            "-o", &out_template,
            "--no-playlist",
            "--print", "after_move:filepath",
            &url,
        ])
        // Force Python to flush output immediately instead of buffering
        .env("PYTHONUNBUFFERED", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan yt-dlp: {e}"))?;

    if let Some(pid) = child.id() { *cancel.pid.lock().unwrap() = Some(pid); }

    // yt-dlp sends ALL output ([download] progress, [Merger], filepath) to stdout.
    // stderr only carries warnings/errors. Read stdout for both progress events
    // and the final filepath printed by --print after_move:filepath.
    let stdout_task = if let Some(stdout) = child.stdout.take() {
        let app_clone = app.clone();
        Some(tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut last = String::new();
            while let Ok(Some(line)) = reader.next_line().await {
                let t = line.trim().to_string();
                if t.is_empty() { continue; }
                let prog = if t.starts_with("[download]") && t.contains('%') {
                    let pct   = parse_yt_percent(&t).unwrap_or(0.0);
                    let total = parse_yt_total(&t);
                    let downloaded = parse_size_bytes(&total)
                        .map(|tb| format_bytes(tb * (pct as f64 / 100.0)))
                        .unwrap_or_default();
                    Some(YtDownloadProgress {
                        percent: pct,
                        speed:   parse_yt_speed(&t),
                        eta:     parse_yt_eta(&t),
                        phase:   "downloading".to_string(),
                        downloaded,
                        total,
                    })
                } else if t.starts_with("[Merger]") || t.contains("Merging formats") {
                    Some(YtDownloadProgress {
                        percent: 99.0, speed: String::new(), eta: String::new(),
                        phase: "merging".to_string(),
                        downloaded: String::new(), total: String::new(),
                    })
                } else { None };
                if let Some(p) = prog {
                    let _ = app_clone.emit("yt-download-progress", p);
                }
                // filepath from --print after_move:filepath: a plain path line (no leading '[')
                if !t.starts_with('[') && !t.starts_with("Deleting") && !t.starts_with("WARNING") {
                    last = t;
                }
            }
            last
        }))
    } else { None };

    // Drain stderr (warnings only) so the pipe doesn't block
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(_)) = reader.next_line().await {}
        });
    }

    let status = child.wait().await
        .map_err(|e| format!("yt-dlp error: {e}"))?;
    *cancel.pid.lock().unwrap() = None;

    if !status.success() {
        return Err("Download dibatalkan atau URL tidak valid. Pastikan URL YouTube benar dan yt-dlp versi terbaru.".to_string());
    }

    let filepath = if let Some(task) = stdout_task {
        task.await.unwrap_or_default()
    } else { String::new() };

    if filepath.is_empty() || !std::path::Path::new(&filepath).exists() {
        return Err("File hasil download tidak ditemukan. Coba perbarui yt-dlp: pip install -U yt-dlp".to_string());
    }

    let _ = app.emit("yt-download-progress", YtDownloadProgress {
        percent: 100.0, speed: String::new(), eta: String::new(), phase: "done".to_string(),
        downloaded: String::new(), total: String::new(),
    });

    Ok(filepath)
}

// ─── LLM download helpers ─────────────────────────────────────────────────────

fn get_llm_download_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    // In dev: save to src-tauri/vendor/llm/ (same dir find_llm_model searches)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent().and_then(|p| p.parent())
            .and_then(|p| p.parent()).and_then(|p| p.parent())
        {
            if root.join("src-tauri").exists() {
                return Ok(root.join("src-tauri").join("vendor").join("llm"));
            }
        }
    }
    // Production: app data dir
    app.path().app_data_dir()
        .map(|p| p.join("models"))
        .map_err(|e| format!("Gagal mendapatkan direktori data: {e}"))
}

#[tauri::command]
pub async fn download_llm_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, DownloadState>,
    url: String,
    filename: String,
    hf_token: String,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;

    let dir = get_llm_download_dir(&app)?;
    tokio::fs::create_dir_all(&dir).await
        .map_err(|e| format!("Gagal membuat folder model: {e}"))?;

    { state.0.lock().unwrap().remove(&filename); }

    let dest = dir.join(&filename);
    let tmp  = dir.join(format!("{filename}.part"));

    // Check for existing partial download to resume
    let already = tokio::fs::metadata(&tmp).await.map(|m| m.len()).unwrap_or(0);

    let mut builder = reqwest::Client::new().get(&url);
    if !hf_token.is_empty() {
        builder = builder.header("Authorization", format!("Bearer {hf_token}"));
    }
    if already > 0 {
        builder = builder.header("Range", format!("bytes={already}-"));
    }

    let mut resp = builder.send().await
        .map_err(|e| format!("Gagal terhubung ke server: {e}"))?;

    let status = resp.status();

    // 206 Partial Content: server supports resume
    // 200 OK:              server ignored Range; restart from scratch
    let (mut downloaded, total, mut file) = if status == reqwest::StatusCode::PARTIAL_CONTENT && already > 0 {
        let total = resp.headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split('/').last())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let f = tokio::fs::OpenOptions::new()
            .write(true).append(true)
            .open(&tmp).await
            .map_err(|e| format!("Gagal membuka file untuk dilanjutkan: {e}"))?;
        (already, total, f)
    } else if status.is_success() {
        let total = resp.content_length().unwrap_or(0);
        let f = tokio::fs::File::create(&tmp).await
            .map_err(|e| format!("Gagal membuat file sementara: {e}"))?;
        (0u64, total, f)
    } else {
        return Err(if status.as_u16() == 401 || status.as_u16() == 403 {
            "Akses ditolak — masukkan token HuggingFace yang valid dan pastikan sudah menyetujui lisensi model di halaman HuggingFace.".to_string()
        } else {
            format!("Server mengembalikan error: HTTP {status}")
        });
    };

    // Emit initial progress so UI shows the resumed offset immediately
    if downloaded > 0 {
        let pct = if total > 0 { downloaded as f64 / total as f64 * 100.0 } else { 0.0 };
        let _ = app.emit("llm-download-progress", serde_json::json!({
            "filename": filename, "downloaded": downloaded,
            "total": total, "percent": pct, "done": false,
        }));
    }

    loop {
        if state.0.lock().unwrap().contains(&filename) {
            // Cancel: flush and keep .part so the download can be resumed later
            let _ = file.flush().await;
            drop(file);
            return Err("Download dibatalkan".to_string());
        }
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if let Err(e) = file.write_all(&chunk).await {
                    drop(file);
                    return Err(format!("Gagal menulis file: {e}"));
                }
                downloaded += chunk.len() as u64;
                let pct = if total > 0 { downloaded as f64 / total as f64 * 100.0 } else { 0.0 };
                let _ = app.emit("llm-download-progress", serde_json::json!({
                    "filename": filename, "downloaded": downloaded,
                    "total": total, "percent": pct, "done": false,
                }));
            }
            Ok(None) => break,
            Err(e) => {
                // Network error: flush and keep .part for resume
                let _ = file.flush().await;
                drop(file);
                return Err(format!("Koneksi terputus saat download: {e}"));
            }
        }
    }

    file.flush().await.map_err(|e| format!("Gagal flush file: {e}"))?;
    drop(file);

    tokio::fs::rename(&tmp, &dest).await
        .map_err(|e| format!("Gagal menyimpan file final: {e}"))?;

    { state.0.lock().unwrap().remove(&filename); }

    let _ = app.emit("llm-download-progress", serde_json::json!({
        "filename": filename, "downloaded": downloaded,
        "total": total, "percent": 100.0, "done": true,
    }));

    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_partial_downloads(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let dir = get_llm_download_dir(&app)?;
    let mut map = serde_json::Map::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(stem) = name.strip_suffix(".part") {
                if let Ok(meta) = tokio::fs::metadata(entry.path()).await {
                    map.insert(stem.to_string(), meta.len().into());
                }
            }
        }
    }
    Ok(serde_json::Value::Object(map))
}

#[tauri::command]
pub async fn discard_partial_download(app: tauri::AppHandle, filename: String) -> Result<(), String> {
    let dir = get_llm_download_dir(&app)?;
    let tmp = dir.join(format!("{filename}.part"));
    if tmp.exists() {
        tokio::fs::remove_file(&tmp).await
            .map_err(|e| format!("Gagal menghapus file: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_llm_model(path: String) -> Result<(), String> {
    tokio::fs::remove_file(&path).await
        .map_err(|e| format!("Gagal menghapus model: {e}"))
}

#[tauri::command]
pub fn cancel_llm_download(state: tauri::State<DownloadState>, filename: String) {
    state.0.lock().unwrap().insert(filename);
}

#[tauri::command]
pub async fn reveal_in_file_manager(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    Command::new("open").args(["-R", &path]).spawn()
        .map_err(|e| format!("Gagal membuka Finder: {e}"))?;

    #[cfg(target_os = "windows")]
    Command::new("explorer").args(["/select,", &path]).spawn()
        .map_err(|e| format!("Gagal membuka Explorer: {e}"))?;

    #[cfg(target_os = "linux")]
    {
        let parent = Path::new(&path).parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(path);
        Command::new("xdg-open").arg(&parent).spawn()
            .map_err(|e| format!("Gagal membuka file manager: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn create_venv() -> Result<String, String> {
    let venv = venv_dir();
    let venv_str = venv.to_string_lossy().to_string();

    // Use system Python (not venv — we're creating it)
    #[cfg(target_os = "macos")]
    let candidates = ["/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"];
    #[cfg(target_os = "linux")]
    let candidates = ["/usr/bin/python3", "/usr/local/bin/python3", ""];
    #[cfg(target_os = "windows")]
    let candidates = ["python", "py", ""];

    let python = candidates.iter()
        .filter(|p| !p.is_empty())
        .find(|p| {
            #[cfg(not(target_os = "windows"))]
            { std::path::Path::new(p).exists() }
            #[cfg(target_os = "windows")]
            { which(p).is_some() }
        })
        .map(|s| s.to_string())
        .or_else(|| which("python3"))
        .unwrap_or_else(|| "python3".to_string());

    std::fs::create_dir_all(venv.parent().unwrap_or(&venv))
        .map_err(|e| format!("Gagal membuat direktori: {e}"))?;

    let output = Command::new(&python)
        .args(["-m", "venv", &venv_str])
        .output()
        .map_err(|e| format!("Gagal membuat virtual environment: {e}"))?;

    if output.status.success() {
        // On Apple Silicon: install mlx-whisper for GPU transcription acceleration
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            let pip = venv.join("bin").join("pip");
            let _ = Command::new(&pip)
                .args(["install", "--quiet", "mlx-whisper"])
                .output();
        }
        Ok(venv_str)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Gagal membuat virtual environment: {err}"))
    }
}

#[tauri::command]
pub async fn install_dependency(install_cmd: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("powershell")
            .args([
                "-NoProfile", "-Command",
                &format!(
                    "Start-Process powershell -ArgumentList @('-NoExit','-Command','{}')",
                    install_cmd.replace('\'', "''")
                ),
            ])
            .spawn()
            .map_err(|e| format!("Gagal membuka terminal: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Terminal\" to do script \"{}\"",
            install_cmd.replace('"', "\\\"")
        );
        Command::new("osascript").args(["-e", &script]).spawn()
            .map_err(|e| format!("Gagal membuka Terminal: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("x-terminal-emulator")
            .args(["-e", "bash", "-c", &format!("{install_cmd}; read -p 'Selesai. Tekan Enter...'")]).spawn()
            .or_else(|_| Command::new("xterm")
                .args(["-e", "bash", "-c", &format!("{install_cmd}; read -p 'Selesai. Tekan Enter...'")]).spawn())
            .map_err(|e| format!("Gagal membuka terminal: {e}"))?;
    }
    Ok(())
}

// ─── License management ──────────────────────────────────────────────────────

/// Ganti dengan product_id dari Gumroad (lihat di License Key module content page)
const GUMROAD_PRODUCT_ID: &str = "Zf1MU9sNCEuC2JI4-kpc9A==";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LicenseInfo {
    pub key: String,
    pub instance_id: String,
    pub product_name: String,
    pub customer_name: String,
    pub customer_email: String,
    #[serde(default = "default_platform")]
    pub platform: String, // "lemonsqueezy" or "gumroad"
}

fn default_platform() -> String {
    "lemonsqueezy".to_string()
}

// ─── LemonSqueezy API types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LsActivateResponse {
    activated: bool,
    error: Option<String>,
    instance: Option<LsInstance>,
    meta: Option<LsMeta>,
}

#[derive(Debug, Deserialize)]
struct LsInstance {
    id: String,
}

#[derive(Debug, Deserialize)]
struct LsMeta {
    product_name: String,
    #[serde(default)]
    customer_name: String,
    #[serde(default)]
    customer_email: String,
}

#[derive(Debug, Deserialize)]
struct LsValidateResponse {
    valid: bool,
    error: Option<String>,
    instance: Option<LsInstance>,
    meta: Option<LsMeta>,
}

// ─── Gumroad API types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GrVerifyResponse {
    success: bool,
    uses: Option<u64>,
    purchase: Option<GrPurchase>,
}

#[derive(Debug, Deserialize)]
struct GrPurchase {
    product_name: Option<String>,
    email: Option<String>,
    refunded: Option<bool>,
    disputed: Option<bool>,
    chargebacked: Option<bool>,
    subscription_ended_at: Option<String>,
    subscription_cancelled_at: Option<String>,
    subscription_failed_at: Option<String>,
}

fn license_file(app: &tauri::AppHandle) -> PathBuf {
    app.path().app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("license.json")
}

async fn save_license(app: &tauri::AppHandle, info: &LicenseInfo) -> Result<(), String> {
    let path = license_file(app);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| format!("Gagal membuat direktori: {e}"))?;
    }
    tokio::fs::write(&path, serde_json::to_string(info).unwrap()).await
        .map_err(|e| format!("Gagal menyimpan lisensi: {e}"))?;
    Ok(())
}

// ─── License commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn check_license(app: tauri::AppHandle) -> Option<LicenseInfo> {
    if cfg!(debug_assertions) {
        return Some(LicenseInfo {
            key: "DEV-MODE".to_string(),
            instance_id: "dev".to_string(),
            product_name: "AutoClipper".to_string(),
            customer_name: "Developer".to_string(),
            customer_email: String::new(),
            platform: "lemonsqueezy".to_string(),
        });
    }
    let path = license_file(&app);
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    let info: LicenseInfo = serde_json::from_str(&content).ok()?;

    let client = reqwest::Client::new();

    match info.platform.as_str() {
        "gumroad" => {
            // Verify via Gumroad API
            let resp = client
                .post("https://api.gumroad.com/v2/licenses/verify")
                .form(&[
                    ("product_id", GUMROAD_PRODUCT_ID),
                    ("license_key", info.key.as_str()),
                    ("increment_uses_count", "false"),
                ])
                .timeout(std::time::Duration::from_secs(10))
                .send().await;

            match resp {
                Ok(r) => {
                    if let Ok(body) = r.json::<GrVerifyResponse>().await {
                        if body.success {
                            // Check refund/dispute/chargeback status
                            if let Some(p) = &body.purchase {
                                let bad = p.refunded.unwrap_or(false)
                                    || p.disputed.unwrap_or(false)
                                    || p.chargebacked.unwrap_or(false);
                                if bad {
                                    return None;
                                }
                                // Check subscription ended/cancelled/failed
                                if p.subscription_ended_at.is_some()
                                    || p.subscription_cancelled_at.is_some()
                                    || p.subscription_failed_at.is_some()
                                {
                                    return None;
                                }
                            }
                            return Some(info);
                        }
                    }
                    None
                }
                Err(_) => Some(info), // no internet → allow offline (grace)
            }
        }
        _ => {
            // Default: LemonSqueezy validate
            let resp = client
                .post("https://api.lemonsqueezy.com/v1/licenses/validate")
                .form(&[("license_key", info.key.as_str()), ("instance_id", info.instance_id.as_str())])
                .timeout(std::time::Duration::from_secs(10))
                .send().await;

            match resp {
                Ok(r) => {
                    if let Ok(body) = r.json::<LsValidateResponse>().await {
                        if body.valid { Some(info) } else { None }
                    } else {
                        Some(info) // parse error → allow offline (grace)
                    }
                }
                Err(_) => Some(info), // no internet → allow offline (grace)
            }
        }
    }
}

#[tauri::command]
pub async fn activate_license(app: tauri::AppHandle, key: String) -> Result<LicenseInfo, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("License key tidak boleh kosong".to_string());
    }

    let instance_name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "AutoClipper".to_string());

    let client = reqwest::Client::new();

    // ── Try LemonSqueezy first ────────────────────────────────────────────
    let ls_resp = client
        .post("https://api.lemonsqueezy.com/v1/licenses/activate")
        .form(&[("license_key", key.as_str()), ("instance_name", instance_name.as_str())])
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;

    if let Ok(resp) = ls_resp {
        if let Ok(body) = resp.json::<LsActivateResponse>().await {
            if body.activated {
                let instance = body.instance.ok_or("Data instance tidak ditemukan")?;
                let meta = body.meta.unwrap_or(LsMeta {
                    product_name: "AutoClipper".to_string(),
                    customer_name: String::new(),
                    customer_email: String::new(),
                });

                let info = LicenseInfo {
                    key: key.clone(),
                    instance_id: instance.id,
                    product_name: meta.product_name,
                    customer_name: meta.customer_name,
                    customer_email: meta.customer_email,
                    platform: "lemonsqueezy".to_string(),
                };

                save_license(&app, &info).await?;
                return Ok(info);
            }
        }
    }

    // ── Fallback: Gumroad verify ──────────────────────────────────────────
    if GUMROAD_PRODUCT_ID != "YOUR_GUMROAD_PRODUCT_ID_HERE" {
        let gr_resp = client
            .post("https://api.gumroad.com/v2/licenses/verify")
            .form(&[
                ("product_id", GUMROAD_PRODUCT_ID),
                ("license_key", key.as_str()),
                ("increment_uses_count", "true"),
            ])
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| format!("Gagal menghubungi server lisensi: {e}"))?;

        let body: GrVerifyResponse = gr_resp.json().await
            .map_err(|e| format!("Respons server tidak valid: {e}"))?;

        if body.success {
            let purchase = body.purchase.unwrap_or(GrPurchase {
                product_name: None,
                email: None,
                refunded: None,
                disputed: None,
                chargebacked: None,
                subscription_ended_at: None,
                subscription_cancelled_at: None,
                subscription_failed_at: None,
            });

            let info = LicenseInfo {
                key,
                instance_id: instance_name,
                product_name: purchase.product_name.unwrap_or_else(|| "AutoClipper".to_string()),
                customer_name: String::new(),
                customer_email: purchase.email.unwrap_or_default(),
                platform: "gumroad".to_string(),
            };

            save_license(&app, &info).await?;
            return Ok(info);
        }

        return Err("License key tidak valid di LemonSqueezy maupun Gumroad".to_string());
    }

    Err("Gagal mengaktivasi lisensi. Pastikan license key valid.".to_string())
}

#[tauri::command]
pub async fn deactivate_license(app: tauri::AppHandle) -> Result<(), String> {
    let path = license_file(&app);
    if !path.exists() {
        return Ok(());
    }
    if let Ok(content) = tokio::fs::read_to_string(&path).await {
        if let Ok(info) = serde_json::from_str::<LicenseInfo>(&content) {
            if info.platform == "lemonsqueezy" {
                let client = reqwest::Client::new();
                let _ = client
                    .post("https://api.lemonsqueezy.com/v1/licenses/deactivate")
                    .form(&[("license_key", info.key.as_str()), ("instance_id", info.instance_id.as_str())])
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await;
            }
            // Gumroad: no deactivate endpoint — just delete the file below
        }
    }
    tokio::fs::remove_file(&path).await
        .map_err(|e| format!("Gagal menghapus lisensi: {e}"))
}

#[tauri::command]
pub async fn get_video_duration(video_path: String) -> Result<f64, String> {
    let ffprobe = find_ffprobe(None);
    let output = Command::new(&ffprobe)
        .args(["-v", "quiet", "-print_format", "json", "-show_format", &video_path])
        .output()
        .map_err(|e| format!("Gagal menjalankan ffprobe: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Parse error: {e}"))?;
    json["format"]["duration"].as_str().and_then(|s| s.parse().ok())
        .ok_or("Tidak bisa mendapatkan durasi video".to_string())
}

// ─── Whisper model management ─────────────────────────────────────────────────

fn hf_hub_cache() -> PathBuf {
    if let Ok(hf_home) = std::env::var("HF_HOME") {
        return PathBuf::from(hf_home).join("hub");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("huggingface").join("hub");
    }
    #[cfg(target_os = "windows")]
    if let Ok(user) = std::env::var("USERPROFILE") {
        return PathBuf::from(user).join(".cache").join("huggingface").join("hub");
    }
    PathBuf::from(".cache/huggingface/hub")
}

const WEIGHT_EXTS: &[&str] = &["safetensors", "npz", "bin", "pt"];

fn snapshot_has_weights(snap_dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(snap_dir) else { return false };
    for entry in entries.flatten() {
        let p = entry.path();
        // Use the symlink/file's own name extension, NOT the resolved target.
        // HF Hub on Windows stores blobs without extensions; the snapshot
        // entries (symlinks or hardlinks) carry the real name (e.g. model.bin).
        if let Some(ext) = p.extension() {
            if WEIGHT_EXTS.contains(&ext.to_str().unwrap_or("")) && p.exists() {
                return true;
            }
        }
    }
    false
}

fn hf_model_cached(cache: &Path, repo_id: &str) -> Option<PathBuf> {
    let dir_name = format!("models--{}", repo_id.replace('/', "--"));
    let model_dir = cache.join(&dir_name);
    if !model_dir.exists() { return None; }

    // Check that at least one snapshot has actual weight files
    let snaps_dir = model_dir.join("snapshots");
    if let Ok(snaps) = std::fs::read_dir(&snaps_dir) {
        for snap in snaps.flatten() {
            if snapshot_has_weights(&snap.path()) {
                return Some(model_dir);
            }
        }
    }
    None
}

struct WhisperModelDef {
    id:           &'static str,
    name:         &'static str,
    mlx_repo:     &'static str,
    fw_repo:      &'static str,
    size_mb:      u64,
    preset:       &'static str,
    preset_label: &'static str,
    description:  &'static str,
}

fn whisper_catalog() -> [WhisperModelDef; 4] {
    [
        WhisperModelDef {
            id: "tiny", name: "Whisper Tiny",
            mlx_repo: "mlx-community/whisper-tiny-mlx",
            fw_repo:  "Systran/faster-whisper-tiny",
            size_mb:  40, preset: "fast", preset_label: "Cepat",
            description: "Tercepat, akurasi rendah — cocok untuk bahasa tunggal yang jelas",
        },
        WhisperModelDef {
            id: "base", name: "Whisper Base",
            mlx_repo: "mlx-community/whisper-base-mlx",
            fw_repo:  "Systran/faster-whisper-base",
            size_mb:  74, preset: "balanced", preset_label: "Seimbang",
            description: "Cepat dengan akurasi yang cukup baik — rekomendasi untuk kebanyakan video",
        },
        WhisperModelDef {
            id: "medium", name: "Whisper Medium",
            mlx_repo: "mlx-community/whisper-medium-mlx",
            fw_repo:  "Systran/faster-whisper-medium",
            size_mb:  769, preset: "accurate", preset_label: "Akurat",
            description: "Akurasi tinggi, lebih lambat — cocok untuk bahasa campuran atau aksen",
        },
        WhisperModelDef {
            id: "large-v3-turbo", name: "Whisper Large v3 Turbo",
            mlx_repo: "mlx-community/whisper-large-v3-turbo",
            fw_repo:  "mobiuslabsgmbh/faster-whisper-large-v3-turbo",
            size_mb:  1_600, preset: "best", preset_label: "Terbaik",
            description: "Akurasi terbaik — ideal untuk bahasa campuran, terminologi khusus, atau kualitas audio rendah",
        },
    ]
}

#[tauri::command]
pub async fn list_whisper_models(app: tauri::AppHandle) -> Vec<WhisperModelInfo> {
    let is_apple_silicon = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    let hf_cache = hf_hub_cache();

    // vendor/models may also hold faster-whisper models
    let vendor = vendor_dir(&app);
    let vendor_models = vendor.as_deref().map(|v| v.join("models"));

    // Also check dev source: src-tauri/vendor/models
    let dev_models: Option<PathBuf> = (|| {
        let exe = std::env::current_exe().ok()?;
        let root = exe.parent()?.parent()?.parent()?.parent()?;
        let p = root.join("src-tauri").join("vendor").join("models");
        if p.exists() { Some(p) } else { None }
    })();

    let catalog = whisper_catalog();
    let mut result = Vec::new();

    for def in &catalog {
        let backend = if is_apple_silicon { "mlx" } else { "faster-whisper" };
        let repo    = if is_apple_silicon { def.mlx_repo } else { def.fw_repo };

        // Check in HF cache
        let mut cached_path = hf_model_cached(&hf_cache, repo);

        // Also check vendor/models dirs (faster-whisper may be downloaded there)
        if cached_path.is_none() {
            if let Some(ref vm) = vendor_models {
                cached_path = hf_model_cached(vm, repo);
            }
        }
        if cached_path.is_none() {
            if let Some(ref dm) = dev_models {
                cached_path = hf_model_cached(dm, repo);
            }
        }

        result.push(WhisperModelInfo {
            id:           def.id.to_string(),
            name:         def.name.to_string(),
            backend:      backend.to_string(),
            preset:       def.preset.to_string(),
            preset_label: def.preset_label.to_string(),
            description:  def.description.to_string(),
            size_mb:      def.size_mb,
            cached:       cached_path.is_some(),
            cache_path:   cached_path.map(|p| p.to_string_lossy().to_string()),
        });
    }

    result
}

#[tauri::command]
pub async fn download_whisper_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, WhisperDownloadState>,
    model_id: String,
    backend: String,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let model_key = format!("{model_id}-{backend}");
    { state.cancel_keys.lock().unwrap().remove(&model_key); }

    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let python = find_python(v);
    let script = find_script(&app, "download_whisper_model.py");

    // Download MLX models to HF default cache; faster-whisper to HF cache too
    // (transcribe.py checks HF cache for both)
    let cache_dir = hf_hub_cache().to_string_lossy().to_string();

    let args = vec![
        script,
        "--model".to_string(), model_id.clone(),
        "--backend".to_string(), backend.clone(),
        "--cache-dir".to_string(), cache_dir,
    ];

    let mut cmd = TokioCommand::new(&python);
    cmd.args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW — suppress console flash
    let mut child = cmd.spawn()
        .map_err(|e| format!("Gagal memulai download: {e}"))?;

    // Store PID for cancellation
    if let Some(pid) = child.id() {
        state.pids.lock().unwrap().insert(model_key.clone(), pid);
    }

    // Stream stderr progress
    let mk_clone = model_key.clone();
    let app_clone = app.clone();
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(pct_str) = line.strip_prefix("PROGRESS:") {
                    if let Ok(pct) = pct_str.trim().parse::<u8>() {
                        let _ = app_clone.emit("whisper-download-progress", serde_json::json!({
                            "model_key": mk_clone,
                            "percent": pct,
                            "done": false,
                            "error": null,
                        }));
                    }
                }
            }
        });
    }

    let cancel_state = state.cancel_keys.lock().unwrap().contains(&model_key);
    if cancel_state {
        let _ = child.kill().await;
        state.pids.lock().unwrap().remove(&model_key);
        return Err("Download dibatalkan".to_string());
    }

    let output = child.wait_with_output().await
        .map_err(|e| format!("Proses download error: {e}"))?;

    state.pids.lock().unwrap().remove(&model_key);

    if state.cancel_keys.lock().unwrap().remove(&model_key) {
        return Err("Download dibatalkan".to_string());
    }

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stdout).to_string();
        let err: serde_json::Value = serde_json::from_str(&err_msg).unwrap_or_default();
        let msg = err["error"].as_str().unwrap_or("Download gagal").to_string();
        let _ = app.emit("whisper-download-progress", serde_json::json!({
            "model_key": model_key,
            "percent": 0,
            "done": true,
            "error": msg,
        }));
        return Err(msg);
    }

    let _ = app.emit("whisper-download-progress", serde_json::json!({
        "model_key": model_key,
        "percent": 100,
        "done": true,
        "error": null,
    }));

    Ok(())
}

#[tauri::command]
pub async fn cancel_whisper_download(
    state: tauri::State<'_, WhisperDownloadState>,
    model_key: String,
) -> Result<(), String> {
    state.cancel_keys.lock().unwrap().insert(model_key.clone());
    if let Some(pid) = state.pids.lock().unwrap().get(&model_key).copied() {
        #[cfg(unix)]
        unsafe { libc::kill(pid as i32, libc::SIGTERM); }
        #[cfg(windows)]
        { let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output(); }
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_whisper_model(cache_path: String) -> Result<(), String> {
    tokio::fs::remove_dir_all(&cache_path).await
        .map_err(|e| format!("Gagal menghapus model: {e}"))
}
