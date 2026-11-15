//! Relift GUI - Tauri 2 backend library.
//!
//! This library contains the Tauri command handlers and state management
//! for the Relift GUI application.

pub mod commands;
pub mod state;

use tracing::info;

// ===== Re-export command types =====

pub use commands::{
    CandidateDto, DeviceInfoDto, FilesystemSummaryDto, ImageOptionsDto, RecoverOptionsDto,
    RecoveryProgressDto, RecoverySummaryDto, ScanOptionsDto, ScanProgressDto, SessionSummaryDto,
};
pub use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_devices_cmd,
            commands::scan_cmd,
            commands::cancel_scan_cmd,
            commands::list_candidates_cmd,
            commands::recover_cmd,
            commands::cancel_recover_cmd,
            commands::create_image_cmd,
            commands::doctor_cmd,
            commands::select_directory,
            commands::get_sessions_dir,
        ])
        .setup(|_app| {
            info!("Relift GUI started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
