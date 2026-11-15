//! On-disk scan sessions.
//!
//! A session is a directory containing:
//!
//! - `session.json` — header, source identity, options, filesystem reports and
//!   counters. Written atomically.
//! - `candidates.jsonl` — one JSON `Candidate` per line, appended as the scan
//!   progresses. This is the only potentially large file.
//! - `recovered.jsonl` — written by the recovery pass.
//! - `README.txt` — human explanation of the format.
//!
//! Candidates stream to disk; a scan never buffers all results in memory.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::candidate::{Candidate, ConfidenceLabel, EntryState, RecoveryMethod};
use crate::carve::CarveSummary;
use crate::error::{Error, Result};
use crate::fs::{FsInfo, FsSummary};
use crate::partitions::PartitionTable;
use crate::source::SourceIdentity;
use crate::util::unix_to_iso;
use crate::{SCHEMA_VERSION, VERSION};

pub const SESSION_FILE: &str = "session.json";
pub const CANDIDATES_FILE: &str = "candidates.jsonl";
pub const RECOVERED_FILE: &str = "recovered.jsonl";
pub const README_FILE: &str = "README.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Scanning,
    Complete,
    Cancelled,
    Failed,
    Recovering,
    Recovered,
    RecoveryPartial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionOptions {
    pub include_live: bool,
    pub carve: bool,
    pub max_candidates: usize,
    /// Empty means every probed volume.
    #[serde(default)]
    pub volume_indices: Vec<u32>,
    /// Manual volume override (`--offset` / `--length`).
    #[serde(default)]
    pub manual_volume: Option<ManualVolume>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ManualVolume {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionSummary {
    pub candidates: u64,
    pub deleted: u64,
    pub live: u64,
    pub with_content: u64,
    pub metadata_only: u64,
    pub by_method: BTreeMap<String, u64>,
    pub by_category: BTreeMap<String, u64>,
    pub by_confidence: BTreeMap<String, u64>,
    pub by_format: BTreeMap<String, u64>,
}

impl SessionSummary {
    fn observe(&mut self, c: &Candidate) {
        self.candidates += 1;
        match c.state {
            EntryState::Deleted => self.deleted += 1,
            EntryState::Live => self.live += 1,
            EntryState::Carved => {}
        }
        if c.has_content() {
            self.with_content += 1;
        } else {
            self.metadata_only += 1;
        }
        *self
            .by_method
            .entry(c.method.as_str().to_string())
            .or_default() += 1;
        *self
            .by_category
            .entry(c.category.as_str().to_string())
            .or_default() += 1;
        *self
            .by_confidence
            .entry(c.confidence.label.as_str().to_string())
            .or_default() += 1;
        *self.by_format.entry(c.format.clone()).or_default() += 1;
    }

    pub fn add(&mut self, other: &SessionSummary) {
        self.candidates += other.candidates;
        self.deleted += other.deleted;
        self.live += other.live;
        self.with_content += other.with_content;
        self.metadata_only += other.metadata_only;
        for (k, v) in &other.by_method {
            *self.by_method.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &other.by_category {
            *self.by_category.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &other.by_confidence {
            *self.by_confidence.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &other.by_format {
            *self.by_format.entry(k.clone()).or_default() += v;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemReport {
    pub volume_index: u32,
    pub offset: u64,
    pub length: u64,
    pub label: Option<String>,
    pub kind: Option<String>,
    pub info: Option<FsInfo>,
    pub scan: Option<FsSummary>,
    pub carving: Option<CarveSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryTotals {
    pub attempted: u64,
    pub succeeded: u64,
    pub partial: u64,
    pub failed: u64,
    pub skipped: u64,
    pub bytes_written: u64,
    pub started: String,
    pub finished: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    pub schema: u32,
    pub tool: String,
    pub tool_version: String,
    pub session_id: String,
    pub created: String,
    pub finished: Option<String>,
    pub status: SessionStatus,
    pub source: SourceIdentity,
    pub options: SessionOptions,
    pub partition_table: Option<PartitionTable>,
    pub filesystems: Vec<FilesystemReport>,
    pub summary: SessionSummary,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub recovery: Option<RecoveryTotals>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl SessionHeader {
    pub fn fs_reports_for(&self, index: u32) -> Option<&FilesystemReport> {
        self.filesystems.iter().find(|f| f.volume_index == index)
    }
}

pub struct Session {
    dir: PathBuf,
    header: SessionHeader,
    candidates: BufWriter<File>,
    next_id: u64,
    since_flush: u64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("dir", &self.dir)
            .field("id", &self.header.session_id)
            .field("candidates_written", &self.header.summary.candidates)
            .finish()
    }
}

impl Session {
    /// Create a new session directory. `dir` may be missing or an existing
    /// empty directory; a non-empty directory is refused so evidence is never
    /// mixed with old output.
    pub fn create(
        dir: impl AsRef<Path>,
        source: SourceIdentity,
        options: SessionOptions,
    ) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        if dir.exists() {
            let meta = fs::metadata(&dir).map_err(|e| Error::Session(e.to_string()))?;
            if !meta.is_dir() {
                return Err(Error::Session(format!(
                    "{} exists and is not a directory",
                    dir.display()
                )));
            }
            let mut entries = fs::read_dir(&dir)
                .map_err(|e| Error::Session(format!("{}: {e}", dir.display())))?;
            if entries.next().is_some() {
                return Err(Error::Session(format!(
                    "{} is not empty; choose a new session directory",
                    dir.display()
                )));
            }
        } else {
            fs::create_dir_all(&dir)
                .map_err(|e| Error::Session(format!("create {}: {e}", dir.display())))?;
        }

        let created = now_iso();
        let session_id = make_session_id(&created, &source.path);
        let header = SessionHeader {
            schema: SCHEMA_VERSION,
            tool: "relift".to_string(),
            tool_version: VERSION.to_string(),
            session_id,
            created,
            finished: None,
            status: SessionStatus::Scanning,
            source,
            options,
            partition_table: None,
            filesystems: Vec::new(),
            summary: SessionSummary::default(),
            destination: None,
            recovery: None,
            warnings: Vec::new(),
            error: None,
        };
        let candidates_path = dir.join(CANDIDATES_FILE);
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&candidates_path)
            .map_err(|e| Error::Session(format!("create {}: {e}", candidates_path.display())))?;
        let session = Self {
            dir,
            header,
            candidates: BufWriter::new(file),
            next_id: 1,
            since_flush: 0,
        };
        session.write_readme()?;
        session.save_header()?;
        Ok(session)
    }

    fn write_readme(&self) -> Result<()> {
        let text = format!(
            "Relift recovery session\n\
             =======================\n\n\
             Created: {created}\n\
             Tool: relift {version}\n\
             Schema: {schema}\n\n\
             Files:\n\
             - session.json      Header, source identity, filesystem reports, counters.\n\
             - candidates.jsonl One candidate per line. This is the scan result.\n\
             - recovered.jsonl  Written when you run `relift recover`.\n\n\
             The source medium was opened read-only. Relift never writes to it.\n\
             Do not edit these files; recovery and reporting read them directly.\n",
            created = self.header.created,
            version = VERSION,
            schema = SCHEMA_VERSION,
        );
        fs::write(self.dir.join(README_FILE), text)
            .map_err(|e| Error::Session(format!("write README: {e}")))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn header(&self) -> &SessionHeader {
        &self.header
    }

    pub fn header_mut(&mut self) -> &mut SessionHeader {
        &mut self.header
    }

    pub fn add_filesystem(&mut self, report: FilesystemReport) {
        self.header.filesystems.push(report);
    }

    pub fn flush(&mut self) -> Result<()> {
        self.candidates
            .flush()
            .map_err(|e| Error::Session(format!("flush candidates: {e}")))
    }

    /// Save the header atomically (write a temp file, then replace).
    pub fn save_header(&self) -> Result<()> {
        save_header_to(&self.dir, &self.header)
    }

    pub fn finish(&mut self, status: SessionStatus) -> Result<()> {
        self.flush()?;
        self.candidates
            .get_ref()
            .sync_all()
            .map_err(|e| Error::Session(format!("sync candidates: {e}")))?;
        self.header.status = status;
        self.header.finished = Some(now_iso());
        self.save_header()
    }

    pub fn fail(&mut self, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        self.header.error = Some(message.clone());
        self.header.warnings.push(message);
        self.finish(SessionStatus::Failed)
    }

    pub fn append_recovery<T: Serialize>(&self, value: &T) -> Result<()> {
        append_json_line(&self.dir, RECOVERED_FILE, value)
    }
}

impl crate::fs::CandidateSink for Session {
    fn emit(&mut self, mut candidate: Candidate) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        candidate.id = id;
        self.header.summary.observe(&candidate);
        serde_json::to_writer(&mut self.candidates, &candidate)
            .map_err(|e| Error::Session(format!("serialize candidate {id}: {e}")))?;
        self.candidates
            .write_all(b"\n")
            .map_err(|e| Error::Session(format!("write candidate {id}: {e}")))?;
        self.since_flush += 1;
        if self.since_flush >= 128 {
            self.since_flush = 0;
            self.flush()?;
        }
        Ok(())
    }
}

/// Save a header into an existing session directory, replacing `session.json`
/// atomically. Used by the recovery pass, which is not a `Session` writer.
pub fn save_header_to(dir: &Path, header: &SessionHeader) -> Result<()> {
    let json = serde_json::to_vec_pretty(header)?;
    let path = dir.join(SESSION_FILE);
    let tmp = dir.join(format!("{SESSION_FILE}.tmp"));
    {
        let mut f = File::create(&tmp)
            .map_err(|e| Error::Session(format!("create {}: {e}", tmp.display())))?;
        f.write_all(&json)
            .map_err(|e| Error::Session(format!("write {}: {e}", tmp.display())))?;
        f.write_all(b"\n")
            .map_err(|e| Error::Session(format!("write {}: {e}", tmp.display())))?;
        f.sync_all()
            .map_err(|e| Error::Session(format!("sync {}: {e}", tmp.display())))?;
    }
    replace_file(&tmp, &path)
}

fn replace_file(tmp: &Path, path: &Path) -> Result<()> {
    match fs::rename(tmp, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            fs::remove_file(path)
                .map_err(|e| Error::Session(format!("replace {}: {e}", path.display())))?;
            fs::rename(tmp, path)
                .map_err(|e| Error::Session(format!("replace {}: {e}", path.display())))
        }
        Err(e) => Err(Error::Session(format!(
            "rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        ))),
    }
}

pub fn append_json_line<T: Serialize>(dir: &Path, name: &str, value: &T) -> Result<()> {
    let path = dir.join(name);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| Error::Session(format!("open {}: {e}", path.display())))?;
    serde_json::to_writer(&mut f, value)
        .map_err(|e| Error::Session(format!("serialize {}: {e}", path.display())))?;
    f.write_all(b"\n")
        .map_err(|e| Error::Session(format!("write {}: {e}", path.display())))?;
    Ok(())
}

pub fn load_header(dir: impl AsRef<Path>) -> Result<SessionHeader> {
    let path = dir.as_ref().join(SESSION_FILE);
    let bytes =
        fs::read(&path).map_err(|e| Error::Session(format!("read {}: {e}", path.display())))?;
    let header: SessionHeader = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Session(format!("parse {}: {e}", path.display())))?;
    if header.schema > SCHEMA_VERSION {
        return Err(Error::Session(format!(
            "session schema {} is newer than this build supports ({SCHEMA_VERSION})",
            header.schema
        )));
    }
    Ok(header)
}

/// Streaming candidate reader. The file is line-delimited JSON and is not
/// loaded into memory.
pub struct CandidateReader {
    reader: BufReader<File>,
    line: u64,
}

impl std::fmt::Debug for CandidateReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CandidateReader")
            .field("line", &self.line)
            .finish()
    }
}

impl CandidateReader {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let path = dir.as_ref().join(CANDIDATES_FILE);
        let f = File::open(&path)
            .map_err(|e| Error::Session(format!("open {}: {e}", path.display())))?;
        Ok(Self {
            reader: BufReader::with_capacity(1 << 20, f),
            line: 0,
        })
    }

    pub fn next_candidate(&mut self) -> Option<Result<Candidate>> {
        let mut line = String::new();
        loop {
            line.clear();
            let read = match self.reader.read_line(&mut line) {
                Ok(n) => n,
                Err(e) => return Some(Err(Error::Session(format!("read candidates: {e}")))),
            };
            if read == 0 {
                return None;
            }
            self.line += 1;
            if line.trim().is_empty() {
                continue;
            }
            return Some(
                serde_json::from_str(&line)
                    .map_err(|e| Error::Session(format!("candidate line {}: {e}", self.line))),
            );
        }
    }
}

/// Load every candidate. Intended for tests and small sessions; recovery and
/// reports use [`CandidateReader`] so memory stays bounded.
pub fn load_candidates(dir: impl AsRef<Path>) -> Result<Vec<Candidate>> {
    let mut reader = CandidateReader::open(dir)?;
    let mut out = Vec::new();
    while let Some(c) = reader.next_candidate() {
        out.push(c?);
    }
    Ok(out)
}

pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    unix_to_iso(secs).unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

fn make_session_id(created: &str, source_path: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(created.as_bytes());
    h.update(std::process::id().to_le_bytes());
    h.update(nanos.to_le_bytes());
    h.update(source_path.as_bytes());
    let digest = h.finalize();
    let suffix = crate::source::hex(&digest[..4]);
    let compact: String = created
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(14)
        .collect();
    format!("{compact}-{suffix}")
}

/// Human label used in reports and CLI output.
pub fn confidence_label_str(label: ConfidenceLabel) -> &'static str {
    label.as_str()
}

/// Method label used in reports.
pub fn method_label_str(method: RecoveryMethod) -> &'static str {
    method.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::{Confidence, DataLocation};

    fn source() -> SourceIdentity {
        SourceIdentity {
            kind: crate::source::SourceKind::Image,
            path: "test.img".into(),
            device_id: None,
            size: 4096,
            fingerprint: "a".repeat(64),
        }
    }

    #[test]
    fn session_roundtrips_candidates() {
        let dir = std::env::temp_dir().join(format!("relift-session-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        {
            let mut s = Session::create(
                &dir,
                source(),
                SessionOptions {
                    include_live: false,
                    carve: false,
                    max_candidates: 10,
                    volume_indices: vec![],
                    manual_volume: None,
                },
            )
            .unwrap();
            let c = Candidate::new(
                "a.txt",
                3,
                "txt",
                DataLocation::resident(b"abc".to_vec()),
                RecoveryMethod::MetadataOnly,
                Confidence::verified(90, "test"),
                "test",
            );
            crate::fs::CandidateSink::emit(&mut s, c).unwrap();
            s.finish(SessionStatus::Complete).unwrap();
        }
        let h = load_header(&dir).unwrap();
        assert_eq!(h.summary.candidates, 1);
        assert_eq!(h.status, SessionStatus::Complete);
        let cs = load_candidates(&dir).unwrap();
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].id, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_empty_session_dir_is_refused() {
        let dir = std::env::temp_dir().join(format!("relift-session2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("keep.txt"), b"x").unwrap();
        let e = Session::create(
            &dir,
            source(),
            SessionOptions {
                include_live: false,
                carve: false,
                max_candidates: 10,
                volume_indices: vec![],
                manual_volume: None,
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("not empty"));
        let _ = fs::remove_dir_all(&dir);
    }
}
