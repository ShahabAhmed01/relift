use serde::{Deserialize, Serialize};

use crate::source::hex;

/// How a candidate was found. This is part of the user-facing provenance and is
/// never reduced to a single "recovered" boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMethod {
    /// Deleted record in the NTFS Master File Table, resident data.
    NtfsMftResident,
    /// Deleted record in the NTFS Master File Table, non-resident data runs.
    NtfsMftRuns,
    /// Deleted FAT directory entry.
    FatDirectory,
    /// Deleted exFAT directory entry set.
    ExfatDirectory,
    /// Deleted ext2/3/4 inode.
    ExtInode,
    /// Name recovered from filesystem directory slack / stale entries.
    DirectorySlack,
    /// Signature carving with structural validation.
    Carved,
    /// Metadata identified a file but its content location is not available.
    MetadataOnly,
}

impl RecoveryMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NtfsMftResident => "ntfs-mft-resident",
            Self::NtfsMftRuns => "ntfs-mft-runs",
            Self::FatDirectory => "fat-directory",
            Self::ExfatDirectory => "exfat-directory",
            Self::ExtInode => "ext-inode",
            Self::DirectorySlack => "directory-slack",
            Self::Carved => "carved",
            Self::MetadataOnly => "metadata-only",
        }
    }

    pub fn preserves_name(self) -> bool {
        !matches!(self, Self::Carved)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLabel {
    /// Structure validates end to end and the evidence is internally
    /// consistent. This is not a cryptographic guarantee for metadata recovery.
    Verified,
    /// Strong evidence, but at least one assumption (for example, a contiguous
    /// guess where the allocation chain was cleared) is involved.
    Likely,
    /// Partially recoverable or partially overwritten.
    Partial,
    /// Detected only; content is not locatable or could not be validated.
    Unverified,
}

impl ConfidenceLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Likely => "likely",
            Self::Partial => "partial",
            Self::Unverified => "unverified",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confidence {
    pub label: ConfidenceLabel,
    /// 0..=100. This is a heuristic based on named evidence, not a calibrated
    /// probability and never a fabricated success percentage.
    pub score: u8,
    pub reasons: Vec<String>,
}

impl Confidence {
    pub fn new(label: ConfidenceLabel, score: u8, reasons: Vec<String>) -> Self {
        Self {
            label,
            score: score.min(100),
            reasons,
        }
    }

    pub fn verified(score: u8, reason: impl Into<String>) -> Self {
        Self::new(ConfidenceLabel::Verified, score, vec![reason.into()])
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reasons.push(reason.into());
        self
    }

    pub fn is_recoverable(&self) -> bool {
        self.label != ConfidenceLabel::Unverified
    }
}

/// A physically contiguous range in the opened source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent {
    pub offset: u64,
    pub length: u64,
}

impl Extent {
    pub fn new(offset: u64, length: u64) -> Self {
        Self { offset, length }
    }

    pub fn end(&self) -> Option<u64> {
        self.offset.checked_add(self.length)
    }
}

/// One logical piece of a file. Sparse files contain holes, and ext4 unwritten
/// extents read back as zeroes; both are represented explicitly so the output
/// file keeps its original byte offsets instead of being silently shortened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Segment {
    /// Bytes read from the source.
    Data { extent: Extent },
    /// Bytes that are logically zero and are not present in the source.
    Zero { length: u64 },
}

impl Segment {
    pub fn data(offset: u64, length: u64) -> Self {
        Self::Data {
            extent: Extent::new(offset, length),
        }
    }

    pub fn zero(length: u64) -> Self {
        Self::Zero { length }
    }

    pub fn length(&self) -> u64 {
        match self {
            Self::Data { extent } => extent.length,
            Self::Zero { length } => *length,
        }
    }
}

/// Where the bytes of a candidate live.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataLocation {
    /// Data runs / cluster chains. The last extent may include cluster slack;
    /// the writer truncates to `Candidate::size`.
    Extents { extents: Vec<Extent> },
    /// Data runs that include holes or unwritten regions. Segment lengths add
    /// up to the stored (not necessarily logical) length, and zero segments are
    /// emitted as zero bytes.
    Segments { segments: Vec<Segment> },
    /// Small file stored inside filesystem metadata (NTFS resident data).
    Resident { bytes: Vec<u8> },
    /// A carved byte range.
    Raw { offset: u64, length: u64 },
    /// Metadata exists but content cannot be located.
    None,
}

impl DataLocation {
    pub fn extents(extents: Vec<Extent>) -> Self {
        Self::Extents { extents }
    }

    pub fn segments(segments: Vec<Segment>) -> Self {
        Self::Segments { segments }
    }

    pub fn resident(bytes: Vec<u8>) -> Self {
        Self::Resident { bytes }
    }

    pub fn raw(offset: u64, length: u64) -> Self {
        Self::Raw { offset, length }
    }

    pub fn none() -> Self {
        Self::None
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Database,
    Executable,
    SourceCode,
    Other,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
            Self::Archive => "archive",
            Self::Database => "database",
            Self::Executable => "executable",
            Self::SourceCode => "source",
            Self::Other => "other",
        }
    }

    pub fn from_format(format: &str) -> Self {
        match format {
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "webp" | "heic" | "heif"
            | "avif" | "cr2" | "nef" | "arw" | "dng" | "orf" | "rw2" | "raf" | "psd" => Self::Image,
            "mp4" | "mov" | "m4v" | "mkv" | "webm" | "avi" | "wmv" | "flv" | "mpg" | "mpeg"
            | "3gp" | "ts" => Self::Video,
            "mp3" | "wav" | "flac" | "ogg" | "oga" | "aac" | "m4a" | "wma" | "aiff" | "opus" => {
                Self::Audio
            }
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "rtf" | "odt" | "ods"
            | "odp" | "epub" | "txt" | "md" | "csv" | "html" | "htm" | "xml" | "json" | "eml"
            | "msg" => Self::Document,
            "zip" | "rar" | "7z" | "gz" | "gzip" | "bz2" | "xz" | "tar" | "zst" | "zstd"
            | "iso" => Self::Archive,
            "sqlite" | "db" | "sqlite3" | "mdb" | "accdb" | "dbf" => Self::Database,
            "exe" | "dll" | "elf" | "so" | "dylib" | "bin" | "msi" => Self::Executable,
            "rs" | "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "go" | "java" | "cs" | "rb"
            | "php" | "sh" | "ps1" => Self::SourceCode,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryState {
    Deleted,
    Live,
    /// Found by content carving; there is no filesystem entry behind it.
    Carved,
}

impl EntryState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deleted => "deleted",
            Self::Live => "live",
            Self::Carved => "carved",
        }
    }
}

/// One recoverable item.
///
/// `path` is the *original* path when the filesystem metadata preserved it. It
/// is not an output path; the recovery writer sanitizes it and confines output
/// to the chosen destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub path: String,
    pub size: u64,
    pub format: String,
    pub category: Category,
    pub data: DataLocation,
    pub method: RecoveryMethod,
    pub confidence: Confidence,
    pub state: EntryState,
    pub complete: bool,
    pub filesystem: String,
    #[serde(default)]
    pub volume_offset: u64,
    #[serde(default)]
    pub record_id: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    #[serde(default)]
    pub accessed: Option<String>,
    #[serde(default)]
    pub deleted: Option<String>,
    #[serde(default)]
    pub validation: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
}

impl Candidate {
    pub fn new(
        name: impl Into<String>,
        size: u64,
        format: impl Into<String>,
        data: DataLocation,
        method: RecoveryMethod,
        confidence: Confidence,
        filesystem: impl Into<String>,
    ) -> Self {
        let format = format.into().to_ascii_lowercase();
        Self {
            id: 0,
            name: name.into(),
            path: String::new(),
            size,
            category: Category::from_format(&format),
            format,
            data,
            method,
            confidence,
            state: EntryState::Deleted,
            complete: false,
            filesystem: filesystem.into(),
            volume_offset: 0,
            record_id: None,
            created: None,
            modified: None,
            accessed: None,
            deleted: None,
            validation: Vec::new(),
            notes: Vec::new(),
            content_sha256: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn with_record_id(mut self, id: impl Into<String>) -> Self {
        self.record_id = Some(id.into());
        self
    }

    pub fn with_complete(mut self, complete: bool) -> Self {
        self.complete = complete;
        self
    }

    pub fn with_state(mut self, state: EntryState) -> Self {
        self.state = state;
        self
    }

    pub fn with_volume_offset(mut self, offset: u64) -> Self {
        self.volume_offset = offset;
        self
    }

    /// The best display path: original path + name when a path was recovered,
    /// otherwise just the name.
    pub fn display_path(&self) -> String {
        let p = self.path.trim_matches('/');
        if p.is_empty() {
            self.name.clone()
        } else {
            format!("{p}/{}", self.name)
        }
    }

    /// Total bytes described by the data location. For extent lists this can
    /// exceed `size` because the final cluster includes slack.
    pub fn stored_bytes(&self) -> u64 {
        match &self.data {
            DataLocation::Extents { extents } => extents
                .iter()
                .fold(0u64, |acc, e| acc.saturating_add(e.length)),
            DataLocation::Segments { segments } => segments
                .iter()
                .fold(0u64, |acc, s| acc.saturating_add(s.length())),
            DataLocation::Resident { bytes } => bytes.len() as u64,
            DataLocation::Raw { length, .. } => *length,
            DataLocation::None => 0,
        }
    }

    pub fn has_content(&self) -> bool {
        !self.data.is_none() && self.stored_bytes() > 0
    }

    pub fn is_jpeg(&self) -> bool {
        self.format == "jpg"
    }

    pub fn resident_hex(&self) -> Option<String> {
        match &self.data {
            DataLocation::Resident { bytes } => Some(hex(bytes)),
            _ => None,
        }
    }
}

/// Coalesce adjacent extents. Keeps session files smaller and recovery reads
/// sequential where the filesystem laid a file out contiguously.
pub fn coalesce_extents(mut extents: Vec<Extent>) -> Vec<Extent> {
    extents.retain(|e| e.length > 0);
    extents.sort_by_key(|e| e.offset);
    let mut out: Vec<Extent> = Vec::with_capacity(extents.len());
    for e in extents {
        if let Some(last) = out.last_mut() {
            if last.end() == Some(e.offset) {
                last.length = last.length.saturating_add(e.length);
                continue;
            }
        }
        out.push(e);
    }
    out
}

/// Infer a format token from a filename. This is only a hint; carving validates
/// actual content instead of trusting extensions.
pub fn format_from_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match base.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && ext.len() <= 12 => {
            let e = ext.to_ascii_lowercase();
            match e.as_str() {
                "jpeg" => "jpg".to_string(),
                "tif" => "tiff".to_string(),
                "htm" => "html".to_string(),
                other => other.to_string(),
            }
        }
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extents_coalesce_and_sort() {
        let got = coalesce_extents(vec![
            Extent::new(100, 10),
            Extent::new(0, 10),
            Extent::new(110, 5),
            Extent::new(10, 0),
            Extent::new(10, 10),
        ]);
        assert_eq!(got, vec![Extent::new(0, 20), Extent::new(100, 15)]);
    }

    #[test]
    fn formats_are_classified_without_trusting_odd_extensions() {
        assert_eq!(format_from_name("a.JPEG"), "jpg");
        assert_eq!(format_from_name("noext"), "unknown");
        assert_eq!(Category::from_format("jpg"), Category::Image);
        assert_eq!(Category::from_format("docx"), Category::Document);
        assert_eq!(Category::from_format("sqlite"), Category::Database);
        assert_eq!(Category::from_format("weird"), Category::Other);
    }

    #[test]
    fn display_path_uses_original_path_when_present() {
        let c = Candidate::new(
            "note.txt",
            10,
            "txt",
            DataLocation::none(),
            RecoveryMethod::MetadataOnly,
            Confidence::new(ConfidenceLabel::Unverified, 10, vec![]),
            "ntfs",
        )
        .with_path("/Docs");
        assert_eq!(c.display_path(), "Docs/note.txt");
        assert!(!c.has_content());
    }
}
