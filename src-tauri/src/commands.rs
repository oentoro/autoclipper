use serde::{Deserialize, Serialize};
use std::process::Command;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
    pub start_index: usize,
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

fn build_classify_prompt(segments: &[SrtSegment]) -> String {
    let segs = sample_segments(segments, 300);
    let text: String = segs.iter()
        .map(|s| format!("[{}] {}: {}", s.index, s.start_time, s.text))
        .collect::<Vec<_>>().join("\n");
    let first = segments.first().map(|s| s.index).unwrap_or(1);
    let last  = segments.last().map(|s| s.index).unwrap_or(1);
    let mins  = segments.last().map(|s| s.end as u64 / 60).unwrap_or(0);
    format!(
        "Transkrip video (~{mins} menit, segmen {first}\u{2013}{last}):\n\n{text}\n\n\
Bagi menjadi bagian-bagian berurutan berdasarkan topik. \
Setiap bagian MAKSIMAL 60 detik (perhatikan timestamp untuk memastikan durasi). \
Balas HANYA dengan JSON (tanpa teks lain):\n\
{{\"sections\":[\
{{\"name\":\"Pembukaan\",\"summary\":\"Ringkasan satu kalimat.\",\"start_index\":{first},\"end_index\":10}},\
{{\"name\":\"Isi Utama\",\"summary\":\"Ringkasan satu kalimat.\",\"start_index\":11,\"end_index\":{last}}}\
]}}\n\
Aturan: bagian berurutan, mencakup semua segmen, nama max 4 kata, durasi tiap bagian max 60 detik."
    )
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

async fn infer_ollama_api(model: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
        "format": "json",
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

async fn ensure_llama_server(
    app: &tauri::AppHandle,
    state: &LlamaServerState,
    model_path: &str,
) -> Result<(), String> {
    let _lock = state.startup_lock.lock().await; // serialize concurrent starts

    let already_running = state.current_model.lock().unwrap().as_str() == model_path;
    if already_running { return Ok(()); }

    // Kill existing server if switching models
    {
        let mut g = state.process.lock().unwrap();
        if let Some(mut child) = g.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    { state.current_model.lock().unwrap().clear(); }

    let binary = find_llama_server(app).ok_or_else(||
        "llama-server binary tidak ditemukan. \
        Jalankan: python3 scripts/download_llama_server.py".to_string()
    )?;

    let mut args = vec![
        "--model".to_string(), model_path.to_string(),
        "--port".to_string(), LLAMA_PORT.to_string(),
        "--ctx-size".to_string(), "16384".to_string(),
        "--threads".to_string(), "4".to_string(),
        "--parallel".to_string(), "1".to_string(),
    ];
    #[cfg(target_os = "macos")]
    { args.push("-ngl".to_string()); args.push("999".to_string()); } // Metal GPU

    let child = std::process::Command::new(&binary)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Gagal menjalankan llama-server: {e}"))?;

    { *state.process.lock().unwrap() = Some(child); }

    // Poll health endpoint (up to 90 seconds — large models take time to load)
    let health = format!("http://localhost:{LLAMA_PORT}/health");
    let client = reqwest::Client::new();
    let mut ready = false;
    for _ in 0..90 {
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
        return Err(
            "llama-server gagal start dalam 90 detik. \
            Coba model yang lebih kecil atau periksa apakah file .gguf valid.".to_string()
        );
    }

    *state.current_model.lock().unwrap() = model_path.to_string();
    Ok(())
}

async fn infer_llama_server(prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "messages": [{"role": "user", "content": prompt}],
        "response_format": {"type": "json_object"},
        "temperature": 0.1,
        "max_tokens": 2048,
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

async fn run_llm_task(
    app: &tauri::AppHandle,
    server_state: &LlamaServerState,
    task: &str,
    segments: &[SrtSegment],
    model_path: &str,
    ollama_model: &str,
) -> Result<String, String> {
    let prompt = if task == "classify" {
        build_classify_prompt(segments)
    } else {
        build_analyze_prompt(segments)
    };

    let resolved_path = if !model_path.is_empty() && Path::new(model_path).exists() {
        Some(model_path.to_string())
    } else if model_path.is_empty() {
        find_llm_model(app)
    } else {
        None
    };

    if let Some(mp) = resolved_path {
        ensure_llama_server(app, server_state, &mp).await?;
        infer_llama_server(&prompt).await
    } else {
        let om = if ollama_model.is_empty() { "gemma4:latest" } else { ollama_model };
        infer_ollama_api(om, &prompt).await.map_err(|e|
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
fn build_retimed_entries(groups: &[ClipGroup]) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();
    let mut cursor = 0.0_f64;
    for group in groups {
        let g_start = group.start_sec;
        let g_dur   = group.duration();
        for seg in &group.segs {
            let sub_start = (cursor + (seg.start - g_start)).max(0.0);
            let sub_end   = (cursor + (seg.end   - g_start)).min(cursor + g_dur);
            entries.push(serde_json::json!({ "start": sub_start, "end": sub_end, "text": seg.text }));
        }
        cursor += g_dur;
    }
    entries
}

// Single group — always transcode so that the seek is frame-accurate.
// Stream copy with fast input seek snaps to the nearest keyframe which can
// include a few frames of content before the intended start, causing repeats
// when the clip is played back after concatenation.
fn encode_one_group(
    ffmpeg: &str, video_path: &str, group: &ClipGroup,
    crop_filter: Option<&str>, dest: &str,
) -> Result<(), String> {
    let ss  = format!("{:.6}", group.start_sec);
    let dur = format!("{:.6}", group.duration());

    let mut args = vec![
        "-y".to_string(),
        "-ss".to_string(), ss,
        "-i".to_string(),  video_path.to_string(),
        "-t".to_string(),  dur,   // duration instead of -to: avoids rounding drift
    ];
    if let Some(crop) = crop_filter {
        args.extend(["-vf".to_string(), crop.to_string()]);
    }
    args.extend([
        "-c:v".to_string(), "libx264".to_string(),
        "-preset".to_string(), "fast".to_string(),
        "-crf".to_string(), "23".to_string(),
        "-threads".to_string(), "0".to_string(),
        "-c:a".to_string(), "aac".to_string(),
        "-b:a".to_string(), "128k".to_string(),
        dest.to_string(),
    ]);

    let status = Command::new(ffmpeg)
        .args(&args)
        .status()
        .map_err(|e| format!("Gagal menjalankan FFmpeg: {e}"))?;

    if status.success() { Ok(()) } else { Err("FFmpeg gagal mengekstrak klip.".to_string()) }
}

// Multiple groups: build ONE FFmpeg command with N inputs (each fast-seeked) and
// a filter_complex concat. This produces correct timestamps without temp files
// and avoids the 2x-speed bug caused by stream-copy + concat demuxer.
fn concat_groups(
    ffmpeg: &str, video_path: &str, groups: &[ClipGroup],
    crop_filter: Option<&str>, dest: &str,
) -> Result<(), String> {
    if groups.len() == 1 {
        return encode_one_group(ffmpeg, video_path, &groups[0], crop_filter, dest);
    }

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
        "-threads".to_string(), "0".to_string(),  // all cores for this one encode pass
        "-c:a".to_string(), "aac".to_string(),
        "-b:a".to_string(), "128k".to_string(),
        dest.to_string(),
    ]);

    let status = Command::new(ffmpeg)
        .args(&args)
        .status()
        .map_err(|e| format!("Gagal menjalankan FFmpeg: {e}"))?;

    if status.success() { Ok(()) } else { Err("FFmpeg gagal menggabungkan klip.".to_string()) }
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

async fn translate_batch(
    client: &reqwest::Client,
    texts: &[String],
    source_lang: &str,
    target_lang: &str,
) -> Result<Vec<String>, String> {
    let src = lang_display_name(source_lang);
    let tgt = lang_display_name(target_lang);
    let input_json = serde_json::to_string(texts).unwrap();

    let prompt = format!(
        "Translate the following subtitle texts from {src} to {tgt}.\n\
        Return ONLY a JSON array of translated strings, same count and order as input.\n\
        No explanation, no extra text — only the JSON array.\n\n\
        Input: {input_json}\n\nOutput:"
    );

    let body = serde_json::json!({
        "model": "gemma4:latest",
        "messages": [{"role": "user", "content": prompt}],
        "stream": false
    });

    let response = client
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .timeout(std::time::Duration::from_secs(180))
        .send().await
        .map_err(|e| format!("Gagal koneksi ke Ollama: {e}. Pastikan Ollama berjalan."))?;

    if !response.status().is_success() {
        return Err(format!("Ollama error: HTTP {}", response.status()));
    }

    let resp: serde_json::Value = response.json().await
        .map_err(|e| format!("Gagal parse response Ollama: {e}"))?;

    let content = resp["message"]["content"].as_str()
        .ok_or("Response Ollama kosong")?;

    // Extract JSON array (model might prepend/append text)
    let start = content.find('[').ok_or_else(|| format!("JSON array tidak ditemukan di response: {content}"))?;
    let end   = content.rfind(']').ok_or("Penutup JSON array tidak ditemukan")?;
    let arr: Vec<String> = serde_json::from_str(&content[start..=end])
        .map_err(|e| format!("Gagal parse array terjemahan: {e}\nContent: {content}"))?;

    if arr.len() != texts.len() {
        return Err(format!(
            "Jumlah teks tidak cocok: dikirim {}, diterima {}", texts.len(), arr.len()
        ));
    }
    Ok(arr)
}

#[tauri::command]
pub async fn translate_transcript(
    segments: Vec<SrtSegment>,
    source_language: String,
    target_language: String,
) -> Result<TranslateResult, String> {
    let client = reqwest::Client::new();
    let all_texts: Vec<String> = segments.iter().map(|s| s.text.clone()).collect();
    let mut translated: Vec<String> = Vec::new();

    // Batch to avoid overwhelming the model
    for chunk in all_texts.chunks(25) {
        let batch = translate_batch(&client, chunk, &source_language, &target_language).await?;
        translated.extend(batch);
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
    let (py_install, ff_install, pip) = ("brew install python", "brew install ffmpeg", "pip3");
    #[cfg(target_os = "linux")]
    let (py_install, ff_install, pip) = (
        "sudo apt install python3  # Ubuntu\nsudo dnf install python3  # Fedora",
        "sudo apt install ffmpeg   # Ubuntu\nsudo dnf install ffmpeg   # Fedora",
        "pip3",
    );
    #[cfg(target_os = "windows")]
    let has_choco = which("choco").is_some();
    #[cfg(target_os = "windows")]
    let (py_install, ff_install, pip) = if has_choco {
        ("choco install python -y", "choco install ffmpeg -y", "pip")
    } else {
        ("winget install Python.Python.3 -e --source winget", "winget install Gyan.FFmpeg -e --source winget", "pip")
    };

    let mut checks: Vec<DepCheck> = Vec::new();

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

    let whisper_ok = python_ok && Command::new(&python)
        .args(["-c", "import faster_whisper"])
        .output().map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("faster-whisper ({source})"),
        ok: whisper_ok,
        path: None,
        error: if !whisper_ok { Some("Package faster-whisper belum terinstall".to_string()) } else { None },
        install_cmd: if !whisper_ok { Some(format!("{pip} install faster-whisper")) } else { None },
        download_url: None,
        optional: false,
    });

    let hf_hub_ok = python_ok && Command::new(&python)
        .args(["-c", "import huggingface_hub"])
        .output().map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("huggingface-hub ({source})"),
        ok: hf_hub_ok,
        path: None,
        error: if !hf_hub_ok { Some("Package huggingface-hub belum terinstall — diperlukan untuk download model Whisper".to_string()) } else { None },
        install_cmd: if !hf_hub_ok { Some(format!("{pip} install huggingface_hub")) } else { None },
        download_url: None,
        optional: false,
    });

    let pillow_ok = python_ok && Command::new(&python)
        .args(["-c", "import PIL"])
        .output().map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("Pillow ({source})"),
        ok: pillow_ok,
        path: None,
        error: if !pillow_ok { Some("Package Pillow belum terinstall".to_string()) } else { None },
        install_cmd: if !pillow_ok { Some(format!("{pip} install Pillow")) } else { None },
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

    let opencv_ok = python_ok && Command::new(&python)
        .args(["-c", "import cv2"])
        .output().map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("opencv-python (Smart Crop)"),
        ok: opencv_ok,
        path: None,
        error: if !opencv_ok { Some("opencv-python belum terinstall — diperlukan untuk Smart Crop".to_string()) } else { None },
        install_cmd: if !opencv_ok { Some(format!("{pip} install opencv-python")) } else { None },
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

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        // Error JSON is written to stdout (stderr is consumed by progress reader)
        let err: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
        let error = err["error"].as_str().unwrap_or("Whisper gagal").to_string();
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
    let content = run_llm_task(&app, &*server, "analyze", &segments, &model_path, &ollama_model).await?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Gagal parse JSON dari AI: {e}\nContent: {content}"))
}

#[tauri::command]
// Split any section whose actual duration (from segments) exceeds max_sec into
// consecutive sub-sections. Each sub-section gets a numbered suffix in its name.
fn split_long_sections(sections: Vec<Section>, segments: &[SrtSegment], max_sec: f64) -> Vec<Section> {
    let mut result = Vec::new();

    for section in sections {
        let segs: Vec<&SrtSegment> = segments.iter()
            .filter(|s| s.index >= section.start_index && s.index <= section.end_index)
            .collect();

        if segs.is_empty() {
            result.push(section);
            continue;
        }

        let total_dur = segs.last().unwrap().end - segs.first().unwrap().start;
        if total_dur <= max_sec {
            result.push(section);
            continue;
        }

        // Walk through segments, closing a sub-section when adding the next one
        // would exceed max_sec from the current sub-section's start.
        let mut i = 0;
        let mut part = 1usize;
        while i < segs.len() {
            let chunk_start = segs[i].start;
            let mut j = i;
            while j + 1 < segs.len() && segs[j + 1].end - chunk_start <= max_sec {
                j += 1;
            }
            result.push(Section {
                name:        format!("{} ({})", section.name, part),
                summary:     section.summary.clone(),
                start_index: segs[i].index,
                end_index:   segs[j].index,
            });
            i = j + 1;
            part += 1;
        }
    }

    result
}

#[tauri::command]
pub async fn classify_transcript(
    app: tauri::AppHandle,
    server: tauri::State<'_, LlamaServerState>,
    segments: Vec<SrtSegment>,
    model_path: String,
    ollama_model: String,
) -> Result<ClassifyResult, String> {
    if segments.is_empty() {
        return Err("Tidak ada segmen untuk diklasifikasikan".to_string());
    }

    let first_idx = segments.first().map(|s| s.index).unwrap_or(1);
    let last_idx  = segments.last().map(|s| s.index).unwrap_or(1);

    let content = run_llm_task(&app, &*server, "classify", &segments, &model_path, &ollama_model).await?;

    let mut result: ClassifyResult = serde_json::from_str(&content)
        .map_err(|e| format!("Gagal parse klasifikasi: {e}\nContent: {content}"))?;

    for sec in &mut result.sections {
        sec.start_index = sec.start_index.max(first_idx);
        sec.end_index   = sec.end_index.min(last_idx).max(sec.start_index);
    }
    if let Some(last) = result.sections.last_mut() {
        last.end_index = last_idx;
    }

    // Guarantee every section is ≤ 60 seconds regardless of what the AI produced
    result.sections = split_long_sections(result.sections, &segments, 60.0);

    Ok(result)
}

async fn exec_smart_crop(
    app: &tauri::AppHandle,
    python: &str,
    ffmpeg: &str,
    input: &str,
    output: &str,
    aspect_ratio: &str,
) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command as TokioCommand;

    let script = find_script(app, "smart_crop.py");
    let mut cmd = TokioCommand::new(python);
    cmd.args([&script, input, output, "--ratio", aspect_ratio])
        .env("AUTOCLIPPER_FFMPEG", ffmpeg)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn()
        .map_err(|e| format!("Gagal menjalankan smart_crop.py: {e}"))?;

    if let Some(stderr) = child.stderr.take() {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(pct_str) = line.strip_prefix("PROGRESS:") {
                    if let Ok(pct) = pct_str.trim().parse::<u8>() {
                        let _ = app_clone.emit("clip-smart-percent", pct);
                    }
                }
            }
        });
    }

    let output_r = child.wait_with_output().await
        .map_err(|e| format!("Gagal menunggu smart_crop.py: {e}"))?;

    if output_r.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output_r.stdout);
        let stderr_str = String::from_utf8_lossy(&output_r.stderr);
        let src = if !stdout.trim().is_empty() { stdout } else { stderr_str };
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
    video_path: String,
    segments: Vec<SrtSegment>,
    selected_indices: Vec<usize>,
    output_path: String,
    burn_subtitles: bool,
    aspect_ratio: String,
    smart_crop: bool,
    font_size: u32,
    font_path: String,
    subtitle_style_json: String,
) -> Result<ClipResult, String> {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let ffmpeg = find_ffmpeg(v);
    let ffprobe = find_ffprobe(v);
    let python = find_python(v);

    let mut selected: Vec<&SrtSegment> = segments.iter()
        .filter(|s| selected_indices.contains(&s.index)).collect();
    if selected.is_empty() { return Err("Tidak ada segmen yang dipilih".to_string()); }
    selected.sort_by_key(|s| s.index);

    let total_segments = selected.len();
    let total_duration: f64 = selected.iter().map(|s| s.end - s.start).sum();

    let groups = group_segments(&selected);

    // Smart crop: skip FFmpeg center crop; let smart_crop.py handle it instead
    let needs_smart = smart_crop && aspect_ratio != "original";
    let ffmpeg_crop = if needs_smart || aspect_ratio == "original" {
        None
    } else {
        let (w, h) = get_video_dims(&video_path, v)?;
        build_crop_filter(&aspect_ratio, w, h)
    };



    match (burn_subtitles, needs_smart) {
        // ── Case 1: no burn, no smart crop ────────────────────────────────
        (false, false) => {
            concat_groups(&ffmpeg, &video_path, &groups, ffmpeg_crop.as_deref(), &output_path)?;
        }

        // ── Case 2: burn only, no smart crop ──────────────────────────────
        (true, false) => {
            let tmp = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            concat_groups(&ffmpeg, &video_path, &groups, ffmpeg_crop.as_deref(), tmp.to_str().unwrap())?;
            let entries = build_retimed_entries(&groups);
            let r = exec_burn_subs(&app, &python, &ffmpeg, &ffprobe, tmp.to_str().unwrap(), &output_path, entries, font_size, &font_path, &subtitle_style_json).await;
            let _ = std::fs::remove_file(&tmp);
            r?;
        }

        // ── Case 3: smart crop only, no burn ──────────────────────────────
        (false, true) => {
            let tmp = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            concat_groups(&ffmpeg, &video_path, &groups, None, tmp.to_str().unwrap())?;
            let r = exec_smart_crop(&app, &python, &ffmpeg, tmp.to_str().unwrap(), &output_path, &aspect_ratio).await;
            let _ = std::fs::remove_file(&tmp);
            r?;
        }

        // ── Case 4: smart crop + burn ─────────────────────────────────────
        (true, true) => {
            let tmp_concat = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
            let tmp_smart  = std::env::temp_dir().join("autoclipper_smart_tmp.mp4");
            concat_groups(&ffmpeg, &video_path, &groups, None, tmp_concat.to_str().unwrap())?;
            let r = exec_smart_crop(&app, &python, &ffmpeg, tmp_concat.to_str().unwrap(), tmp_smart.to_str().unwrap(), &aspect_ratio).await;
            let _ = std::fs::remove_file(&tmp_concat);
            r?;
            let entries = build_retimed_entries(&groups);
            let r = exec_burn_subs(&app, &python, &ffmpeg, &ffprobe, tmp_smart.to_str().unwrap(), &output_path, entries, font_size, &font_path, &subtitle_style_json).await;
            let _ = std::fs::remove_file(&tmp_smart);
            r?;
        }
    }

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LicenseInfo {
    pub key: String,
    pub instance_id: String,
    pub product_name: String,
    pub customer_name: String,
    pub customer_email: String,
}

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

fn license_file(app: &tauri::AppHandle) -> PathBuf {
    app.path().app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("license.json")
}

#[tauri::command]
pub async fn check_license(app: tauri::AppHandle) -> Option<LicenseInfo> {
    if cfg!(debug_assertions) {
        return Some(LicenseInfo {
            key: "DEV-MODE".to_string(),
            instance_id: "dev".to_string(),
            product_name: "AutoClipper".to_string(),
            customer_name: "Developer".to_string(),
            customer_email: String::new(),
        });
    }
    let path = license_file(&app);
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    serde_json::from_str(&content).ok()
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
    let resp = client
        .post("https://api.lemonsqueezy.com/v1/licenses/activate")
        .form(&[("license_key", key.as_str()), ("instance_name", instance_name.as_str())])
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Gagal menghubungi server lisensi: {e}"))?;

    let body: LsActivateResponse = resp.json().await
        .map_err(|e| format!("Respons server tidak valid: {e}"))?;

    if !body.activated {
        return Err(body.error.unwrap_or_else(|| "Lisensi tidak valid atau sudah habis digunakan".to_string()));
    }

    let instance = body.instance.ok_or("Data instance tidak ditemukan")?;
    let meta = body.meta.unwrap_or(LsMeta {
        product_name: "AutoClipper".to_string(),
        customer_name: String::new(),
        customer_email: String::new(),
    });

    let info = LicenseInfo {
        key,
        instance_id: instance.id,
        product_name: meta.product_name,
        customer_name: meta.customer_name,
        customer_email: meta.customer_email,
    };

    let path = license_file(&app);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| format!("Gagal membuat direktori: {e}"))?;
    }
    tokio::fs::write(&path, serde_json::to_string(&info).unwrap()).await
        .map_err(|e| format!("Gagal menyimpan lisensi: {e}"))?;

    Ok(info)
}

#[tauri::command]
pub async fn deactivate_license(app: tauri::AppHandle) -> Result<(), String> {
    let path = license_file(&app);
    if !path.exists() {
        return Ok(());
    }
    if let Ok(content) = tokio::fs::read_to_string(&path).await {
        if let Ok(info) = serde_json::from_str::<LicenseInfo>(&content) {
            let client = reqwest::Client::new();
            let _ = client
                .post("https://api.lemonsqueezy.com/v1/licenses/deactivate")
                .form(&[("license_key", info.key.as_str()), ("instance_id", info.instance_id.as_str())])
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;
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
