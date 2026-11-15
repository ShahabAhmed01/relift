//! Tauri command handlers for Relift GUI.

use relift_core::{
    candidate::ConfidenceLabel,
    devices::list_devices,
    imaging::{create_image, ImageOptions},
    recover::{run_recovery, RecoverOptions},
    scan::{run_scan, ScanProgress, ScanRequest},
    session::load_header,
    source::open_source,
};

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State, Window};

// ===== Tauri Commands =====

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeviceInfoDto {
    pub path: String,
    pub kind: String,
    pub label: Option<String>,
    pub filesystem: Option<String>,
    pub size: Option<u64>,
    pub size_human: String,
    pub accessible: bool,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanOptionsDto {
    pub source: String,
    pub session_dir: String,
    pub include_live: bool,
    pub carve: bool,
    pub volume_indices: Vec<u32>,
    pub max_candidates: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecoverOptionsDto {
    pub session_dir: String,
    pub dest: String,
    pub min_confidence: Option<String>,
    pub ids: Option<Vec<u64>>,
    pub verify: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageOptionsDto {
    pub source: String,
    pub dest: String,
    pub verify: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScanProgressDto {
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub percent: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecoveryProgressDto {
    pub index: u64,
    pub total: u64,
    pub percent: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionSummaryDto {
    pub session_dir: String,
    pub source_path: String,
    pub source_size: u64,
    pub source_size_human: String,
    pub status: String,
    pub filesystems: Vec<FilesystemSummaryDto>,
    pub candidates: u64,
    pub deleted: u64,
    pub live: u64,
    pub with_content: u64,
    pub metadata_only: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FilesystemSummaryDto {
    pub volume_index: u32,
    pub offset: u64,
    pub length: u64,
    pub label: Option<String>,
    pub kind: Option<String>,
    pub candidates: u64,
    pub deleted: u64,
    pub live: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecoverySummaryDto {
    pub destination: String,
    pub considered: u64,
    pub attempted: u64,
    pub succeeded: u64,
    pub partial: u64,
    pub failed: u64,
    pub skipped: u64,
    pub bytes_written: u64,
    pub bytes_written_human: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CandidateDto {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub size_human: String,
    pub format: String,
    pub confidence: String,
    pub confidence_score: u8,
    pub confidence_reasons: Vec<String>,
    pub method: String,
    pub state: String,
    pub modified: Option<String>,
    pub accessed: Option<String>,
    pub created: Option<String>,
    pub volume_offset: u64,
    pub record_id: Option<String>,
    pub complete: bool,
    pub validation: Vec<String>,
}

#[tauri::command]
pub async fn list_devices_cmd() -> Result<Vec<DeviceInfoDto>, String> {
    let list = list_devices();
    let mut devices = Vec::new();
    for device in list.devices {
        let size_human = device
            .size
            .map(relift_core::util::human_bytes)
            .unwrap_or_else(|| "unknown".to_string());
        devices.push(DeviceInfoDto {
            path: device.path,
            kind: format!("{:?}", device.kind).to_lowercase(),
            label: device.label,
            filesystem: device.filesystem,
            size: device.size,
            size_human,
            accessible: device.accessible,
            note: device.note,
        });
    }
    Ok(devices)
}

#[tauri::command]
pub async fn scan_cmd(
    app: AppHandle,
    options: ScanOptionsDto,
    state: State<'_, AppState>,
) -> Result<SessionSummaryDto, String> {
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    *state.scan_cancel.lock().unwrap() = Some(cancel_flag.clone());

    let src = open_source(&options.source).map_err(|e| format!("could not open source: {}", e))?;

    let identity = src.identity();
    if identity.kind == relift_core::source::SourceKind::RawDevice {
        // Warn but continue
    }

    let mut request = ScanRequest::new(&options.session_dir);
    request.include_live = options.include_live;
    request.carve = options.carve;
    request.volume_indices = options.volume_indices;
    request.max_candidates = options.max_candidates.max(1);
    request.cancelled = cancel_flag;

    let app_handle = app.clone();
    let mut progress = move |p: ScanProgress| {
        let percent = if p.total == 0 {
            100
        } else {
            ((p.done.min(p.total) as u128 * 100) / p.total as u128) as u32
        };
        let _ = app_handle.emit(
            "scan-progress",
            ScanProgressDto {
                phase: p.phase.to_string(),
                done: p.done,
                total: p.total,
                percent,
            },
        );
    };

    let header = run_scan(src.as_ref(), &request, &mut progress)
        .map_err(|e| format!("scan failed: {}", e))?;

    let summary = SessionSummaryDto {
        session_dir: options.session_dir,
        source_path: header.source.path,
        source_size: header.source.size,
        source_size_human: relift_core::util::human_bytes(header.source.size),
        status: format!("{:?}", header.status).to_lowercase(),
        filesystems: header
            .filesystems
            .iter()
            .map(|fs| {
                let scan = fs.scan.as_ref();
                FilesystemSummaryDto {
                    volume_index: fs.volume_index,
                    offset: fs.offset,
                    length: fs.length,
                    label: fs.label.clone(),
                    kind: fs.kind.clone(),
                    candidates: scan.map(|s| s.emitted).unwrap_or(0),
                    deleted: scan.map(|s| s.deleted_entries).unwrap_or(0),
                    live: scan.map(|s| s.live_entries).unwrap_or(0),
                    warnings: fs
                        .scan
                        .as_ref()
                        .map(|s| s.warnings.clone())
                        .unwrap_or_default(),
                }
            })
            .collect(),
        candidates: header.summary.candidates,
        deleted: header.summary.deleted,
        live: header.summary.live,
        with_content: header.summary.with_content,
        metadata_only: header.summary.metadata_only,
        warnings: header.warnings,
    };

    Ok(summary)
}

#[tauri::command]
pub async fn cancel_scan_cmd(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(flag) = state.scan_cancel.lock().unwrap().take() {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn list_candidates_cmd(session_dir: String) -> Result<Vec<CandidateDto>, String> {
    let candidates = relift_core::session::load_candidates(std::path::PathBuf::from(&session_dir))
        .map_err(|e| format!("could not load candidates: {}", e))?;

    Ok(candidates
        .into_iter()
        .map(|c| CandidateDto {
            id: c.id,
            name: c.name,
            path: c.path,
            size: c.size,
            size_human: relift_core::util::human_bytes(c.size),
            format: c.format,
            confidence: format!("{:?}", c.confidence.label).to_lowercase(),
            confidence_score: c.confidence.score,
            confidence_reasons: c.confidence.reasons,
            method: format!("{:?}", c.method).to_lowercase(),
            state: format!("{:?}", c.state).to_lowercase(),
            modified: c.modified,
            accessed: c.accessed,
            created: c.created,
            volume_offset: c.volume_offset,
            record_id: c.record_id,
            complete: c.complete,
            validation: c.validation,
        })
        .collect())
}

#[tauri::command]
pub async fn recover_cmd(
    app: AppHandle,
    options: RecoverOptionsDto,
    state: State<'_, AppState>,
) -> Result<RecoverySummaryDto, String> {
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    *state.recover_cancel.lock().unwrap() = Some(cancel_flag.clone());

    let header = load_header(std::path::PathBuf::from(&options.session_dir))
        .map_err(|e| format!("could not read session: {}", e))?;

    let source_path = std::path::PathBuf::from(&header.source.path);
    let src = open_source(&source_path).map_err(|e| format!("could not open source: {}", e))?;

    let dest = std::path::PathBuf::from(&options.dest);
    if !dest.exists() {
        std::fs::create_dir_all(&dest)
            .map_err(|e| format!("could not create destination: {}", e))?;
    }

    let min_confidence =
        options
            .min_confidence
            .as_deref()
            .and_then(|v| match v.to_lowercase().as_str() {
                "verified" => Some(ConfidenceLabel::Verified),
                "likely" => Some(ConfidenceLabel::Likely),
                "partial" => Some(ConfidenceLabel::Partial),
                "unverified" => Some(ConfidenceLabel::Unverified),
                _ => None,
            });

    let mut recover_options = RecoverOptions::new(dest);
    recover_options.verify = options.verify;
    recover_options.min_confidence = min_confidence;
    recover_options.ids = options.ids.map(|v| v.into_iter().collect());

    let app_handle = app.clone();
    let mut progress = move |p: relift_core::recover::RecoveryProgress| {
        let percent = if p.total == 0 {
            100
        } else {
            ((p.index.min(p.total) as u128 * 100) / p.total as u128) as u32
        };
        let _ = app_handle.emit(
            "recover-progress",
            RecoveryProgressDto {
                index: p.index,
                total: p.total,
                percent,
            },
        );
    };

    let summary = run_recovery(
        std::path::Path::new(&options.session_dir),
        src.as_ref(),
        &recover_options,
        &mut progress,
    )
    .map_err(|e| format!("recovery failed: {}", e))?;

    Ok(RecoverySummaryDto {
        destination: summary.destination,
        considered: summary.considered,
        attempted: summary.attempted,
        succeeded: summary.succeeded,
        partial: summary.partial,
        failed: summary.failed,
        skipped: summary.skipped,
        bytes_written: summary.bytes_written,
        bytes_written_human: relift_core::util::human_bytes(summary.bytes_written),
        warnings: summary.warnings,
    })
}

#[tauri::command]
pub async fn cancel_recover_cmd(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(flag) = state.recover_cancel.lock().unwrap().take() {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn create_image_cmd(
    app: AppHandle,
    options: ImageOptionsDto,
) -> Result<serde_json::Value, String> {
    let src = open_source(&options.source).map_err(|e| format!("could not open source: {}", e))?;

    let app_handle = app.clone();
    let mut progress = move |done: u64, total: u64| {
        let percent = if total == 0 {
            100
        } else {
            ((done.min(total) as u128 * 100) / total as u128) as u32
        };
        let _ = app_handle.emit(
            "image-progress",
            ScanProgressDto {
                phase: "image".to_string(),
                done,
                total,
                percent,
            },
        );
    };

    let image_options = ImageOptions {
        verify: options.verify,
        ..Default::default()
    };

    let result = create_image(
        src.as_ref(),
        &std::path::PathBuf::from(&options.dest),
        &image_options,
        &mut progress,
    )
    .map_err(|e| format!("imaging failed: {}", e))?;

    Ok(serde_json::json!({
        "path": result.path,
        "bytes": result.bytes,
        "bytes_human": relift_core::util::human_bytes(result.bytes),
        "sha256": result.sha256,
        "verified": result.verified,
        "elapsed_seconds": result.elapsed_seconds,
    }))
}

#[tauri::command]
pub async fn doctor_cmd() -> Result<serde_json::Value, String> {
    let report = relift_core::devices::doctor();
    serde_json::to_value(report).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn select_directory(window: Window) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    window.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p.map(|p| p.to_string()));
    });
    let result = rx.await.map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command]
pub async fn get_sessions_dir() -> Result<String, String> {
    let base = std::env::temp_dir();
    Ok(base.to_string_lossy().to_string())
}

#[cfg(test)]
mod dto_contract_tests {
    use super::{DeviceInfoDto, ScanOptionsDto};

    #[test]
    fn scan_options_deserialize_snake_case_fields() {
        let options: ScanOptionsDto = serde_json::from_value(serde_json::json!({
            "source": "image.dd",
            "session_dir": "session",
            "include_live": false,
            "carve": true,
            "volume_indices": [0],
            "max_candidates": 10
        }))
        .unwrap();

        assert_eq!(options.session_dir, "session");
        assert!(options.carve);
    }

    #[test]
    fn device_info_serializes_only_supported_fields_in_snake_case() {
        let device = DeviceInfoDto {
            path: "image.dd".to_string(),
            kind: "file".to_string(),
            label: None,
            filesystem: None,
            size: Some(1024),
            size_human: "1 KiB".to_string(),
            accessible: true,
            note: None,
        };
        let value = serde_json::to_value(device).unwrap();

        assert_eq!(value["size_human"], "1 KiB");
        assert!(value.get("sizeHuman").is_none());
        assert!(value.get("health").is_none());
        assert!(value.get("is_system_drive").is_none());
    }
}
