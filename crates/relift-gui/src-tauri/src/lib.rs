//! Relift GUI - Tauri 2 backend library.
//!
//! This library contains the Tauri command handlers and state management
//! for the Relift GUI application.

pub mod commands;
pub mod state;

use relift_core::{
    candidate::ConfidenceLabel,
    devices::{list_devices, DeviceInfo, DeviceKind},
    error::Error,
    imaging::{create_image, ImageOptions},
    recover::{run_recovery, RecoverOptions, RecoverySummary},
    scan::{run_scan, ScanProgress, ScanRequest},
    session::{load_header, ManualVolume, SessionHeader, SessionStatus},
    source::open_source,
    util::human_bytes,
};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{error, info};

// ===== Re-export command types =====

pub use commands::{
    AppState, CandidateDto, DeviceInfoDto, FilesystemSummaryDto, ImageOptionsDto,
    RecoverOptionsDto, RecoveryProgressDto, RecoverySummaryDto, ScanOptionsDto, ScanProgressDto,
    SessionSummaryDto,
};

// ===== Tauri Commands =====

#[tauri::command]
async fn list_devices_cmd() -> Result<Vec<commands::DeviceInfoDto>, String> {
    commands::list_devices_cmd().await
}

#[tauri::command]
async fn scan_cmd(
    app: tauri::AppHandle,
    options: commands::ScanOptionsDto,
    state: State<'_, commands::AppState>,
) -> Result<commands::SessionSummaryDto, String> {
    commands::scan_cmd(app, options, state).await
}

#[tauri::command]
async fn cancel_scan_cmd(state: State<'_, commands::AppState>) -> Result<(), String> {
    commands::cancel_scan_cmd(state).await
}

#[tauri::command]
async fn list_candidates_cmd(session_dir: String) -> Result<Vec<commands::CandidateDto>, String> {
    commands::list_candidates_cmd(session_dir).await
}

#[tauri::command]
async fn recover_cmd(
    app: tauri::AppHandle,
    options: commands::RecoverOptionsDto,
    state: State<'_, commands::AppState>,
) -> Result<commands::RecoverySummaryDto, String> {
    commands::recover_cmd(app, options, state).await
}

#[tauri::command]
async fn cancel_recover_cmd(state: State<'_, commands::AppState>) -> Result<(), String> {
    commands::cancel_recover_cmd(state).await
}

#[tauri::command]
async fn create_image_cmd(
    app: tauri::AppHandle,
    options: commands::ImageOptionsDto,
) -> Result<serde_json::Value, String> {
    commands::create_image_cmd(app, options).await
}

#[tauri::command]
async fn doctor_cmd() -> Result<serde_json::Value, String> {
    commands::doctor_cmd().await
}

#[tauri::command]
async fn select_directory(window: tauri::Window) -> Result<Option<String>, String> {
    commands::select_directory(window).await
}

#[tauri::command]
async fn get_sessions_dir() -> Result<String, String> {
    commands::get_sessions_dir().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            list_devices_cmd,
            scan_cmd,
            cancel_scan_cmd,
            list_candidates_cmd,
            recover_cmd,
            cancel_recover_cmd,
            create_image_cmd,
            doctor_cmd,
            select_directory,
            get_sessions_dir,
        ])
        .setup(|app| {
            info!("Relift GUI started");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
