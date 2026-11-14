//! Recovery writer.
//!
//! Reads only from the `ReadOnlySource`, writes only below a destination that
//! was proven separate from the evidence, uses `create_new` for every output,
//! sanitizes every path component, and records a SHA-256 for each file it
//! writes. A file with an I/O error is renamed to `.partial` so a complete
//! filename always means the byte copy finished (though the source metadata may
//! still say the file was incomplete).

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

use crate::candidate::{Candidate, ConfidenceLabel, DataLocation};
use crate::error::{Error, Result};
use crate::hash::{hash_file, Hasher};
use crate::safety::{sanitize_filename, sanitize_relative, validate_destination};
use crate::session::{
    append_json_line, load_header, now_iso, save_header_to, CandidateReader, RecoveryTotals,
    SessionStatus, RECOVERED_FILE,
};
use crate::source::ReadOnlySource;

const COPY_CHUNK: usize = 256 * 1024;
const MAX_DISPLAYED_RESULTS: usize = 200;

#[derive(Debug, Clone)]
pub struct RecoverOptions {
    pub dest: PathBuf,
    pub verify: bool,
    pub min_confidence: Option<ConfidenceLabel>,
    pub ids: Option<HashSet<u64>>,
    pub cancelled: Arc<AtomicBool>,
}

impl RecoverOptions {
    pub fn new(dest: impl Into<PathBuf>) -> Self {
        Self {
            dest: dest.into(),
            verify: true,
            min_confidence: None,
            ids: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStatus {
    Recovered,
    Partial,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoveredFile {
    pub id: u64,
    pub name: String,
    pub original_path: String,
    pub output_path: Option<String>,
    pub expected_size: u64,
    pub bytes_written: u64,
    pub sha256: Option<String>,
    pub verified: bool,
    pub status: RecoveryStatus,
    pub confidence: String,
    pub method: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoverySummary {
    pub considered: u64,
    pub attempted: u64,
    pub succeeded: u64,
    pub partial: u64,
    pub failed: u64,
    pub skipped: u64,
    pub bytes_written: u64,
    pub destination: String,
    pub results: Vec<RecoveredFile>,
    pub results_truncated: bool,
    pub warnings: Vec<String>,
    pub started: String,
    pub finished: String,
}

#[derive(Debug, Clone)]
pub struct RecoveryProgress {
    pub index: u64,
    pub total: u64,
    pub bytes_written: u64,
    pub current: String,
}

/// Recover a session into `options.dest`. The session's recorded source
/// identity must still match the opened source; a changed image is refused.
pub fn run_recovery(
    session_dir: &Path,
    src: &dyn ReadOnlySource,
    options: &RecoverOptions,
    progress: &mut dyn FnMut(RecoveryProgress),
) -> Result<RecoverySummary> {
    let mut header = load_header(session_dir)?;
    let identity = src.identity();
    if identity.size != header.source.size || identity.fingerprint != header.source.fingerprint {
        return Err(Error::SourceMismatch(format!(
            "source does not match the session: expected size {} fingerprint {}, got size {} \
             fingerprint {}. Re-scan the current source instead of recovering stale results.",
            header.source.size, header.source.fingerprint, identity.size, identity.fingerprint
        )));
    }
    let dest = validate_destination(&header.source, &options.dest)?;
    let started = now_iso();
    header.status = SessionStatus::Recovering;
    header.destination = Some(dest.to_string_lossy().to_string());
    header.recovery = Some(RecoveryTotals {
        attempted: 0,
        succeeded: 0,
        partial: 0,
        failed: 0,
        skipped: 0,
        bytes_written: 0,
        started: started.clone(),
        finished: None,
    });
    save_header_to(session_dir, &header)?;

    let total = header.summary.candidates;
    let mut reader = CandidateReader::open(session_dir)?;
    let mut summary = RecoverySummary {
        considered: 0,
        attempted: 0,
        succeeded: 0,
        partial: 0,
        failed: 0,
        skipped: 0,
        bytes_written: 0,
        destination: dest.to_string_lossy().to_string(),
        results: Vec::new(),
        results_truncated: false,
        warnings: Vec::new(),
        started,
        finished: String::new(),
    };
    let mut index = 0u64;

    while let Some(candidate) = reader.next_candidate() {
        let candidate = candidate?;
        index += 1;
        summary.considered += 1;
        if options.is_cancelled() {
            summary.warnings.push(
                "recovery cancelled by user; processed output is valid but incomplete".into(),
            );
            break;
        }
        if let Some(ids) = &options.ids {
            if !ids.contains(&candidate.id) {
                summary.skipped += 1;
                continue;
            }
        }
        if let Some(min) = options.min_confidence {
            if confidence_rank(candidate.confidence.label) < confidence_rank(min) {
                summary.skipped += 1;
                continue;
            }
        }
        if candidate.size > 0 && !candidate.has_content() {
            summary.skipped += 1;
            let record = RecoveredFile {
                id: candidate.id,
                name: candidate.name.clone(),
                original_path: candidate.display_path(),
                output_path: None,
                expected_size: candidate.size,
                bytes_written: 0,
                sha256: None,
                verified: false,
                status: RecoveryStatus::Skipped,
                confidence: candidate.confidence.label.as_str().into(),
                method: candidate.method.as_str().into(),
                note: Some("no content location was recoverable from metadata".into()),
            };
            append_json_line(session_dir, RECOVERED_FILE, &record)?;
            push_result(&mut summary, record);
            continue;
        }
        summary.attempted += 1;
        let outcome = recover_one(src, &dest, &candidate, options.verify);
        match outcome {
            Ok(record) => {
                summary.bytes_written += record.bytes_written;
                match record.status {
                    RecoveryStatus::Recovered => summary.succeeded += 1,
                    RecoveryStatus::Partial => summary.partial += 1,
                    RecoveryStatus::Failed => summary.failed += 1,
                    RecoveryStatus::Skipped => summary.skipped += 1,
                }
                append_json_line(session_dir, RECOVERED_FILE, &record)?;
                push_result(&mut summary, record);
            }
            Err(e) => {
                summary.failed += 1;
                let record = RecoveredFile {
                    id: candidate.id,
                    name: candidate.name.clone(),
                    original_path: candidate.display_path(),
                    output_path: None,
                    expected_size: candidate.size,
                    bytes_written: 0,
                    sha256: None,
                    verified: false,
                    status: RecoveryStatus::Failed,
                    confidence: candidate.confidence.label.as_str().into(),
                    method: candidate.method.as_str().into(),
                    note: Some(e.to_string()),
                };
                append_json_line(session_dir, RECOVERED_FILE, &record)?;
                push_result(&mut summary, record.clone());
                summary
                    .warnings
                    .push(format!("candidate {} failed: {e}", candidate.id));
            }
        }
        progress(RecoveryProgress {
            index,
            total,
            bytes_written: summary.bytes_written,
            current: candidate.name.clone(),
        });
    }

    summary.finished = now_iso();
    if options.is_cancelled() {
        summary.warnings.push("recovery was cancelled".into());
    }
    header.recovery = Some(RecoveryTotals {
        attempted: summary.attempted,
        succeeded: summary.succeeded,
        partial: summary.partial,
        failed: summary.failed,
        skipped: summary.skipped,
        bytes_written: summary.bytes_written,
        started: summary.started.clone(),
        finished: Some(summary.finished.clone()),
    });
    header.status = if summary.failed > 0 || summary.partial > 0 || options.is_cancelled() {
        SessionStatus::RecoveryPartial
    } else {
        SessionStatus::Recovered
    };
    header.finished = Some(summary.finished.clone());
    for w in &summary.warnings {
        if !header.warnings.contains(w) {
            header.warnings.push(w.clone());
        }
    }
    save_header_to(session_dir, &header)?;
    Ok(summary)
}

fn push_result(summary: &mut RecoverySummary, record: RecoveredFile) {
    if summary.results.len() < MAX_DISPLAYED_RESULTS {
        summary.results.push(record);
    } else {
        summary.results_truncated = true;
    }
}

fn confidence_rank(label: ConfidenceLabel) -> u8 {
    match label {
        ConfidenceLabel::Unverified => 0,
        ConfidenceLabel::Partial => 1,
        ConfidenceLabel::Likely => 2,
        ConfidenceLabel::Verified => 3,
    }
}

fn recover_one(
    src: &dyn ReadOnlySource,
    root: &Path,
    candidate: &Candidate,
    verify: bool,
) -> Result<RecoveredFile> {
    let rel = output_relative_path(candidate);
    let (path, mut file) = create_output(root, &rel)?;
    let mut hasher = Hasher::new();
    let mut written = 0u64;
    let mut complete = candidate.complete;
    let write_result = write_data(
        src,
        &mut file,
        &candidate.data,
        candidate.size,
        &mut written,
        &mut hasher,
    );

    let mut note = None;
    let mut status = RecoveryStatus::Recovered;
    let mut io_failed = false;
    match write_result {
        Ok(()) => {
            if written < candidate.size {
                complete = false;
                note = Some(format!(
                    "only {written} of {} recorded bytes were locatable",
                    candidate.size
                ));
            }
            if !candidate.complete {
                complete = false;
                if note.is_none() {
                    note = Some("filesystem metadata already indicated an incomplete file".into());
                }
            }
        }
        Err(e) => {
            io_failed = true;
            complete = false;
            note = Some(e.to_string());
        }
    }
    let hash = hasher.finish_hex();
    let flush = file.flush().and_then(|_| file.sync_all());
    drop(file);
    if let Err(e) = flush {
        io_failed = true;
        complete = false;
        note = Some(format!("flush failed: {e}"));
    }

    let mut verified = false;
    let mut output_path = Some(path.to_string_lossy().to_string());
    if !io_failed {
        if verify {
            match hash_file(&path) {
                Ok(actual) if actual == hash => verified = true,
                Ok(actual) => {
                    io_failed = true;
                    note = Some(format!(
                        "written file hash {actual} does not match copy hash {hash}"
                    ));
                }
                Err(e) => {
                    io_failed = true;
                    note = Some(format!("could not re-read written file: {e}"));
                }
            }
        }
    }
    if io_failed {
        status = RecoveryStatus::Failed;
        complete = false;
        match quarantine(&path) {
            Some(partial) => {
                output_path = Some(partial.to_string_lossy().to_string());
                note = Some(format!(
                    "{}; partial data left at {}",
                    note.unwrap_or_else(|| "write failed".into()),
                    partial.display()
                ));
            }
            None => {
                output_path = None;
                note = Some(format!(
                    "{}; incomplete output was removed",
                    note.unwrap_or_else(|| "write failed".into())
                ));
            }
        }
    } else if !complete {
        status = RecoveryStatus::Partial;
    }

    Ok(RecoveredFile {
        id: candidate.id,
        name: candidate.name.clone(),
        original_path: candidate.display_path(),
        output_path,
        expected_size: candidate.size,
        bytes_written: written,
        sha256: Some(hash),
        verified,
        status,
        confidence: candidate.confidence.label.as_str().into(),
        method: candidate.method.as_str().into(),
        note,
    })
}

fn write_data(
    src: &dyn ReadOnlySource,
    out: &mut File,
    data: &DataLocation,
    size: u64,
    written: &mut u64,
    hasher: &mut Hasher,
) -> Result<()> {
    match data {
        DataLocation::Extents { extents } => {
            for extent in extents {
                if *written >= size {
                    break;
                }
                copy_extent(
                    src,
                    out,
                    extent.offset,
                    extent.length,
                    size,
                    written,
                    hasher,
                )?;
            }
        }
        DataLocation::Segments { segments } => {
            for segment in segments {
                if *written >= size {
                    break;
                }
                match segment {
                    crate::candidate::Segment::Data { extent } => {
                        copy_extent(
                            src,
                            out,
                            extent.offset,
                            extent.length,
                            size,
                            written,
                            hasher,
                        )?;
                    }
                    crate::candidate::Segment::Zero { length } => {
                        write_zeros(out, *length, size, written, hasher)?;
                    }
                }
            }
        }
        DataLocation::Resident { bytes } => {
            let take = (bytes.len() as u64).min(size) as usize;
            out.write_all(&bytes[..take]).map_err(|e| Error::Io {
                offset: 0,
                source: e,
            })?;
            hasher.update(&bytes[..take]);
            *written += take as u64;
        }
        DataLocation::Raw { offset, length } => {
            copy_extent(src, out, *offset, *length, size, written, hasher)?;
        }
        DataLocation::None => {}
    }
    Ok(())
}

fn copy_extent(
    src: &dyn ReadOnlySource,
    out: &mut File,
    offset: u64,
    length: u64,
    size: u64,
    written: &mut u64,
    hasher: &mut Hasher,
) -> Result<()> {
    let source_len = src.len();
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::Corrupt("extent offset overflow".into()))?;
    if end > source_len {
        return Err(Error::Corrupt(format!(
            "extent {offset:#x}..{end:#x} lies beyond the source length {source_len:#x}"
        )));
    }
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut at = offset;
    let mut left = length;
    while left > 0 && *written < size {
        let want = left.min(buf.len() as u64).min(size - *written) as usize;
        let n = src.read_at(at, &mut buf[..want])?;
        if n == 0 {
            return Err(Error::UnexpectedEof {
                offset: at,
                wanted: want,
                got: 0,
            });
        }
        out.write_all(&buf[..n]).map_err(|e| Error::Io {
            offset: 0,
            source: e,
        })?;
        hasher.update(&buf[..n]);
        *written += n as u64;
        at += n as u64;
        left -= n as u64;
    }
    Ok(())
}

fn write_zeros(
    out: &mut File,
    length: u64,
    size: u64,
    written: &mut u64,
    hasher: &mut Hasher,
) -> Result<()> {
    let mut left = length.min(size.saturating_sub(*written));
    let zeros = vec![0u8; COPY_CHUNK];
    while left > 0 {
        let n = left.min(zeros.len() as u64) as usize;
        out.write_all(&zeros[..n]).map_err(|e| Error::Io {
            offset: 0,
            source: e,
        })?;
        hasher.update(&zeros[..n]);
        *written += n as u64;
        left -= n as u64;
    }
    Ok(())
}

fn output_relative_path(candidate: &Candidate) -> PathBuf {
    let mut rel = sanitize_relative(&candidate.path).unwrap_or_default();
    let fallback = format!("recovered_{:06}", candidate.id);
    let mut name = sanitize_filename(&candidate.name, &fallback);
    if name == "unnamed" {
        name = fallback;
    }
    if Path::new(&name).extension().is_none()
        && !candidate.format.is_empty()
        && candidate.format != "unknown"
    {
        if let Some(ext) = sanitize_filename(&candidate.format, "bin")
            .split('.')
            .next()
        {
            name.push('.');
            name.push_str(ext);
        }
    }
    rel.push(name);
    rel
}

fn create_output(root: &Path, rel: &Path) -> Result<(PathBuf, File)> {
    let rel = if root.as_os_str().len() + rel.as_os_str().len() > 230 {
        // Windows legacy path limits: flatten rather than fail.
        rel.file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("recovered.bin"))
    } else {
        rel.to_path_buf()
    };
    if let Some(parent) = rel.parent() {
        create_confined_dirs(root, parent)?;
    }
    for n in 0..1000u32 {
        let candidate = if n == 0 {
            rel.clone()
        } else {
            numbered(&rel, n)
        };
        let path = root.join(&candidate);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(Error::Io {
                    offset: 0,
                    source: e,
                })
            }
        }
    }
    Err(Error::Other(format!(
        "could not find a free output name for {}",
        rel.display()
    )))
}

fn create_confined_dirs(root: &Path, rel_parent: &Path) -> Result<()> {
    let mut cur = root.to_path_buf();
    for comp in rel_parent.components() {
        let next = cur.join(comp.as_os_str());
        match fs::symlink_metadata(&next) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(Error::UnsafeDestination(format!(
                        "refusing to write through symlink {}",
                        next.display()
                    )));
                }
                if !meta.is_dir() {
                    return Err(Error::UnsafeDestination(format!(
                        "{} exists and is not a directory",
                        next.display()
                    )));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&next).map_err(|e| Error::Io {
                    offset: 0,
                    source: e,
                })?;
            }
            Err(e) => {
                return Err(Error::Io {
                    offset: 0,
                    source: e,
                })
            }
        }
        cur = next;
    }
    let canonical = fs::canonicalize(&cur).map_err(|e| Error::Io {
        offset: 0,
        source: e,
    })?;
    if !canonical.starts_with(root) {
        return Err(Error::UnsafeDestination(format!(
            "{} escapes the destination root {}",
            canonical.display(),
            root.display()
        )));
    }
    Ok(())
}

fn numbered(rel: &Path, n: u32) -> PathBuf {
    let name = rel
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let (stem, ext) = match name.split_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), Some(e.to_string())),
        _ => (name, None),
    };
    let new_name = match ext {
        Some(e) => format!("{stem} ({n}).{e}"),
        None => format!("{stem} ({n})"),
    };
    rel.with_file_name(new_name)
}

fn quarantine(path: &Path) -> Option<PathBuf> {
    let mut s = path.as_os_str().to_os_string();
    s.push(".partial");
    let partial = PathBuf::from(s);
    match fs::rename(path, &partial) {
        Ok(()) => Some(partial),
        Err(_) => {
            // Never leave a partial file under a name that looks complete.
            let _ = fs::remove_file(path);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{Confidence, RecoveryMethod};
    use crate::fs::CandidateSink;
    use crate::session::{load_candidates, Session, SessionOptions};
    use crate::source::{FileSource, SourceIdentity, SourceKind};

    fn setup() -> (PathBuf, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("relift-recover-{}-{serial}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let image = base.join("src.img");
        fs::write(&image, b"hello world").unwrap();
        let session = base.join("session");
        let identity = SourceIdentity {
            kind: SourceKind::Image,
            path: image.to_string_lossy().to_string(),
            device_id: None,
            size: 11,
            fingerprint: "b".repeat(64),
        };
        let mut s = Session::create(
            &session,
            identity,
            SessionOptions {
                include_live: false,
                carve: false,
                max_candidates: 10,
                volume_indices: vec![],
                manual_volume: None,
            },
        )
        .unwrap();
        let mut c = Candidate::new(
            "greeting.txt",
            11,
            "txt",
            DataLocation::resident(b"hello world".to_vec()),
            RecoveryMethod::FatDirectory,
            Confidence::verified(98, "test"),
            "fat32",
        );
        c.complete = true;
        CandidateSink::emit(&mut s, c).unwrap();
        s.finish(SessionStatus::Complete).unwrap();
        (base, session)
    }

    #[test]
    fn recovers_and_verifies_resident_file() {
        let (base, session) = setup();
        let image = base.join("src.img");
        let src = FileSource::open(&image).unwrap();
        // The test session's identity is synthetic, so recovery must verify
        // against it. Replace the header identity with the real one.
        let mut header = load_header(&session).unwrap();
        header.source = src.identity();
        save_header_to(&session, &header).unwrap();
        let dest = base.join("out");
        fs::create_dir_all(&dest).unwrap();
        let options = RecoverOptions::new(&dest);
        let mut progress = |_p: RecoveryProgress| {};
        let summary = run_recovery(&session, &src, &options, &mut progress).unwrap();
        assert_eq!(summary.succeeded, 1);
        assert_eq!(fs::read(dest.join("greeting.txt")).unwrap(), b"hello world");
        assert_eq!(load_candidates(&session).unwrap().len(), 1);
        let log = fs::read_to_string(session.join(RECOVERED_FILE)).unwrap();
        assert!(log.contains("greeting.txt"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn refuses_mismatched_source() {
        let (base, session) = setup();
        let image = base.join("src.img");
        fs::write(&image, b"different!").unwrap();
        let src = FileSource::open(&image).unwrap();
        let dest = base.join("out");
        fs::create_dir_all(&dest).unwrap();
        let options = RecoverOptions::new(&dest);
        let mut progress = |_p: RecoveryProgress| {};
        let err = run_recovery(&session, &src, &options, &mut progress).unwrap_err();
        assert!(err.to_string().contains("does not match"));
        let _ = fs::remove_dir_all(&base);
    }
}
