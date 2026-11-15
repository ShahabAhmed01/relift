//! Filesystem identification and metadata recovery.
//!
//! Support is deliberately tiered. A filesystem is listed as metadata-capable
//! only when a parser in this crate implements deleted-entry recovery for it and
//! is covered by fixtures. Everything else is identified but reported as
//! unsupported for metadata recovery; raw carving may still find content.

// On-disk format definitions intentionally keep every documented field (and
// parsers for key types not yet wired into the recovery path) so the structs
// remain faithful to the published layouts and usable by follow-up work.
#![allow(dead_code)]

pub mod apfs;
pub mod btrfs;
pub mod exfat;
pub mod ext;
pub mod fat;
pub mod hfs;
pub mod ntfs;
pub mod xfs;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::candidate::Candidate;
use crate::error::Result;
use crate::source::ReadOnlySource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsKind {
    Ntfs,
    Fat12,
    Fat16,
    Fat32,
    ExFat,
    Ext2,
    Ext3,
    Ext4,
    HfsPlus,
    Apfs,
    Btrfs,
    Xfs,
    Refs,
    Iso9660,
    Unknown,
}

impl FsKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ntfs => "ntfs",
            Self::Fat12 => "fat12",
            Self::Fat16 => "fat16",
            Self::Fat32 => "fat32",
            Self::ExFat => "exfat",
            Self::Ext2 => "ext2",
            Self::Ext3 => "ext3",
            Self::Ext4 => "ext4",
            Self::HfsPlus => "hfs+",
            Self::Apfs => "apfs",
            Self::Btrfs => "btrfs",
            Self::Xfs => "xfs",
            Self::Refs => "refs",
            Self::Iso9660 => "iso9660",
            Self::Unknown => "unknown",
        }
    }

    pub fn metadata_supported(self) -> bool {
        matches!(
            self,
            Self::Ntfs
                | Self::Fat12
                | Self::Fat16
                | Self::Fat32
                | Self::ExFat
                | Self::Ext2
                | Self::Ext3
                | Self::Ext4
        )
    }

    pub fn is_fat(self) -> bool {
        matches!(self, Self::Fat12 | Self::Fat16 | Self::Fat32)
    }

    pub fn is_ext(self) -> bool {
        matches!(self, Self::Ext2 | Self::Ext3 | Self::Ext4)
    }
}

#[derive(Debug, Clone)]
pub struct VolumeRef {
    pub offset: u64,
    pub length: u64,
    /// Human label from the partition table, if any.
    pub label: Option<String>,
}

impl VolumeRef {
    pub fn new(offset: u64, length: u64) -> Self {
        Self {
            offset,
            length,
            label: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsInfo {
    pub kind: FsKind,
    pub label: Option<String>,
    pub cluster_size: Option<u64>,
    pub details: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FsScanOptions {
    /// Include entries that are still allocated (forensic listing). Default is
    /// deleted-only.
    pub include_live: bool,
    /// Hard cap on emitted candidates for this scan.
    pub max_candidates: usize,
    pub cancelled: Arc<AtomicBool>,
}

impl Default for FsScanOptions {
    fn default() -> Self {
        Self {
            include_live: false,
            max_candidates: 1_000_000,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl FsScanOptions {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Receives candidates as they are discovered. Implementations must be
/// streaming; the engine does not buffer a whole volume's results in memory.
pub trait CandidateSink {
    fn emit(&mut self, candidate: Candidate) -> Result<()>;
}

/// Collects candidates in memory. Intended for tests and small volumes.
#[derive(Debug, Default)]
pub struct VecSink {
    pub items: Vec<Candidate>,
}

impl CandidateSink for VecSink {
    fn emit(&mut self, candidate: Candidate) -> Result<()> {
        self.items.push(candidate);
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FsSummary {
    pub kind: Option<FsKind>,
    pub label: Option<String>,
    pub live_entries: u64,
    pub deleted_entries: u64,
    pub emitted: u64,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

impl FsSummary {
    pub fn new(kind: FsKind, label: Option<String>) -> Self {
        Self {
            kind: Some(kind),
            label,
            ..Default::default()
        }
    }
}

/// Identify a filesystem at `volume.offset` without trusting the partition
/// table type byte.
pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    if volume.length < 512 || volume.offset >= src.len() {
        return None;
    }
    let read_len = volume.length.min(128 * 1024);
    let buf = src.read_vec(volume.offset, read_len).ok()?;

    if buf.len() >= 512 && buf.get(3..11) == Some(b"NTFS    ") {
        return ntfs::probe(src, volume);
    }
    if buf.len() >= 512 && buf.get(3..11) == Some(b"EXFAT   ") {
        return exfat::probe(src, volume);
    }
    if let Some(info) = fat::probe(src, volume) {
        return Some(info);
    }
    if let Some(info) = ext::probe(src, volume) {
        return Some(info);
    }

    // Identify common filesystems we do not yet parse, so the UI can be honest
    // instead of silently returning "no deleted files".
    if buf.len() >= 2048 {
        if u16_be_at(&buf, 1024 + 2) == Some(0x482B) {
            return Some(identify(FsKind::HfsPlus, "HFS+ volume header", &buf));
        }
        if buf.get(1024 + 3..1024 + 7) == Some(b"NXSB") {
            return Some(identify(FsKind::Apfs, "APFS container superblock", &buf));
        }
        if buf.get(0x10040..0x10048) == Some(b"_BHRfS_M") {
            return Some(identify(FsKind::Btrfs, "Btrfs superblock", &buf));
        }
        if buf.get(0..4) == Some(b"XFSB") {
            return Some(identify(FsKind::Xfs, "XFS superblock", &buf));
        }
        if buf.get(3..7) == Some(b"ReFS") {
            return Some(identify(FsKind::Refs, "ReFS volume header", &buf));
        }
        if buf.get(0x8001..0x8006) == Some(b"CD001") {
            return Some(identify(
                FsKind::Iso9660,
                "ISO 9660 primary volume descriptor",
                &buf,
            ));
        }
    }
    None
}

fn identify(kind: FsKind, detail: &str, _buf: &[u8]) -> FsInfo {
    let mut details = BTreeMap::new();
    details.insert("signature".to_string(), detail.to_string());
    FsInfo {
        kind,
        label: None,
        cluster_size: None,
        details,
        warnings: vec![format!(
            "{} is identified but metadata-based deleted-file recovery is not implemented for it",
            kind.as_str()
        )],
    }
}

fn u16_be_at(buf: &[u8], off: usize) -> Option<u16> {
    crate::util::u16_be(buf, off)
}

/// Run the metadata recovery engine for the filesystem detected in `volume`.
pub fn scan_metadata(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    options: &FsScanOptions,
    sink: &mut dyn CandidateSink,
) -> Result<FsSummary> {
    let Some(info) = probe(src, volume) else {
        return Ok(FsSummary {
            warnings: vec![
                "no known filesystem signature at this offset; use deep carving instead".into(),
            ],
            ..Default::default()
        });
    };
    match info.kind {
        FsKind::Ntfs => ntfs::scan(src, volume, options, sink),
        FsKind::ExFat => exfat::scan(src, volume, options, sink),
        FsKind::Apfs => apfs::scan(src, volume, options, sink),
        FsKind::HfsPlus => hfs::scan(src, volume, options, sink),
        FsKind::Btrfs => btrfs::scan(src, volume, options, sink),
        FsKind::Xfs => xfs::scan(src, volume, options, sink),
        k if k.is_fat() => fat::scan(src, volume, options, sink),
        k if k.is_ext() => ext::scan(src, volume, options, sink),
        other => Ok(FsSummary {
            kind: Some(other),
            label: info.label,
            warnings: vec![format!(
                "{} is identified but metadata recovery is not implemented; no files were enumerated",
                other.as_str()
            )],
            ..Default::default()
        }),
    }
}

/// Probe helper that reads and validates a boot sector for a specific parser.
pub(crate) fn read_at_volume(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    offset: u64,
    len: u64,
) -> Result<Vec<u8>> {
    let abs = volume
        .offset
        .checked_add(offset)
        .ok_or_else(|| crate::error::Error::Corrupt("volume-relative offset overflow".into()))?;
    if offset
        .checked_add(len)
        .map(|end| end > volume.length)
        .unwrap_or(true)
    {
        return Err(crate::error::Error::Corrupt(format!(
            "read {offset:#x}..+{len:#x} outside volume length {:#x}",
            volume.length
        )));
    }
    let max = len.min(crate::source::MAX_BUFFERED_READ);
    src.read_vec(abs, max)
}

/// Compute cluster size from BPB values with sanity bounds.
pub(crate) fn cluster_size(bytes_per_sector: u32, sectors_per_cluster: u32) -> Option<u64> {
    if !(128..=4096).contains(&bytes_per_sector)
        || !bytes_per_sector.is_power_of_two()
        || sectors_per_cluster == 0
        || sectors_per_cluster > 4096
        || !sectors_per_cluster.is_power_of_two()
    {
        return None;
    }
    let size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    if size > 1024 * 1024 {
        return None;
    }
    Some(size)
}
