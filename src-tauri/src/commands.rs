use serde::{Deserialize, Serialize};
use std::process::Command;
use std::path::{Path, PathBuf};
use tauri::Manager;

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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeResult {
    pub important_indices: Vec<usize>,
    pub reasoning: String,
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
}

// ─── Vendor dir ───────────────────────────────────────────────────────────────

fn vendor_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?.join("vendor");
    if dir.exists() { Some(dir) } else { None }
}

// ─── Path finders ─────────────────────────────────────────────────────────────

fn which(bin: &str) -> Option<String> {
    let out = Command::new("which").arg(bin).output().ok()?;
    if out.status.success() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !p.is_empty() { return Some(p); }
    }
    None
}

fn find_python(vendor: Option<&Path>) -> String {
    if let Some(v) = vendor {
        let p = v.join("python/bin/python3");
        if p.exists() { return p.to_string_lossy().to_string(); }
    }
    for c in ["/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"] {
        if Path::new(c).exists() { return c.to_string(); }
    }
    which("python3").unwrap_or_else(|| "python3".to_string())
}

fn find_ffmpeg(vendor: Option<&Path>) -> String {
    if let Some(v) = vendor {
        let p = v.join("bin/ffmpeg");
        if p.exists() { return p.to_string_lossy().to_string(); }
    }
    for c in ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"] {
        if Path::new(c).exists() { return c.to_string(); }
    }
    which("ffmpeg").unwrap_or_else(|| "ffmpeg".to_string())
}

fn find_ffprobe(vendor: Option<&Path>) -> String {
    if let Some(v) = vendor {
        let p = v.join("bin/ffprobe");
        if p.exists() { return p.to_string_lossy().to_string(); }
    }
    let ffmpeg = find_ffmpeg(vendor);
    let probe = ffmpeg.replace("ffmpeg", "ffprobe");
    if probe != ffmpeg && Path::new(&probe).exists() { return probe; }
    for c in ["/opt/homebrew/bin/ffprobe", "/usr/local/bin/ffprobe", "/usr/bin/ffprobe"] {
        if Path::new(c).exists() { return c.to_string(); }
    }
    which("ffprobe").unwrap_or_else(|| "ffprobe".to_string())
}

fn find_script(app: &tauri::AppHandle, name: &str) -> String {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let p = resource_dir.join("scripts").join(name);
        if p.exists() { return p.to_string_lossy().to_string(); }
    }
    let cwd_path = format!("scripts/{name}");
    if Path::new(&cwd_path).exists() { return cwd_path; }
    if let Ok(exe) = std::env::current_exe() {
        // target/debug/autoclipper → go up 4 levels to project root
        if let Some(root) = exe.parent().and_then(|p| p.parent())
            .and_then(|p| p.parent()).and_then(|p| p.parent())
        {
            let p = root.join("scripts").join(name);
            if p.exists() { return p.to_string_lossy().to_string(); }
        }
    }
    format!("scripts/{name}")
}

fn find_model_dir(vendor: Option<&Path>) -> Option<String> {
    let dir = vendor?.join("models");
    if dir.exists() { Some(dir.to_string_lossy().to_string()) } else { None }
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
        "9:16"     => (9.0, 16.0),
        "16:9"     => (16.0, 9.0),
        "1:1"      => (1.0, 1.0),
        "4:5"      => (4.0, 5.0),
        "original" => return None,
        _          => return None,
    };
    let src_ar = src_w as f64 / src_h as f64;
    let tgt_ar = tgt_aw / tgt_ah;
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
        entries.push(serde_json::json!({
            "start": cursor,
            "end": cursor + dur,
            "text": seg.text,
        }));
        cursor += dur;
    }
    entries
}

fn concat_segments(
    ffmpeg: &str,
    video_path: &str,
    selected: &[&SrtSegment],
    crop_filter: Option<&str>,
    dest: &str,
) -> Result<(), String> {
    let total = selected.len();
    let mut filter_parts: Vec<String> = Vec::new();
    let mut concat_inputs = String::new();

    for (i, seg) in selected.iter().enumerate() {
        let start = seg.start;
        let end = seg.end;
        let vf = if let Some(crop) = crop_filter {
            format!("[0:v]trim=start={start:.3}:end={end:.3},setpts=PTS-STARTPTS,{crop}[v{i}]")
        } else {
            format!("[0:v]trim=start={start:.3}:end={end:.3},setpts=PTS-STARTPTS[v{i}]")
        };
        filter_parts.push(format!(
            "{vf}; [0:a]atrim=start={start:.3}:end={end:.3},asetpts=PTS-STARTPTS[a{i}]"
        ));
        concat_inputs.push_str(&format!("[v{i}][a{i}]"));
    }
    let mut filter = filter_parts.join("; ");
    filter.push_str(&format!("; {concat_inputs}concat=n={total}:v=1:a=1[outv][outa]"));

    let status = Command::new(ffmpeg)
        .args(["-y", "-i", video_path,
               "-filter_complex", &filter,
               "-map", "[outv]", "-map", "[outa]",
               "-c:v", "libx264", "-preset", "fast", "-crf", "23",
               "-c:a", "aac", "-b:a", "128k",
               dest])
        .status()
        .map_err(|e| format!("Gagal menjalankan FFmpeg concat: {e}"))?;

    if status.success() { Ok(()) } else { Err("FFmpeg gagal menggabungkan segmen.".to_string()) }
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn check_dependencies(app: tauri::AppHandle) -> DepsStatus {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let mut checks: Vec<DepCheck> = Vec::new();

    let bundled = vendor.is_some();
    let source = if bundled { "bundled" } else { "system" };

    // Python
    let python = find_python(v);
    let python_ok = Command::new(&python).arg("--version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("Python 3 ({source})"),
        ok: python_ok,
        path: if python_ok { Some(python.clone()) } else { None },
        error: if !python_ok { Some("Python 3 tidak ditemukan".to_string()) } else { None },
        install_cmd: if !python_ok && !bundled { Some("brew install python".to_string()) } else { None },
        optional: false,
    });

    // FFmpeg
    let ffmpeg = find_ffmpeg(v);
    let ffmpeg_ok = Command::new(&ffmpeg).arg("-version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("FFmpeg ({source})"),
        ok: ffmpeg_ok,
        path: if ffmpeg_ok { Some(ffmpeg.clone()) } else { None },
        error: if !ffmpeg_ok { Some("FFmpeg tidak ditemukan".to_string()) } else { None },
        install_cmd: if !ffmpeg_ok && !bundled { Some("brew install ffmpeg".to_string()) } else { None },
        optional: false,
    });

    // ffprobe
    let ffprobe = find_ffprobe(v);
    let ffprobe_ok = Command::new(&ffprobe).arg("-version").output()
        .map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("ffprobe ({source})"),
        ok: ffprobe_ok,
        path: if ffprobe_ok { Some(ffprobe) } else { None },
        error: if !ffprobe_ok { Some("ffprobe tidak ditemukan".to_string()) } else { None },
        install_cmd: if !ffprobe_ok && !bundled { Some("brew install ffmpeg".to_string()) } else { None },
        optional: false,
    });

    // faster-whisper
    let whisper_ok = python_ok && Command::new(&python)
        .args(["-c", "import faster_whisper"])
        .output().map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("faster-whisper ({source})"),
        ok: whisper_ok,
        path: None,
        error: if !whisper_ok { Some("Package faster-whisper belum terinstall".to_string()) } else { None },
        install_cmd: if !whisper_ok && !bundled { Some("pip3 install faster-whisper".to_string()) } else { None },
        optional: false,
    });

    // Pillow
    let pillow_ok = python_ok && Command::new(&python)
        .args(["-c", "import PIL"])
        .output().map(|o| o.status.success()).unwrap_or(false);
    checks.push(DepCheck {
        name: format!("Pillow ({source})"),
        ok: pillow_ok,
        path: None,
        error: if !pillow_ok { Some("Package Pillow belum terinstall".to_string()) } else { None },
        install_cmd: if !pillow_ok && !bundled { Some("pip3 install Pillow".to_string()) } else { None },
        optional: false,
    });

    // Scripts
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

    // Ollama (optional)
    let ollama_ok = reqwest::Client::new()
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(3))
        .send().await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    checks.push(DepCheck {
        name: "Ollama (AI Analyze)".to_string(),
        ok: ollama_ok,
        path: if ollama_ok { Some("http://localhost:11434".to_string()) } else { None },
        error: if !ollama_ok { Some("Ollama tidak berjalan — fitur AI Analyze tidak tersedia".to_string()) } else { None },
        install_cmd: if !ollama_ok { Some("ollama serve  # lalu: ollama pull gemma4".to_string()) } else { None },
        optional: true,
    });

    let all_required_ok = checks.iter().filter(|c| !c.optional).all(|c| c.ok);
    DepsStatus { all_required_ok, checks }
}

#[tauri::command]
pub async fn transcribe_video(app: tauri::AppHandle, video_path: String) -> Result<TranscribeResult, String> {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let script = find_script(&app, "transcribe.py");
    let python = find_python(v);

    let mut args = vec![script.clone(), video_path.clone()];
    if let Some(model_dir) = find_model_dir(v) {
        args.push("--model-dir".to_string());
        args.push(model_dir);
    }

    let output = Command::new(&python)
        .args(&args)
        .output()
        .map_err(|e| format!("Gagal menjalankan Whisper: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Whisper error: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: TranscribeResult = serde_json::from_str(&stdout)
        .map_err(|e| format!("Gagal parse hasil transkripsi: {e}\nOutput: {stdout}"))?;

    Ok(result)
}

#[tauri::command]
pub async fn analyze_transcript(
    segments: Vec<SrtSegment>,
    prompt_override: Option<String>,
) -> Result<AnalyzeResult, String> {
    let transcript_text: String = segments.iter()
        .map(|s| format!("[{}] {}: {}", s.index, s.start_time, s.text))
        .collect::<Vec<_>>().join("\n");

    let system_prompt = "Kamu adalah asisten yang ahli dalam menganalisis transkrip video berbahasa Indonesia. \
        Tugasmu adalah memilih segmen-segmen paling penting dan informatif dari transkrip yang diberikan.";

    let user_prompt = prompt_override.unwrap_or_else(|| format!(
        "Berikut adalah transkrip video:\n\n{}\n\n\
        Pilih segmen-segmen yang paling penting, menarik, atau menjadi topik utama dari video ini. \
        Berikan response dalam format JSON berikut:\n\
        {{\"important_indices\": [list nomor index segmen yang penting], \"reasoning\": \"alasan pemilihan\"}}\n\
        Hanya balas dengan JSON, tidak perlu teks tambahan.",
        transcript_text
    ));

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": "gemma4:latest",
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "stream": false,
        "format": "json"
    });

    let response = client
        .post("http://localhost:11434/api/chat")
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send().await
        .map_err(|e| format!("Gagal koneksi ke Ollama: {e}. Pastikan Ollama berjalan."))?;

    if !response.status().is_success() {
        return Err(format!("Ollama error: HTTP {}", response.status()));
    }

    let resp_json: serde_json::Value = response.json().await
        .map_err(|e| format!("Gagal parse response Ollama: {e}"))?;

    let content = resp_json["message"]["content"]
        .as_str().ok_or("Response Ollama tidak memiliki content")?;

    serde_json::from_str(content)
        .map_err(|e| format!("Gagal parse JSON dari AI: {e}\nContent: {content}"))
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
) -> Result<ClipResult, String> {
    let vendor = vendor_dir(&app);
    let v = vendor.as_deref();
    let ffmpeg = find_ffmpeg(v);
    let ffprobe = find_ffprobe(v);
    let python = find_python(v);

    let mut selected: Vec<&SrtSegment> = segments.iter()
        .filter(|s| selected_indices.contains(&s.index))
        .collect();

    if selected.is_empty() {
        return Err("Tidak ada segmen yang dipilih".to_string());
    }
    selected.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());

    let total_duration: f64 = selected.iter().map(|s| s.end - s.start).sum();
    let total_segments = selected.len();

    let crop_filter: Option<String> = if aspect_ratio != "original" {
        let (w, h) = get_video_dims(&video_path, v)?;
        build_crop_filter(&aspect_ratio, w, h)
    } else {
        None
    };
    let crop_ref = crop_filter.as_deref();

    if burn_subtitles {
        let tmp_path = std::env::temp_dir().join("autoclipper_concat_tmp.mp4");
        let tmp_str = tmp_path.to_string_lossy().to_string();

        concat_segments(&ffmpeg, &video_path, &selected, crop_ref, &tmp_str)?;

        let entries = build_retimed_entries(&selected);
        let entries_path = std::env::temp_dir().join("autoclipper_entries.json");
        std::fs::write(&entries_path, serde_json::to_string(&entries).unwrap())
            .map_err(|e| format!("Gagal menulis entries JSON: {e}"))?;

        let script = find_script(&app, "burn_subtitles.py");
        let out = Command::new(&python)
            .args([&script, &tmp_str, &entries_path.to_string_lossy().to_string(), &output_path])
            .env("AUTOCLIPPER_FFMPEG", &ffmpeg)
            .env("AUTOCLIPPER_FFPROBE", &ffprobe)
            .output()
            .map_err(|e| format!("Gagal menjalankan burn_subtitles.py: {e}"))?;

        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_file(&entries_path);

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!("Subtitle burn gagal: {stderr}"));
        }
    } else {
        concat_segments(&ffmpeg, &video_path, &selected, crop_ref, &output_path)?;
    }

    let ar_note = if aspect_ratio != "original" { format!(" [{aspect_ratio}]") } else { String::new() };
    let sub_note = if burn_subtitles { " + subtitle" } else { "" };
    Ok(ClipResult {
        output_path,
        success: true,
        message: format!(
            "Berhasil menggabungkan {total_segments} segmen ({:.1}s total){sub_note}{ar_note}",
            total_duration
        ),
        total_segments,
        duration_secs: total_duration,
    })
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

    json["format"]["duration"]
        .as_str().and_then(|s| s.parse::<f64>().ok())
        .ok_or("Tidak bisa mendapatkan durasi video".to_string())
}
