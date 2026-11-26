//! Scan orchestration: partition table, filesystem probe, metadata recovery and
//! optional validating carving, all streamed into a [`Session`].

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::carve::{scan_volume, CarveOptions, CarveSummary};
use crate::error::{Error, Result};
use crate::fs::{probe, scan_metadata, FsScanOptions, FsSummary, VolumeRef};
use crate::partitions::{read_partition_table, PartitionTable, Volume};
use crate::session::{
    load_header, now_iso, FilesystemReport, ManualVolume, Session, SessionHeader, SessionOptions,
    SessionStatus,
};
use crate::source::{ReadOnlySource, SourceKind};

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub session_dir: PathBuf,
    pub include_live: bool,
    pub carve: bool,
    pub volume_indices: Vec<u32>,
    pub manual_volume: Option<ManualVolume>,
    pub max_candidates: usize,
    pub cancelled: Arc<AtomicBool>,
    /// Continue an interrupted session instead of creating a new one. The
    /// session's recorded options are used; `session_dir` must exist.
    pub resume: bool,
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
            resume: false,
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
///
/// With [`ScanRequest::resume`] set, an existing session is revalidated against
/// the source and continued from its last committed volume.
pub fn run_scan(
    src: &dyn ReadOnlySource,
    request: &ScanRequest,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<SessionHeader> {
    if request.resume {
        resume_scan(src, request, progress)
    } else {
        create_scan(src, request, progress)
    }
}

fn create_scan(
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

    finish_scan(src, request, &mut session, progress)
}

fn resume_scan(
    src: &dyn ReadOnlySource,
    request: &ScanRequest,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<SessionHeader> {
    let header = load_header(&request.session_dir)?;
    ensure_resumable(&header, src)?;
    let stored = header.options.clone();
    let mut session = Session::resume(&request.session_dir, header)?;

    let current = src.identity();
    let recorded_path = session.header().source.path.clone();
    if current.path != recorded_path {
        let message = format!(
            "source path changed from {recorded_path} to {}; pass --source when recovering",
            current.path
        );
        push_warning(&mut session.header_mut().warnings, &message);
    }

    let desc = src.description();
    for w in &desc.warnings {
        push_warning(&mut session.header_mut().warnings, w);
    }
    if !desc.read_only {
        let _ = session.fail("source description reported a writable handle; refusing to resume");
        return Err(Error::invalid(
            "source is not read-only; refusing to resume",
        ));
    }
    session.header_mut().status = SessionStatus::Scanning;
    session.header_mut().finished = None;
    session.header_mut().error = None;
    push_warning(
        &mut session.header_mut().warnings,
        &format!("scan resumed at {}", now_iso()),
    );
    session.save_header()?;

    // Options are taken from the session: the checkpoint describes work that
    // was planned under those options, so silently changing them would make the
    // completed-volume boundaries meaningless.
    let effective = ScanRequest {
        session_dir: request.session_dir.clone(),
        include_live: stored.include_live,
        carve: stored.carve,
        volume_indices: stored.volume_indices.clone(),
        manual_volume: stored.manual_volume,
        max_candidates: stored.max_candidates.max(1),
        cancelled: request.cancelled.clone(),
        resume: true,
    };
    finish_scan(src, &effective, &mut session, progress)
}

fn finish_scan(
    src: &dyn ReadOnlySource,
    request: &ScanRequest,
    session: &mut Session,
    progress: &mut dyn FnMut(ScanProgress),
) -> Result<SessionHeader> {
    let outcome = scan_inner(src, request, session, progress);
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

fn ensure_resumable(header: &SessionHeader, src: &dyn ReadOnlySource) -> Result<()> {
    match header.status {
        SessionStatus::Scanning | SessionStatus::Cancelled | SessionStatus::Failed => {}
        SessionStatus::Complete => {
            return Err(Error::Session(
                "session is already complete; nothing to resume".into(),
            ))
        }
        SessionStatus::Recovering | SessionStatus::Recovered | SessionStatus::RecoveryPartial => {
            return Err(Error::Session(
                "session has entered recovery; scan resume is no longer possible".into(),
            ))
        }
    }
    if header.checkpoint.is_none() {
        return Err(Error::Session(
            "session has no scan checkpoint; start a new scan".into(),
        ));
    }

    let current = src.identity();
    let recorded = &header.source;
    if current.kind != recorded.kind {
        return Err(Error::SourceMismatch(format!(
            "source kind changed from {:?} to {:?}",
            recorded.kind, current.kind
        )));
    }
    if current.size != recorded.size {
        return Err(Error::SourceMismatch(format!(
            "source size changed from {} to {} bytes",
            recorded.size, current.size
        )));
    }
    if current.fingerprint != recorded.fingerprint {
        return Err(Error::SourceMismatch(
            "source fingerprint does not match the session".into(),
        ));
    }
    if current.kind == SourceKind::RawDevice && current.device_id != recorded.device_id {
        return Err(Error::SourceMismatch(
            "raw device identifier does not match the session".into(),
        ));
    }
    Ok(())
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
    let completed = validate_checkpoint(session.header(), &volumes)?;
    session.header_mut().partition_table = Some(table);
    let mut volume_reports: Vec<FilesystemReport> = session.header().filesystems.clone();

    for volume in &volumes {
        if request.is_cancelled() {
            break;
        }
        if completed.contains(&volume.index) {
            continue;
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
        if request.is_cancelled() {
            // The volume is incomplete: do not publish its report and do not
            // advance the checkpoint. Its candidates remain in the file for a
            // cancelled session, but resume truncates them and scans it again.
            break;
        }
        volume_reports.push(report);
        session.header_mut().filesystems = volume_reports.clone();
        session.record_volume_complete(volume.index)?;
    }

    progress(ScanProgress {
        phase: "done",
        done: total_source,
        total: total_source,
    });
    if request.is_cancelled() {
        push_warning(
            &mut session.header_mut().warnings,
            "scan cancelled by user; results are partial and the interrupted volume will be rescanned on resume",
        );
    }
    Ok(())
}

/// Validate the persisted checkpoint against the freshly resolved volumes.
/// A checkpoint that no longer describes this source is refused rather than
/// silently skipped.
fn validate_checkpoint(header: &SessionHeader, volumes: &[Volume]) -> Result<BTreeSet<u32>> {
    let checkpoint = header.checkpoint.clone().unwrap_or_default();
    let completed: BTreeSet<u32> = checkpoint.completed_volumes.iter().copied().collect();
    for index in &completed {
        let volume = volumes.iter().find(|v| v.index == *index).ok_or_else(|| {
            Error::SourceMismatch(format!(
                "checkpoint records completed volume {index}, but the source has no such volume"
            ))
        })?;
        let report = header
            .filesystems
            .iter()
            .find(|r| r.volume_index == *index)
            .ok_or_else(|| {
                Error::Session(format!(
                    "checkpoint marks volume {index} complete but its report is missing"
                ))
            })?;
        if report.offset != volume.offset || report.length != volume.length {
            return Err(Error::SourceMismatch(format!(
                "volume {index} moved from {:#x}+{:#x} to {:#x}+{:#x}",
                report.offset, report.length, volume.offset, volume.length
            )));
        }
    }
    for report in &header.filesystems {
        if !completed.contains(&report.volume_index) {
            return Err(Error::Session(format!(
                "volume {} has a report but no completed checkpoint",
                report.volume_index
            )));
        }
    }
    Ok(completed)
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
    use std::path::Path;

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

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relift-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_image(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn signature(candidate: &crate::candidate::Candidate) -> String {
        let mut value = serde_json::to_value(candidate).unwrap();
        if let Some(object) = value.as_object_mut() {
            object.remove("id");
        }
        value.to_string()
    }

    fn assert_same_candidates(
        fresh: &[crate::candidate::Candidate],
        resumed: &[crate::candidate::Candidate],
    ) {
        assert_eq!(resumed.len(), fresh.len());
        let mut a: Vec<String> = fresh.iter().map(signature).collect();
        let mut b: Vec<String> = resumed.iter().map(signature).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        let ids: Vec<u64> = resumed.iter().map(|c| c.id).collect();
        assert_eq!(
            ids,
            (1..=resumed.len() as u64).collect::<Vec<_>>(),
            "candidate IDs must be dense after truncation and re-emission"
        );
    }

    #[test]
    fn resume_after_cancelled_carve_matches_uninterrupted_scan() {
        let dir = temp_dir("resume-carve");
        let fixture = fat32_image();
        let image = write_image(&dir, "fat32.img", fixture.image());
        let src = FileSource::open(&image).unwrap();

        let fresh_dir = dir.join("fresh");
        let mut fresh_req = ScanRequest::new(&fresh_dir);
        fresh_req.carve = true;
        let mut noop = |_p: ScanProgress| {};
        let fresh = run_scan(&src, &fresh_req, &mut noop).unwrap();
        assert_eq!(fresh.status, SessionStatus::Complete);
        let fresh_candidates = crate::session::load_candidates(&fresh_dir).unwrap();

        let interrupted_dir = dir.join("interrupted");
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut req = ScanRequest::new(&interrupted_dir);
        req.carve = true;
        req.cancelled = cancelled.clone();
        let mut saw_carve = false;
        let mut progress = |p: ScanProgress| {
            if p.phase == "carve" {
                saw_carve = true;
                if p.done >= 8 * 1024 * 1024 {
                    cancelled.store(true, Ordering::Relaxed);
                }
            }
        };
        let interrupted = run_scan(&src, &req, &mut progress).unwrap();
        assert!(saw_carve, "the test must interrupt during carving");
        assert_eq!(interrupted.status, SessionStatus::Cancelled);
        let checkpoint = interrupted.checkpoint.as_ref().unwrap();
        assert!(
            checkpoint.completed_volumes.is_empty(),
            "a partially carved volume must not be committed: {checkpoint:?}"
        );
        let partial = crate::session::load_candidates(&interrupted_dir).unwrap();
        assert!(
            !partial.is_empty(),
            "metadata candidates were streamed before the interruption"
        );

        let mut resume_req = ScanRequest::new(&interrupted_dir);
        resume_req.resume = true;
        let mut phases = Vec::new();
        let mut progress = |p: ScanProgress| phases.push(p.phase);
        let resumed = run_scan(&src, &resume_req, &mut progress).unwrap();
        assert_eq!(resumed.status, SessionStatus::Complete);
        assert!(
            phases.contains(&"carve"),
            "the uncommitted volume must be rescanned including carving: {phases:?}"
        );

        let resumed_candidates = crate::session::load_candidates(&interrupted_dir).unwrap();
        assert_same_candidates(&fresh_candidates, &resumed_candidates);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two independent exFAT volumes in an MBR. Returns the image and the byte
    /// offset of the second volume.
    fn two_volume_exfat_image() -> (Vec<u8>, u64) {
        let first = exfat_image();
        let second = exfat_image();
        let sectors = (first.image().len() / 512) as u32;
        let start_lbas = [2048u32, 8192u32];
        let total = (start_lbas[1] as u64 + sectors as u64) * 512;
        let mut disk = vec![0u8; total as usize];
        disk[510] = 0x55;
        disk[511] = 0xAA;
        for (i, start) in start_lbas.iter().enumerate() {
            let entry = 446 + i * 16;
            disk[entry + 4] = 0x07;
            disk[entry + 8..entry + 12].copy_from_slice(&start.to_le_bytes());
            disk[entry + 12..entry + 16].copy_from_slice(&sectors.to_le_bytes());
        }
        for (start, fixture) in start_lbas.iter().zip([&first, &second]) {
            let offset = *start as usize * 512;
            disk[offset..offset + fixture.image().len()].copy_from_slice(fixture.image());
        }
        (disk, start_lbas[1] as u64 * 512)
    }

    #[test]
    fn resume_skips_volumes_already_committed() {
        let dir = temp_dir("resume-volumes");
        let (disk, second_start) = two_volume_exfat_image();
        let image = write_image(&dir, "disk.img", &disk);
        let src = FileSource::open(&image).unwrap();

        let fresh_dir = dir.join("fresh");
        let mut noop = |_p: ScanProgress| {};
        let fresh = run_scan(&src, &ScanRequest::new(&fresh_dir), &mut noop).unwrap();
        assert_eq!(fresh.status, SessionStatus::Complete);
        assert_eq!(fresh.filesystems.len(), 2);
        let fresh_candidates = crate::session::load_candidates(&fresh_dir).unwrap();
        assert!(fresh_candidates.len() > 1);

        let interrupted_dir = dir.join("interrupted");
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut req = ScanRequest::new(&interrupted_dir);
        req.cancelled = cancelled.clone();
        let mut progress = |p: ScanProgress| {
            if p.phase == "metadata" && p.done == second_start {
                cancelled.store(true, Ordering::Relaxed);
            }
        };
        let interrupted = run_scan(&src, &req, &mut progress).unwrap();
        assert_eq!(interrupted.status, SessionStatus::Cancelled);
        assert_eq!(
            interrupted.checkpoint.as_ref().unwrap().completed_volumes,
            vec![0]
        );
        assert_eq!(interrupted.filesystems.len(), 1);
        assert_eq!(interrupted.filesystems[0].volume_index, 0);

        let mut resume_req = ScanRequest::new(&interrupted_dir);
        resume_req.resume = true;
        let mut noop = |_p: ScanProgress| {};
        let resumed = run_scan(&src, &resume_req, &mut noop).unwrap();
        assert_eq!(resumed.status, SessionStatus::Complete);
        assert_eq!(
            resumed.filesystems.len(),
            2,
            "resume must not append a duplicate report for volume 0"
        );

        let resumed_candidates = crate::session::load_candidates(&interrupted_dir).unwrap();
        // Duplicating volume 0 would show up as extra candidates.
        assert_same_candidates(&fresh_candidates, &resumed_candidates);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resume_refuses_a_different_source() {
        let dir = temp_dir("resume-mismatch");
        let fixture = exfat_image();
        let image = write_image(&dir, "original.img", fixture.image());
        let src = FileSource::open(&image).unwrap();

        let session_dir = dir.join("session");
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut req = ScanRequest::new(&session_dir);
        req.cancelled = cancelled.clone();
        let mut progress = |p: ScanProgress| {
            if p.phase == "metadata" {
                cancelled.store(true, Ordering::Relaxed);
            }
        };
        let interrupted = run_scan(&src, &req, &mut progress).unwrap();
        assert_eq!(interrupted.status, SessionStatus::Cancelled);

        let mut other = fixture.image().to_vec();
        other[0x100] ^= 0xFF;
        let other_path = write_image(&dir, "other.img", &other);
        let other_src = FileSource::open(&other_path).unwrap();

        let mut resume_req = ScanRequest::new(&session_dir);
        resume_req.resume = true;
        let mut noop = |_p: ScanProgress| {};
        let error = run_scan(&other_src, &resume_req, &mut noop).unwrap_err();
        assert!(
            matches!(error, Error::SourceMismatch(_)),
            "expected a source mismatch, got {error}"
        );

        let header = load_header(&session_dir).unwrap();
        assert_eq!(header.status, SessionStatus::Cancelled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resume_refuses_a_completed_session() {
        let dir = temp_dir("resume-complete");
        let fixture = exfat_image();
        let image = write_image(&dir, "exfat.img", fixture.image());
        let src = FileSource::open(&image).unwrap();
        let session_dir = dir.join("session");
        let mut noop = |_p: ScanProgress| {};
        let header = run_scan(&src, &ScanRequest::new(&session_dir), &mut noop).unwrap();
        assert_eq!(header.status, SessionStatus::Complete);

        let mut req = ScanRequest::new(&session_dir);
        req.resume = true;
        let error = run_scan(&src, &req, &mut noop).unwrap_err();
        assert!(error.to_string().contains("already complete"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resume_refuses_a_session_without_a_checkpoint() {
        let dir = temp_dir("resume-no-checkpoint");
        let fixture = exfat_image();
        let image = write_image(&dir, "exfat.img", fixture.image());
        let src = FileSource::open(&image).unwrap();
        let session_dir = dir.join("session");
        let mut noop = |_p: ScanProgress| {};
        run_scan(&src, &ScanRequest::new(&session_dir), &mut noop).unwrap();

        let mut header = load_header(&session_dir).unwrap();
        header.status = SessionStatus::Cancelled;
        header.finished = None;
        header.checkpoint = None;
        crate::session::save_header_to(&session_dir, &header).unwrap();

        let mut req = ScanRequest::new(&session_dir);
        req.resume = true;
        let error = run_scan(&src, &req, &mut noop).unwrap_err();
        assert!(error.to_string().contains("no scan checkpoint"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
