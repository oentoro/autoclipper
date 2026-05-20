use serde::{Deserialize, Serialize};
use std::process::Command;
use std::path::Path;

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
}

fn find_python() -> String {
    let candidates = ["/opt/homebrew/bin/python3", "/usr/bin/python3", "python3"];
    for c in candidates {
        if Path::new(c).exists() || c == "python3" {
            return c.to_string();
        }
    }
    "python3".to_string()
}

fn find_ffmpeg() -> String {
    let candidates = ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "ffmpeg"];
    for c in candidates {
        if Path::new(c).exists() {
            return c.to_string();
        }
    }
    "ffmpeg".to_string()
}

#[tauri::command]
pub async fn transcribe_video(video_path: String) -> Result<TranscribeResult, String> {
    let script_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("../../../scripts/transcribe.py")))
        .unwrap_or_else(|| std::path::PathBuf::from("scripts/transcribe.py"));

    let script = if script_path.exists() {
        script_path.to_string_lossy().to_string()
    } else {
        "scripts/transcribe.py".to_string()
    };

    let python = find_python();
    let output = Command::new(&python)
        .args([&script, &video_path])
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
    let transcript_text: String = segments
        .iter()
        .map(|s| format!("[{}] {}: {}", s.index, s.start_time, s.text))
        .collect::<Vec<_>>()
        .join("\n");

    let system_prompt = "Kamu adalah asisten yang ahli dalam menganalisis transkrip video berbahasa Indonesia. \
        Tugasmu adalah memilih segmen-segmen paling penting dan informatif dari transkrip yang diberikan.";

    let user_prompt = prompt_override.unwrap_or_else(|| {
        format!(
            "Berikut adalah transkrip video:\n\n{}\n\n\
            Pilih segmen-segmen yang paling penting, menarik, atau menjadi topik utama dari video ini. \
            Berikan response dalam format JSON berikut:\n\
            {{\"important_indices\": [list nomor index segmen yang penting], \"reasoning\": \"alasan pemilihan\"}}\n\
            Hanya balas dengan JSON, tidak perlu teks tambahan.",
            transcript_text
        )
    });

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
        .send()
        .await
        .map_err(|e| format!("Gagal koneksi ke Ollama: {e}. Pastikan Ollama berjalan."))?;

    if !response.status().is_success() {
        return Err(format!("Ollama error: HTTP {}", response.status()));
    }

    let resp_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Gagal parse response Ollama: {e}"))?;

    let content = resp_json["message"]["content"]
        .as_str()
        .ok_or("Response Ollama tidak memiliki content")?;

    let result: AnalyzeResult = serde_json::from_str(content)
        .map_err(|e| format!("Gagal parse JSON dari AI: {e}\nContent: {content}"))?;

    Ok(result)
}

#[tauri::command]
pub async fn clip_video(
    video_path: String,
    segments: Vec<SrtSegment>,
    selected_indices: Vec<usize>,
    output_dir: String,
) -> Result<Vec<ClipResult>, String> {
    let ffmpeg = find_ffmpeg();
    let video_name = Path::new(&video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    let selected: Vec<&SrtSegment> = segments
        .iter()
        .filter(|s| selected_indices.contains(&s.index))
        .collect();

    if selected.is_empty() {
        return Err("Tidak ada segmen yang dipilih".to_string());
    }

    let mut results = Vec::new();
    let output_path_dir = Path::new(&output_dir);

    for (i, seg) in selected.iter().enumerate() {
        let output_filename = format!("{}_clip_{:03}_{}.mp4", video_name, i + 1, seg.index);
        let output_path = output_path_dir.join(&output_filename);

        let duration = seg.end - seg.start;
        let start_str = format!("{:.3}", seg.start);
        let duration_str = format!("{:.3}", duration);

        let status = Command::new(&ffmpeg)
            .args([
                "-y",
                "-ss", &start_str,
                "-i", &video_path,
                "-t", &duration_str,
                "-c:v", "libx264",
                "-c:a", "aac",
                "-avoid_negative_ts", "make_zero",
                output_path.to_str().unwrap_or(""),
            ])
            .status()
            .map_err(|e| format!("Gagal menjalankan FFmpeg: {e}"))?;

        if status.success() {
            results.push(ClipResult {
                output_path: output_path.to_string_lossy().to_string(),
                success: true,
                message: format!("Clip {} berhasil dibuat", i + 1),
            });
        } else {
            results.push(ClipResult {
                output_path: output_path.to_string_lossy().to_string(),
                success: false,
                message: format!("Gagal membuat clip {}", i + 1),
            });
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn get_video_duration(video_path: String) -> Result<f64, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            &video_path,
        ])
        .output()
        .map_err(|e| format!("Gagal menjalankan ffprobe: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|e| format!("Parse error: {e}"))?;

    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("Tidak bisa mendapatkan durasi video")?;

    Ok(duration)
}
