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
    { which("python").or_else(|| which("python3")).unwrap_or_else(|| "python.exe".to_string()) }
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
Bagi menjadi 3\u{2013}7 bagian berurutan berdasarkan topik. \
Balas HANYA dengan JSON (tanpa teks lain):\n\
{{\"sections\":[\
{{\"name\":\"Pembukaan\",\"summary\":\"Ringkasan satu kalimat.\",\"start_index\":{first},\"end_index\":10}},\
{{\"name\":\"Isi Utama\",\"summary\":\"Ringkasan satu kalimat.\",\"start_index\":11,\"end_index\":{last}}}\
]}}\n\
Aturan: bagian berurutan, mencakup semua segmen, nama max 4 kata."
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

fn build_retimed_entries(selected: &[&SrtSegment]) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();
    let mut cursor = 0.0_f64;
    for seg in selected.iter() {
        let dur = seg.end - seg.start;
        entries.push(serde_json::json!({ "start": cursor, "end": cursor + dur, "text": seg.text }));
        cursor += dur;
    }
    entries
}

fn concat_segments(
    ffmpeg: &str, video_path: &str, selected: &[&SrtSegment],
    crop_filter: Option<&str>, dest: &str,
) -> Result<(), String> {
    use std::io::Write;

    let tmp_dir = std::env::temp_dir().join("autoclipper_segs");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Gagal membuat folder temp: {e}"))?;

    let mut seg_paths: Vec<PathBuf> = Vec::new();

    // Pass 1: encode each segment to its own file so every clip starts with an I-frame
    for (i, seg) in selected.iter().enumerate() {
        let seg_path = tmp_dir.join(format!("s{i:04}.mp4"));
        let trim_vf = format!(
            "trim=start={:.6}:end={:.6},setpts=PTS-STARTPTS",
            seg.start, seg.end
        );
        let fc = match crop_filter {
            Some(crop) => format!(
                "[0:v]{trim_vf},{crop}[v]; [0:a]atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS[a]",
                seg.start, seg.end
            ),
            None => format!(
                "[0:v]{trim_vf}[v]; [0:a]atrim=start={:.6}:end={:.6},asetpts=PTS-STARTPTS[a]",
                seg.start, seg.end
            ),
        };
        let status = Command::new(ffmpeg)
            .args([
                "-y", "-i", video_path,
                "-filter_complex", &fc,
                "-map", "[v]", "-map", "[a]",
                "-c:v", "libx264", "-preset", "fast", "-crf", "23",
                "-c:a", "aac", "-b:a", "128k",
                seg_path.to_str().unwrap(),
            ])
            .status()
            .map_err(|e| format!("Gagal menjalankan FFmpeg: {e}"))?;
        if !status.success() {
            for f in &seg_paths { let _ = std::fs::remove_file(f); }
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(format!("FFmpeg gagal mengekstrak segmen {}.", i + 1));
        }
        seg_paths.push(seg_path);
    }

    // Pass 2: concat demuxer (-c copy) — no re-encode, no frame gaps
    let list_path = tmp_dir.join("list.txt");
    {
        let mut f = std::fs::File::create(&list_path)
            .map_err(|e| format!("Gagal membuat concat list: {e}"))?;
        for p in &seg_paths {
            writeln!(f, "file '{}'", p.to_str().unwrap().replace('\'', r"'\''"))
                .map_err(|e| format!("Gagal menulis list: {e}"))?;
        }
    }
    let status = Command::new(ffmpeg)
        .args([
            "-y", "-f", "concat", "-safe", "0",
            "-i", list_path.to_str().unwrap(),
            "-c", "copy", dest,
        ])
        .status()
        .map_err(|e| format!("Gagal menjalankan FFmpeg: {e}"))?;

    for f in &seg_paths { let _ = std::fs::remove_file(f); }
    let _ = std::fs::remove_file(&list_path);
    let _ = std::fs::remove_dir(&tmp_dir);

    if status.success() { Ok(()) } else { Err("FFmpeg gagal menggabungkan segmen.".to_string()) }
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
    let (py_install, ff_install, pip) = ("winget install Python.Python.3", "winget install ffmpeg", "pip");

    let mut checks: Vec<DepCheck> = Vec::new();

    let python = find_python(v);
    let python_ok = Command::new(&python).arg("--version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("Python 3 ({source})"),
        ok: python_ok,
        path: if python_ok { Some(python.clone()) } else { None },
        error: if !python_ok { Some("Python 3 tidak ditemukan".to_string()) } else { None },
        install_cmd: if !python_ok && !bundled { Some(py_install.to_string()) } else { None },
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
        install_cmd: if !ffmpeg_ok && !bundled { Some(ff_install.to_string()) } else { None },
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
        install_cmd: if !ffprobe_ok && !bundled { Some(ff_install.to_string()) } else { None },
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
        install_cmd: if !whisper_ok && !bundled { Some(format!("{pip} install faster-whisper")) } else { None },
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
        install_cmd: if !pillow_ok && !bundled { Some(format!("{pip} install Pillow")) } else { None },
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
        optional: false,
    });

    let llama_server_path = find_llama_server(&app);
    let llama_server_ok = llama_server_path.is_some();
    checks.push(DepCheck {
        name: "llama-server (AI Lokal)".to_string(),
        ok: llama_server_ok,
        path: llama_server_path,
        error: if !llama_server_ok {
            Some("Binary llama-server tidak ditemukan. Jalankan: python3 scripts/download_llama_server.py".to_string())
        } else { None },
        install_cmd: if !llama_server_ok {
            Some("python3 scripts/download_llama_server.py".to_string())
        } else { None },
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
) -> Result<TranscribeResult, String> {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let python = find_python(v);
    let script = find_script(&app, "transcribe.py");

    let mut args = vec![script, video_path];
    if !source_language.is_empty() {
        args.push("--language".to_string());
        args.push(source_language);
    }
    let preset = if preset.is_empty() { "balanced".to_string() } else { preset };
    args.push("--preset".to_string());
    args.push(preset);
    if let Some(model_dir) = find_model_dir(v) {
        args.push("--model-dir".to_string());
        args.push(model_dir);
    }

    let output = Command::new(&python).args(&args).output()
        .map_err(|e| format!("Gagal menjalankan Whisper: {e}"))?;

    if !output.status.success() {
        return Err(format!("Whisper error: {}", String::from_utf8_lossy(&output.stderr)));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
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

    Ok(result)
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
    selected.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());

    let total_duration: f64 = selected.iter().map(|s| s.end - s.start).sum();
    let total_segments = selected.len();

    let crop_filter = if aspect_ratio != "original" {
        let (w, h) = get_video_dims(&video_path, v)?;
        build_crop_filter(&aspect_ratio, w, h)
    } else { None };

    if burn_subtitles {
        let tmp_path = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
        let tmp_str = tmp_path.to_string_lossy().to_string();
        concat_segments(&ffmpeg, &video_path, &selected, crop_filter.as_deref(), &tmp_str)?;

        let entries = build_retimed_entries(&selected);
        let entries_path = std::env::temp_dir().join("autoclipper_entries.json");
        std::fs::write(&entries_path, serde_json::to_string(&entries).unwrap())
            .map_err(|e| format!("Gagal menulis entries JSON: {e}"))?;

        let script = find_script(&app, "burn_subtitles.py");
        let entries_str = entries_path.to_string_lossy().to_string();
        let mut burn_args = vec![script, tmp_str.clone(), entries_str, output_path.clone()];
        if font_size > 0 {
            burn_args.push("--font-size".to_string());
            burn_args.push(font_size.to_string());
        }
        if !font_path.is_empty() {
            burn_args.push("--font".to_string());
            burn_args.push(font_path.clone());
        }
        if !subtitle_style_json.is_empty() && subtitle_style_json != "{}" {
            burn_args.push("--style".to_string());
            burn_args.push(subtitle_style_json.clone());
        }
        let out = Command::new(&python)
            .args(&burn_args)
            .env("AUTOCLIPPER_FFMPEG", &ffmpeg)
            .env("AUTOCLIPPER_FFPROBE", &ffprobe)
            .output()
            .map_err(|e| format!("Gagal menjalankan burn_subtitles.py: {e}"))?;

        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_file(&entries_path);

        if !out.status.success() {
            return Err(format!("Subtitle burn gagal: {}", String::from_utf8_lossy(&out.stderr)));
        }
    } else {
        concat_segments(&ffmpeg, &video_path, &selected, crop_filter.as_deref(), &output_path)?;
    }

    let ar_note = if aspect_ratio != "original" { format!(" [{aspect_ratio}]") } else { String::new() };
    let sub_note = if burn_subtitles { " + subtitle" } else { "" };
    Ok(ClipResult {
        output_path,
        success: true,
        message: format!("Berhasil menggabungkan {total_segments} segmen ({:.1}s total){sub_note}{ar_note}", total_duration),
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
