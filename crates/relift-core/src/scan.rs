//! Scan orchestration: partition table, filesystem probe, metadata recovery and
//! optional validating carving, all streamed into a [`Session`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::carve::{scan_volume, CarveOptions, CarveSummary};
use crate::error::{Error, Result};
use crate::fs::{probe, scan_metadata, FsScanOptions, FsSummary, VolumeRef};
use crate::partitions::{read_partition_table, PartitionTable, Volume};
use crate::session::{
    FilesystemReport, ManualVolume, Session, SessionHeader, SessionOptions, SessionStatus,
};
use crate::source::ReadOnlySource;

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub session_dir: PathBuf,
    pub include_live: bool,
    pub carve: bool,
    pub volume_indices: Vec<u32>,
    pub manual_volume: Option<ManualVolume>,
    pub max_candidates: usize,
    pub cancelled: Arc<AtomicBool>,
}

impl ScanRequest {
    pub fn new(session_dir: impl Into<PathBuf>) -> Self {
        Self {
            session_dir: session_dir.into(),
            include_live: false,
            carve: false,
            volume_indices: Vec::new(),
            manual_volume: None,
            max_candidates: 1_000_000,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub phase: &'static str,
    pub done: u64,
    pub total: u64,
}

/// Run a scan and persist it. On error the session header is marked failed when
/// possible, so a partial scan is never mistaken for a complete one.
pub fn run_scan(
    src: &dyn ReadOnlySource,
    request: &ScanRequest,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<SessionHeader> {
    let options = SessionOptions {
        include_live: request.include_live,
        carve: request.carve,
        max_candidates: request.max_candidates.max(1),
        volume_indices: request.volume_indices.clone(),
        manual_volume: request.manual_volume,
    };
    let mut session = Session::create(&request.session_dir, src.identity(), options)?;
    let desc = src.description();
    for w in &desc.warnings {
        session.header_mut().warnings.push(w.clone());
    }
    if !desc.read_only {
        let _ = session.fail("source description reported a writable handle; refusing to scan");
        return Err(Error::invalid("source is not read-only; refusing to scan"));
    }

    let outcome = scan_inner(src, request, &mut session, progress);
    match outcome {
        Ok(()) => {
            let status = if request.is_cancelled() {
                SessionStatus::Cancelled
            } else {
                SessionStatus::Complete
            };
            session.finish(status)?;
            Ok(session.header().clone())
        }
        Err(e) => {
            let _ = session.fail(e.to_string());
            Err(e)
        }
    }
}

fn scan_inner(
    src: &dyn ReadOnlySource,
    request: &ScanRequest,
    session: &mut Session,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<()> {
    let total_source = src.len();
    progress(ScanProgress {
        phase: "probe",
        done: 0,
        total: total_source,
    });

    let (table, volumes) = resolve_volumes(src, request)?;
    session.header_mut().partition_table = Some(table);
    let mut volume_reports: Vec<FilesystemReport> = Vec::new();

    for volume in &volumes {
        if request.is_cancelled() {
            break;
        }
        progress(ScanProgress {
            phase: "metadata",
            done: volume.offset,
            total: total_source,
        });
        let vol_ref = VolumeRef {
            offset: volume.offset,
            length: volume.length,
            label: volume.label.clone(),
        };
        let mut report = FilesystemReport {
            volume_index: volume.index,
            offset: volume.offset,
            length: volume.length,
            label: volume.label.clone(),
            kind: None,
            info: None,
            scan: None,
            carving: None,
        };
        let info = probe(src, &vol_ref);
        let mut scan_summary = FsSummary::default();
        if let Some(info) = &info {
            report.kind = Some(info.kind.as_str().to_string());
            report.label = info.label.clone().or(report.label);
            report.info = Some(info.clone());
            for w in &info.warnings {
                push_warning(&mut session.header_mut().warnings, w);
            }
            if info.kind.metadata_supported() {
                let remaining = remaining_candidates(request, session);
                if remaining == 0 {
                    session
                        .header_mut()
                        .warnings
                        .push("candidate limit reached before metadata scan".into());
                    scan_summary.warnings.push("candidate limit reached".into());
                } else {
                    let fs_options = FsScanOptions {
                        include_live: request.include_live,
                        max_candidates: remaining,
                        cancelled: request.cancelled.clone(),
                    };
                    scan_summary = scan_metadata(src, &vol_ref, &fs_options, session)?;
                }
            } else {
                scan_summary.warnings.push(format!(
                    "{} is identified but metadata recovery is not implemented",
                    info.kind.as_str()
                ));
            }
        } else {
            scan_summary.warnings.push(format!(
                "no known filesystem signature at offset {:#x}; metadata recovery skipped",
                volume.offset
            ));
        }
        for w in &scan_summary.warnings {
            push_warning(&mut session.header_mut().warnings, w);
        }
        report.scan = Some(scan_summary);
        progress(ScanProgress {
            phase: "metadata",
            done: volume.offset.saturating_add(volume.length),
            total: total_source,
        });

        if request.carve && !request.is_cancelled() {
            let remaining = remaining_candidates(request, session);
            if remaining == 0 {
                report.carving = Some(CarveSummary {
                    truncated: true,
                    warnings: vec!["candidate limit reached before carving".into()],
                    ..Default::default()
                });
            } else {
                let carve_options = CarveOptions {
                    max_candidates: remaining,
                    min_size: 512,
                    cancelled: request.cancelled.clone(),
                };
                let mut carve_progress = |done: u64, total: u64| {
                    progress(ScanProgress {
                        phase: "carve",
                        done: volume.offset.saturating_add(done),
                        total: total_source.max(total),
                    });
                };
                let carving =
                    scan_volume(src, &vol_ref, &carve_options, session, &mut carve_progress)?;
                for w in &carving.warnings {
                    push_warning(&mut session.header_mut().warnings, w);
                }
                report.carving = Some(carving);
            }
        }
        volume_reports.push(report);
        session.header_mut().filesystems = volume_reports.clone();
        session.save_header()?;
    }

    progress(ScanProgress {
        phase: "done",
        done: total_source,
        total: total_source,
    });
    if request.is_cancelled() {
        session
            .header_mut()
            .warnings
            .push("scan cancelled by user; results are partial".into());
    }
    Ok(())
}

fn remaining_candidates(request: &ScanRequest, session: &Session) -> usize {
    request
        .max_candidates
        .max(1)
        .saturating_sub(session.header().summary.candidates as usize)
}

fn push_warning(warnings: &mut Vec<String>, message: &str) {
    if warnings.len() >= 128 || warnings.iter().any(|w| w == message) {
        return;
    }
    warnings.push(message.to_string());
}

fn resolve_volumes(
    src: &dyn ReadOnlySource,
    request: &ScanRequest,
) -> Result<(PartitionTable, Vec<Volume>)> {
    if let Some(manual) = request.manual_volume {
        if manual.length == 0 {
            return Err(Error::invalid(
                "manual volume length must be greater than zero",
            ));
        }
        if manual
            .offset
            .checked_add(manual.length)
            .map(|end| end > src.len())
            .unwrap_or(true)
        {
            return Err(Error::invalid(format!(
                "manual volume {:#x}..+{:#x} lies outside the source length {:#x}",
                manual.offset,
                manual.length,
                src.len()
            )));
        }
        let volume = Volume {
            index: 0,
            offset: manual.offset,
            length: manual.length,
            scheme: crate::partitions::PartitionScheme::None,
            type_hint: "manual range".into(),
            label: None,
        };
        return Ok((PartitionTable::none(), vec![volume]));
    }

    let table = read_partition_table(src)?;
    let mut volumes = table.volumes.clone();
    if volumes.is_empty() {
        volumes.push(Volume {
            index: 0,
            offset: 0,
            length: src.len(),
            scheme: crate::partitions::PartitionScheme::None,
            type_hint: "whole source".into(),
            label: None,
        });
    }
    if !request.volume_indices.is_empty() {
        let mut selected = Vec::new();
        for idx in &request.volume_indices {
            let found = volumes
                .iter()
                .find(|v| v.index == *idx)
                .ok_or_else(|| Error::invalid(format!("volume {idx} does not exist")))?;
            selected.push(found.clone());
        }
        volumes = selected;
    }
    Ok((table, volumes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{load_header, SessionStatus};
    use crate::source::FileSource;
    use relift_testkit::{exfat_image, ext4_image, fat32_image, ntfs_image};

    #[test]
    fn scans_fat32_fixture_into_a_session() {
        let dir = std::env::temp_dir().join(format!("relift-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image_path = dir.join("fat32.img");
        let fixture = fat32_image();
        std::fs::write(&image_path, fixture.image()).unwrap();
        let src = FileSource::open(&image_path).unwrap();
        let session_dir = dir.join("session");
        let request = ScanRequest::new(&session_dir);
        let mut progress = |_p: ScanProgress| {};
        let header = run_scan(&src, &request, &mut progress).unwrap();
        assert_eq!(header.status, SessionStatus::Complete);
        assert_eq!(header.filesystems.len(), 1);
        assert!(header.summary.deleted >= 1);
        let loaded = load_header(&session_dir).unwrap();
        assert_eq!(loaded.summary.candidates, header.summary.candidates);
        let candidates = crate::session::load_candidates(&session_dir).unwrap();
        assert!(
            candidates.iter().any(|c| c.name == fixture.deleted_name),
            "wanted {:?}, got {:?}",
            fixture.deleted_name,
            candidates
                .iter()
                .map(|c| (&c.name, &c.method, &c.data))
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scans_exfat_fixture_into_a_session() {
        let dir = std::env::temp_dir().join(format!("relift-scan-exfat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image_path = dir.join("exfat.img");
        let fixture = exfat_image();
        std::fs::write(&image_path, fixture.image()).unwrap();
        let src = FileSource::open(&image_path).unwrap();
        let session_dir = dir.join("session");
        let request = ScanRequest::new(&session_dir);
        let mut progress = |_p: ScanProgress| {};
        let header = run_scan(&src, &request, &mut progress).unwrap();
        assert_eq!(header.status, SessionStatus::Complete);
        assert_eq!(header.filesystems.len(), 1);
        assert_eq!(
            header.filesystems[0].scan.as_ref().unwrap().kind,
            Some(crate::fs::FsKind::ExFat)
        );
        let candidates = crate::session::load_candidates(&session_dir).unwrap();
        assert!(
            header.summary.deleted >= 3,
            "deleted {}, candidates {:?}, warnings {:?}",
            header.summary.deleted,
            candidates.iter().map(|c| &c.name).collect::<Vec<_>>(),
            header
                .filesystems
                .first()
                .and_then(|f| f.scan.as_ref())
                .map(|s| &s.warnings)
        );
        assert!(
            candidates.iter().any(|c| c.name == fixture.deleted_name),
            "wanted {:?}, got {:?}",
            fixture.deleted_name,
            candidates.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
        assert!(candidates
            .iter()
            .any(|c| c.name == "holiday photo 2024.jpg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scans_ntfs_fixture_into_a_session() {
        let dir = std::env::temp_dir().join(format!("relift-scan-ntfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image_path = dir.join("ntfs.img");
        let fixture = ntfs_image();
        std::fs::write(&image_path, fixture.image()).unwrap();
        let src = FileSource::open(&image_path).unwrap();
        let session_dir = dir.join("session");
        let request = ScanRequest::new(&session_dir);
        let mut progress = |_p: ScanProgress| {};
        let header = run_scan(&src, &request, &mut progress).unwrap();
        assert_eq!(header.status, SessionStatus::Complete);
        let candidates = crate::session::load_candidates(&session_dir).unwrap();
        assert!(
            candidates.iter().any(|c| c.name == fixture.deleted_name),
            "wanted {:?}, got {:?}, warnings {:?}",
            fixture.deleted_name,
            candidates
                .iter()
                .map(|c| (&c.name, &c.path))
                .collect::<Vec<_>>(),
            header
                .filesystems
                .first()
                .and_then(|f| f.scan.as_ref())
                .map(|s| &s.warnings)
        );
        assert!(candidates
            .iter()
            .any(|c| c.name == "photo.jpg" && c.path == "docs"));
        assert!(candidates
            .iter()
            .any(|c| c.name == "hidden.txt" && c.path == "old dir"));
        assert!(candidates.iter().any(|c| c.name == "sparse.bin"));
        assert!(candidates.iter().any(|c| c.name == "no-content.bin"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scans_ext4_fixture_into_a_session() {
        let dir = std::env::temp_dir().join(format!("relift-scan-ext4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image_path = dir.join("ext4.img");
        let fixture = ext4_image();
        std::fs::write(&image_path, fixture.image()).unwrap();
        let src = FileSource::open(&image_path).unwrap();
        let session_dir = dir.join("session");
        let request = ScanRequest::new(&session_dir);
        let mut progress = |_p: ScanProgress| {};
        let header = run_scan(&src, &request, &mut progress).unwrap();
        assert_eq!(header.status, SessionStatus::Complete);
        assert_eq!(
            header.filesystems[0].scan.as_ref().unwrap().kind,
            Some(crate::fs::FsKind::Ext4)
        );
        let candidates = crate::session::load_candidates(&session_dir).unwrap();
        let names: Vec<(&str, &str)> = candidates
            .iter()
            .map(|c| (c.name.as_str(), c.path.as_str()))
            .collect();
        assert!(
            candidates.iter().any(|c| c.name == fixture.deleted_name),
            "wanted {:?}, got {:?}, warnings {:?}",
            fixture.deleted_name,
            names,
            header
                .filesystems
                .first()
                .and_then(|f| f.scan.as_ref())
                .map(|s| &s.warnings)
        );
        assert!(candidates
            .iter()
            .any(|c| c.name == "holiday photo 2024.jpg"));
        assert!(candidates
            .iter()
            .any(|c| c.name == "deleted_notes.txt" && c.path == "docs"));
        assert!(candidates
            .iter()
            .any(|c| c.name == "old.txt" && c.path == "old dir"));
        assert!(candidates.iter().any(|c| c.name == "inode_20"));
        assert!(candidates
            .iter()
            .any(|c| c.name == "indirect.bin" && c.has_content()));
        assert!(!candidates.iter().any(|c| c.name == fixture.live_name));

        let sparse = candidates
            .iter()
            .find(|c| c.name == "sparse.bin")
            .expect("sparse.bin");
        let crate::candidate::DataLocation::Segments { segments } = &sparse.data else {
            panic!("sparse.bin should use segments: {:?}", sparse.data);
        };
        assert_eq!(segments.len(), 3, "{segments:?}");
        assert!(matches!(
            segments[1],
            crate::candidate::Segment::Zero { length: 1024 }
        ));

        let unwritten = candidates
            .iter()
            .find(|c| c.name == "unwritten.bin")
            .expect("unwritten.bin");
        let crate::candidate::DataLocation::Segments { segments } = &unwritten.data else {
            panic!("unwritten.bin should use segments: {:?}", unwritten.data);
        };
        assert_eq!(segments.len(), 1);
        assert!(matches!(
            segments[0],
            crate::candidate::Segment::Zero { length: 1024 }
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
