mod commands;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(commands::DownloadState::default())
        .manage(commands::LlamaServerState::default())
        .manage(commands::WhisperDownloadState::default())
        .manage(commands::ProcessCancelState::default())
        .manage(commands::YtDownloadState::default())
        .invoke_handler(tauri::generate_handler![
            check_dependencies,
            get_system_fonts,
            read_font_base64,
            transcribe_video,
            translate_transcript,
            analyze_transcript,
            classify_transcript,
            list_llm_models,
            download_llm_model,
            delete_llm_model,
            cancel_llm_download,
            get_partial_downloads,
            discard_partial_download,
            clip_video,
            get_video_duration,
            reveal_in_file_manager,
            install_dependency,
            create_venv,
            cancel_transcription,
            cancel_clipping,
            download_youtube,
            cancel_youtube_download,
            check_license,
            activate_license,
            deactivate_license,
            list_whisper_models,
            download_whisper_model,
            cancel_whisper_download,
            delete_whisper_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
