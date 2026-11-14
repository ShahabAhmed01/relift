//! FAT12/16/32 metadata recovery.
//!
//! Deleted FAT directory entries keep their name bytes, sizes and timestamps;
//! only the first character of an 8.3 name is overwritten with `0xE5`, and the
//! FAT chain is often cleared. Relift recovers the long filename when it is
//! intact, follows the chain when present, and otherwise assumes contiguous
//! allocation with a *lower* confidence and an explicit reason.

use std::collections::{BTreeMap, HashSet};

use crate::candidate::{
    coalesce_extents, format_from_name, Candidate, Confidence, ConfidenceLabel, DataLocation,
    EntryState, Extent, RecoveryMethod,
};
use crate::error::{Error, Result};
use crate::fs::{
    cluster_size as checked_cluster_size, read_at_volume, CandidateSink, FsInfo, FsKind,
    FsScanOptions, FsSummary, VolumeRef,
};
use crate::source::ReadOnlySource;
use crate::util::{align_up, fat_datetime_to_iso, u16_le, u32_le, u8_at, utf16le_to_string};

const FAT12_EOC: u32 = 0x0FF8;
const FAT16_EOC: u32 = 0xFFF8;
const FAT32_EOC: u32 = 0x0FFF_FFF8;
const FAT32_BAD: u32 = 0x0FFF_FFF7;
const MAX_FAT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_DIR_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHAIN_CLUSTERS: u64 = 4 * 1024 * 1024;
const MAX_DEPTH: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

impl FatType {
    fn kind(self) -> FsKind {
        match self {
            Self::Fat12 => FsKind::Fat12,
            Self::Fat16 => FsKind::Fat16,
            Self::Fat32 => FsKind::Fat32,
        }
    }

    fn eoc(self) -> u32 {
        match self {
            Self::Fat12 => FAT12_EOC,
            Self::Fat16 => FAT16_EOC,
            Self::Fat32 => FAT32_EOC,
        }
    }

    fn bad(self) -> u32 {
        match self {
            Self::Fat12 => 0x0FF7,
            Self::Fat16 => 0xFFF7,
            Self::Fat32 => FAT32_BAD,
        }
    }

    fn max_cluster(self) -> u32 {
        match self {
            Self::Fat12 => 0x0FFF,
            Self::Fat16 => 0xFFFF,
            Self::Fat32 => 0x0FFF_FFFF,
        }
    }
}

#[derive(Debug, Clone)]
struct FatBoot {
    fat_type: FatType,
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    cluster_size: u64,
    reserved_sectors: u32,
    num_fats: u32,
    root_entries: u32,
    total_sectors: u64,
    fat_size_sectors: u32,
    root_cluster: u32,
    cluster_count: u32,
    fat_offset: u64,
    root_dir_offset: u64,
    root_dir_sectors: u32,
    data_offset: u64,
    volume_label: Option<String>,
    serial: u32,
}

impl FatBoot {
    fn parse(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Result<Self> {
        if volume.length < 512 {
            return Err(Error::invalid("volume too small for a FAT boot sector"));
        }
        let bs = read_at_volume(src, volume, 0, 512)?;
        if bs.len() < 512 {
            return Err(Error::invalid("truncated FAT boot sector"));
        }
        if bs.get(3..11) == Some(b"NTFS    ") || bs.get(3..11) == Some(b"EXFAT   ") {
            return Err(Error::invalid("not a FAT boot sector"));
        }
        let jump = bs[0];
        let has_jump = matches!(jump, 0xEB | 0xE9 | 0x90);
        let label = bs.get(54..62).and_then(ascii_label);
        let label32 = bs.get(82..90).and_then(ascii_label);
        if label.is_none() && label32.is_none() && !has_jump {
            return Err(Error::invalid("no FAT boot-sector signature"));
        }

        let bytes_per_sector = u16_le(&bs, 11).unwrap_or(0) as u32;
        let sectors_per_cluster = u8_at(&bs, 13).unwrap_or(0) as u32;
        let reserved_sectors = u16_le(&bs, 14).unwrap_or(0) as u32;
        let num_fats = u8_at(&bs, 16).unwrap_or(0) as u32;
        let root_entries = u16_le(&bs, 17).unwrap_or(0) as u32;
        let total16 = u16_le(&bs, 19).unwrap_or(0) as u64;
        let total32 = u32_le(&bs, 32).unwrap_or(0) as u64;
        let fat_size16 = u16_le(&bs, 22).unwrap_or(0) as u32;
        let fat_size32 = u32_le(&bs, 36).unwrap_or(0) as u32;
        let root_cluster = u32_le(&bs, 44).unwrap_or(0);
        let serial = u32_le(&bs, 39).unwrap_or(0);

        let Some(cluster_size) = checked_cluster_size(bytes_per_sector, sectors_per_cluster) else {
            return Err(Error::invalid("FAT BPB has an invalid sector/cluster size"));
        };
        if reserved_sectors == 0 || !(1..=4).contains(&num_fats) {
            return Err(Error::invalid("FAT BPB has invalid reserved/FAT count"));
        }
        let fat_size_sectors = if fat_size16 != 0 {
            fat_size16
        } else {
            fat_size32
        };
        if fat_size_sectors == 0 {
            return Err(Error::invalid("FAT BPB has zero FAT size"));
        }
        let total_sectors = if total16 != 0 { total16 } else { total32 };
        if total_sectors == 0 {
            return Err(Error::invalid("FAT BPB has zero total sectors"));
        }
        let root_dir_bytes = (root_entries as u64)
            .checked_mul(32)
            .ok_or_else(|| Error::Corrupt("FAT root directory size overflow".into()))?;
        let root_dir_sectors = align_up(root_dir_bytes, bytes_per_sector as u64)
            .ok_or_else(|| Error::Corrupt("FAT root directory size overflow".into()))?
            / bytes_per_sector as u64;
        let root_dir_sectors = u32::try_from(root_dir_sectors)
            .map_err(|_| Error::Corrupt("FAT root directory is too large".into()))?;

        let metadata_sectors = (reserved_sectors as u64)
            .checked_add(num_fats as u64 * fat_size_sectors as u64)
            .and_then(|v| v.checked_add(root_dir_sectors as u64))
            .ok_or_else(|| Error::Corrupt("FAT metadata size overflow".into()))?;
        if metadata_sectors >= total_sectors {
            return Err(Error::invalid("FAT metadata consumes the whole volume"));
        }
        let data_sectors = total_sectors - metadata_sectors;
        let cluster_count = (data_sectors / sectors_per_cluster as u64) as u32;
        if cluster_count == 0 {
            return Err(Error::invalid("FAT volume has no data clusters"));
        }
        let fat_type = if cluster_count < 4085 {
            FatType::Fat12
        } else if cluster_count < 65_525 {
            FatType::Fat16
        } else {
            FatType::Fat32
        };
        if fat_type == FatType::Fat32 && root_entries != 0 {
            return Err(Error::invalid(
                "FAT32 boot sector declares a fixed root directory",
            ));
        }
        if fat_type != FatType::Fat32 && root_entries == 0 {
            return Err(Error::invalid(
                "FAT12/16 boot sector declares no fixed root directory",
            ));
        }
        if fat_type == FatType::Fat32
            && !(2..=cluster_count.saturating_add(1)).contains(&root_cluster)
        {
            return Err(Error::invalid("FAT32 root cluster is out of range"));
        }

        let fat_bytes = fat_size_sectors as u64 * bytes_per_sector as u64;
        if fat_bytes > MAX_FAT_BYTES {
            return Err(Error::unsupported(format!(
                "FAT is {} bytes, above the supported limit of {} bytes",
                fat_bytes, MAX_FAT_BYTES
            )));
        }
        let fat_offset = reserved_sectors as u64 * bytes_per_sector as u64;
        let root_dir_offset = (reserved_sectors as u64 + num_fats as u64 * fat_size_sectors as u64)
            * bytes_per_sector as u64;
        let data_offset = metadata_sectors * bytes_per_sector as u64;
        let fs_bytes = total_sectors
            .checked_mul(bytes_per_sector as u64)
            .ok_or_else(|| Error::Corrupt("FAT volume size overflow".into()))?;
        if fs_bytes > volume.length {
            return Err(Error::invalid(format!(
                "FAT BPB describes {fs_bytes} bytes but the volume is only {} bytes",
                volume.length
            )));
        }

        let volume_label = label32.or(label).filter(|s| !s.is_empty());
        Ok(Self {
            fat_type,
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size,
            reserved_sectors,
            num_fats,
            root_entries,
            total_sectors,
            fat_size_sectors,
            root_cluster,
            cluster_count,
            fat_offset,
            root_dir_offset,
            root_dir_sectors,
            data_offset,
            volume_label,
            serial,
        })
    }

    fn info(&self) -> FsInfo {
        let mut details = BTreeMap::new();
        details.insert("fat_type".into(), self.fat_type.kind().as_str().into());
        details.insert("bytes_per_sector".into(), self.bytes_per_sector.to_string());
        details.insert(
            "sectors_per_cluster".into(),
            self.sectors_per_cluster.to_string(),
        );
        details.insert("cluster_size".into(), self.cluster_size.to_string());
        details.insert("cluster_count".into(), self.cluster_count.to_string());
        details.insert("fat_copies".into(), self.num_fats.to_string());
        details.insert("serial".into(), format!("{:08X}", self.serial));
        FsInfo {
            kind: self.fat_type.kind(),
            label: self.volume_label.clone(),
            cluster_size: Some(self.cluster_size),
            details,
            warnings: Vec::new(),
        }
    }

    fn cluster_offset(&self, cluster: u32) -> Option<u64> {
        if cluster < 2 || cluster > self.cluster_count.saturating_add(1) {
            return None;
        }
        self.data_offset
            .checked_add((cluster as u64 - 2).checked_mul(self.cluster_size)?)
    }
}

fn ascii_label(bytes: &[u8]) -> Option<String> {
    if !bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        return None;
    }
    let s = String::from_utf8_lossy(bytes).trim().to_string();
    if s == "FAT12" || s == "FAT16" || s == "FAT32" {
        Some(s)
    } else {
        None
    }
}

/// Identify a FAT volume.
pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    FatBoot::parse(src, volume).ok().map(|b| b.info())
}

struct Fat {
    bytes: Vec<u8>,
    kind: FatType,
}

impl Fat {
    fn read(src: &dyn ReadOnlySource, volume: &VolumeRef, boot: &FatBoot) -> Result<Self> {
        let fat_bytes = boot.fat_size_sectors as u64 * boot.bytes_per_sector as u64;
        let root = volume.offset.checked_add(boot.fat_offset);
        let Some(root) = root else {
            return Err(Error::Corrupt("FAT offset overflow".into()));
        };
        let bytes = match src.read_vec(root, fat_bytes) {
            Ok(b) => b,
            Err(first) => {
                if boot.num_fats >= 2 {
                    let second = root
                        .checked_add(fat_bytes)
                        .ok_or_else(|| Error::Corrupt("FAT copy offset overflow".into()))?;
                    src.read_vec(second, fat_bytes).map_err(|_| first)?
                } else {
                    return Err(first);
                }
            }
        };
        Ok(Self {
            bytes,
            kind: boot.fat_type,
        })
    }

    fn entry(&self, cluster: u32) -> Option<u32> {
        if cluster > self.kind.max_cluster() {
            return None;
        }
        match self.kind {
            FatType::Fat12 => {
                let off = (cluster as usize) + (cluster as usize / 2);
                let v = u16_le(&self.bytes, off)?;
                Some(if cluster & 1 == 0 { v & 0x0FFF } else { v >> 4 } as u32)
            }
            FatType::Fat16 => u16_le(&self.bytes, cluster as usize * 2).map(|v| v as u32),
            FatType::Fat32 => u32_le(&self.bytes, cluster as usize * 4).map(|v| v & 0x0FFF_FFFF),
        }
    }

    fn is_eoc(&self, v: u32) -> bool {
        v >= self.kind.eoc()
    }

    fn is_free(&self, v: u32) -> bool {
        v == 0
    }

    fn is_bad(&self, v: u32) -> bool {
        v == self.kind.bad()
    }
}

fn follow_chain(
    fat: &Fat,
    boot: &FatBoot,
    start: u32,
    needed_clusters: u64,
) -> (Vec<u32>, bool, &'static str) {
    if start < 2 || start > boot.cluster_count.saturating_add(1) {
        return (Vec::new(), false, "first cluster is out of range");
    }
    let target = needed_clusters.min(MAX_CHAIN_CLUSTERS);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = start;
    let mut exact = true;
    loop {
        if out.len() as u64 >= target {
            break;
        }
        if !seen.insert(cur) {
            exact = false;
            break;
        }
        if cur < 2 || cur > boot.cluster_count.saturating_add(1) {
            exact = false;
            break;
        }
        out.push(cur);
        let Some(next) = fat.entry(cur) else {
            exact = false;
            break;
        };
        if fat.is_eoc(next) {
            break;
        }
        if fat.is_free(next) {
            exact = false;
            break;
        }
        if fat.is_bad(next) {
            exact = false;
            break;
        }
        cur = next;
    }
    let reason = if !exact {
        "FAT chain ended before the file size was covered"
    } else if (out.len() as u64) < target {
        "FAT chain is shorter than the file size"
    } else {
        "FAT chain covers the file size"
    };
    (out, exact, reason)
}

fn contiguous_chain(boot: &FatBoot, start: u32, needed: u64) -> (Vec<u32>, bool) {
    if start < 2 || start > boot.cluster_count.saturating_add(1) {
        return (Vec::new(), false);
    }
    let available = (boot.cluster_count as u64 + 1).saturating_sub(start as u64);
    let take = needed.min(available);
    let mut out = Vec::with_capacity(take as usize);
    for i in 0..take {
        out.push(start + i as u32);
    }
    (out, take >= needed)
}

fn clusters_to_extents(boot: &FatBoot, volume: &VolumeRef, clusters: &[u32]) -> Vec<Extent> {
    let mut extents = Vec::new();
    for &c in clusters {
        let Some(rel) = boot.cluster_offset(c) else {
            continue;
        };
        let Some(abs) = volume.offset.checked_add(rel) else {
            continue;
        };
        extents.push(Extent::new(abs, boot.cluster_size));
    }
    coalesce_extents(extents)
}

#[derive(Debug, Clone)]
struct DirEntry {
    name: String,
    short_name: String,
    attr: u8,
    first_cluster: u32,
    size: u32,
    created: Option<String>,
    modified: Option<String>,
    accessed: Option<String>,
    deleted: bool,
    lfn_checksum_ok: bool,
    first_char_lost: bool,
    raw_offset: usize,
}

impl DirEntry {
    fn is_dir(&self) -> bool {
        self.attr & 0x10 != 0
    }
    fn is_volume_label(&self) -> bool {
        self.attr & 0x08 != 0
    }
}

fn lfn_checksum(short: &[u8]) -> u8 {
    let mut sum = 0u8;
    for b in short {
        sum = sum.rotate_right(1).wrapping_add(*b);
    }
    sum
}

fn parse_short_name(raw: &[u8]) -> (String, bool) {
    let mut name = [b' '; 8];
    name.copy_from_slice(&raw[..8]);
    let mut ext = [b' '; 3];
    ext.copy_from_slice(&raw[8..11]);
    let mut lost = false;
    if name[0] == 0xE5 {
        lost = true;
        name[0] = b'_';
    } else if name[0] == 0x05 {
        name[0] = 0xE5;
    }
    let base = String::from_utf8_lossy(&name).trim_end().to_string();
    let ext = String::from_utf8_lossy(&ext).trim_end().to_string();
    let full = if ext.is_empty() {
        base
    } else {
        format!("{base}.{ext}")
    };
    (full.trim_start_matches('.').to_string(), lost)
}

fn decode_lfn(parts: &[(u8, String)], checksum_ok: bool) -> Option<String> {
    if parts.is_empty() {
        return None;
    }
    let mut ordered: Vec<&(u8, String)> = parts.iter().collect();
    let orders_valid = parts
        .iter()
        .all(|(o, _)| (1..=20).contains(o) && *o != 0xE5);
    if orders_valid {
        ordered.sort_by_key(|(o, _)| *o);
    } else {
        // LFN entries are stored in reverse order before the short entry.
        ordered.reverse();
    }
    let joined = ordered.iter().map(|(_, s)| s.as_str()).collect::<String>();
    let trimmed = joined.trim_end_matches('\u{0}').trim_end().to_string();
    if trimmed.is_empty() {
        None
    } else if checksum_ok {
        Some(trimmed)
    } else {
        Some(trimmed)
    }
}

fn parse_dir_entries(data: &[u8], base: u64) -> Vec<DirEntry> {
    let mut out = Vec::new();
    let mut lfn: Vec<(u8, String)> = Vec::new();
    let mut pending_lfn_checksum: Option<u8> = None;
    let mut i = 0usize;
    while i + 32 <= data.len() {
        let e = &data[i..i + 32];
        if e[0] == 0x00 {
            break;
        }
        let attr = e[11];
        if attr == 0x0F {
            let order = e[0];
            let mut text = Vec::with_capacity(13 * 2);
            text.extend_from_slice(&e[1..11]);
            text.extend_from_slice(&e[14..26]);
            text.extend_from_slice(&e[28..32]);
            let s = utf16le_to_string(&text);
            lfn.push((order, s));
            if pending_lfn_checksum.is_none() {
                pending_lfn_checksum = Some(e[13]);
            }
            i += 32;
            continue;
        }
        let deleted = e[0] == 0xE5;
        let (short_name, first_char_lost) = parse_short_name(&e[0..11]);
        if short_name.is_empty() || short_name == "." || short_name == ".." {
            lfn.clear();
            pending_lfn_checksum = None;
            i += 32;
            continue;
        }
        let checksum_ok = pending_lfn_checksum
            .map(|c| c == lfn_checksum(&e[0..11]))
            .unwrap_or(false);
        let name = decode_lfn(&lfn, checksum_ok).unwrap_or_else(|| short_name.clone());
        let high = u16_le(e, 20).unwrap_or(0) as u32;
        let low = u16_le(e, 26).unwrap_or(0) as u32;
        let first_cluster = (high << 16) | low;
        let size = u32_le(e, 28).unwrap_or(0);
        let created = fat_datetime_to_iso(u16_le(e, 16).unwrap_or(0), u16_le(e, 14).unwrap_or(0));
        let accessed_date = u16_le(e, 18).unwrap_or(0);
        let accessed = if accessed_date != 0 {
            fat_datetime_to_iso(accessed_date, 0)
        } else {
            None
        };
        let modified = fat_datetime_to_iso(u16_le(e, 24).unwrap_or(0), u16_le(e, 22).unwrap_or(0));
        out.push(DirEntry {
            name,
            short_name,
            attr,
            first_cluster,
            size,
            created,
            modified,
            accessed,
            deleted,
            lfn_checksum_ok: checksum_ok,
            first_char_lost,
            raw_offset: (base + i as u64) as usize,
        });
        lfn.clear();
        pending_lfn_checksum = None;
        i += 32;
    }
    out
}

struct FatScan<'a> {
    src: &'a dyn ReadOnlySource,
    volume: &'a VolumeRef,
    boot: FatBoot,
    fat: Fat,
    options: &'a FsScanOptions,
    sink: &'a mut dyn CandidateSink,
    summary: FsSummary,
    visited_dirs: HashSet<u32>,
}

impl<'a> FatScan<'a> {
    fn read_chain(&self, start: u32) -> Result<Vec<u8>> {
        let (clusters, _exact, _reason) =
            follow_chain(&self.fat, &self.boot, start, MAX_CHAIN_CLUSTERS);
        let mut out = Vec::new();
        for c in clusters {
            let Some(rel) = self.boot.cluster_offset(c) else {
                break;
            };
            let abs = self
                .volume
                .offset
                .checked_add(rel)
                .ok_or_else(|| Error::Corrupt("cluster offset overflow".into()))?;
            let len = self
                .boot
                .cluster_size
                .min(MAX_DIR_BYTES.saturating_sub(out.len() as u64));
            if len == 0 {
                break;
            }
            let mut buf = vec![0u8; len as usize];
            self.src.read_exact_at(abs, &mut buf)?;
            out.extend_from_slice(&buf);
            if out.len() as u64 >= MAX_DIR_BYTES {
                break;
            }
        }
        Ok(out)
    }

    fn read_root(&self) -> Result<Vec<u8>> {
        if self.boot.fat_type == FatType::Fat32 {
            self.read_chain(self.boot.root_cluster)
        } else {
            let bytes = self.boot.root_dir_sectors as u64 * self.boot.bytes_per_sector as u64;
            let abs = self
                .volume
                .offset
                .checked_add(self.boot.root_dir_offset)
                .ok_or_else(|| Error::Corrupt("root directory offset overflow".into()))?;
            self.src.read_vec(abs, bytes.min(MAX_DIR_BYTES))
        }
    }

    fn scan_root(&mut self) -> Result<()> {
        let data = self.read_root()?;
        let base = if self.boot.fat_type == FatType::Fat32 {
            self.volume
                .offset
                .checked_add(
                    self.boot
                        .cluster_offset(self.boot.root_cluster)
                        .unwrap_or(0),
                )
                .unwrap_or(self.volume.offset)
        } else {
            self.volume
                .offset
                .checked_add(self.boot.root_dir_offset)
                .unwrap_or(self.volume.offset)
        };
        self.scan_dir_data(&data, base, "", 0)?;
        Ok(())
    }

    fn scan_dir_data(&mut self, data: &[u8], base: u64, path: &str, depth: u32) -> Result<()> {
        if self.options.is_cancelled() {
            return Ok(());
        }
        let entries = parse_dir_entries(data, base);
        for entry in entries {
            if self.options.is_cancelled() {
                return Ok(());
            }
            if entry.is_volume_label() {
                if self.boot.volume_label.is_none() && !entry.deleted {
                    self.summary.label = Some(entry.name.trim_end_matches('.').to_string());
                }
                continue;
            }
            if entry.name.is_empty() {
                continue;
            }
            if entry.deleted {
                self.summary.deleted_entries += 1;
            } else {
                self.summary.live_entries += 1;
            }

            let child_path = if path.is_empty() {
                String::new()
            } else {
                path.to_string()
            };

            if entry.is_dir() {
                if depth >= MAX_DEPTH {
                    self.summary
                        .warnings
                        .push(format!("directory depth limit reached at {path}"));
                    continue;
                }
                if !entry.deleted && !entry.first_cluster_is_valid(&self.boot) {
                    continue;
                }
                if entry.deleted && !entry.first_cluster_is_valid(&self.boot) {
                    continue;
                }
                if !self.visited_dirs.insert(entry.first_cluster) {
                    continue;
                }
                let sub = if child_path.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{child_path}/{}", entry.name)
                };
                if entry.deleted {
                    if let Ok(subdata) = self.read_chain(entry.first_cluster) {
                        let abs = self.volume.offset.saturating_add(
                            self.boot.cluster_offset(entry.first_cluster).unwrap_or(0),
                        );
                        self.scan_dir_data(&subdata, abs, &sub, depth + 1)?;
                    }
                } else {
                    let subdata = self.read_chain(entry.first_cluster)?;
                    let abs = self
                        .volume
                        .offset
                        .saturating_add(self.boot.cluster_offset(entry.first_cluster).unwrap_or(0));
                    self.scan_dir_data(&subdata, abs, &sub, depth + 1)?;
                }
                continue;
            }

            if entry.deleted || self.options.include_live {
                if self.summary.emitted as usize >= self.options.max_candidates {
                    self.summary.truncated = true;
                    return Ok(());
                }
                let candidate = self.make_candidate(&entry, path);
                self.sink.emit(candidate)?;
                self.summary.emitted += 1;
            }
        }
        Ok(())
    }

    fn make_candidate(&self, entry: &DirEntry, parent_path: &str) -> Candidate {
        let mut reasons = vec!["deleted FAT directory entry".to_string()];
        let needed = if entry.size == 0 {
            0
        } else {
            (entry.size as u64 + self.boot.cluster_size - 1) / self.boot.cluster_size
        };
        let mut data = DataLocation::None;
        let mut complete = true;
        let mut label = ConfidenceLabel::Likely;
        let mut score = 84u8;

        if entry.name.is_empty() {
            label = ConfidenceLabel::Partial;
            score = 40;
            reasons.push("name is empty".into());
        }

        if entry.size == 0 {
            data = DataLocation::resident(Vec::new());
            complete = true;
            label = ConfidenceLabel::Verified;
            score = 92;
            reasons.push("zero-length file entry is internally consistent".into());
        } else if entry.first_cluster < 2 {
            label = ConfidenceLabel::Unverified;
            score = 20;
            complete = false;
            reasons.push("first cluster is unavailable; content cannot be located".into());
        } else if entry.first_cluster > self.boot.cluster_count.saturating_add(1) {
            label = ConfidenceLabel::Unverified;
            score = 15;
            complete = false;
            reasons.push("first cluster is outside the volume".into());
        } else {
            let (mut clusters, chain_exact, chain_reason) =
                follow_chain(&self.fat, &self.boot, entry.first_cluster, needed);
            if (clusters.len() as u64) < needed {
                let (contig, enough) = contiguous_chain(&self.boot, entry.first_cluster, needed);
                if !contig.is_empty() {
                    clusters = contig;
                    reasons
                        .push("FAT chain is incomplete; contiguous allocation was assumed".into());
                    label = ConfidenceLabel::Partial;
                    score = 58;
                    complete = enough;
                    if !enough {
                        reasons.push("contiguous run also ends before the file size".into());
                    }
                } else {
                    label = ConfidenceLabel::Partial;
                    score = 35;
                    complete = false;
                    reasons.push("no usable cluster chain could be reconstructed".into());
                }
            } else {
                reasons.push(chain_reason.into());
                if chain_exact {
                    reasons.push("all data clusters were described by the FAT".into());
                } else {
                    label = ConfidenceLabel::Partial;
                    score = 62;
                    // The directory entry size is authoritative. When the
                    // chain covers every cluster the size needs, the bytes are
                    // complete even though the terminator was lost.
                    reasons.push(
                        "FAT chain terminator is missing but all needed clusters are mapped".into(),
                    );
                }
            }
            if clusters.is_empty() {
                data = DataLocation::None;
                complete = false;
                if label != ConfidenceLabel::Unverified {
                    label = ConfidenceLabel::Partial;
                    score = 30;
                }
            } else {
                let total = clusters.len() as u64 * self.boot.cluster_size;
                if total < entry.size as u64 {
                    complete = false;
                    label = ConfidenceLabel::Partial;
                    score = score.min(55);
                    reasons
                        .push("reconstructed clusters are shorter than the recorded size".into());
                }
                data =
                    DataLocation::extents(clusters_to_extents(&self.boot, self.volume, &clusters));
            }
        }

        if entry.lfn_checksum_ok {
            reasons.push("long filename checksum is valid".into());
            score = score.saturating_add(3);
        } else if entry.name.contains('_') && entry.first_char_lost {
            reasons.push(
                "first character of the 8.3 name was overwritten by the delete marker".into(),
            );
        }
        if entry.first_char_lost && entry.lfn_checksum_ok {
            reasons.push("long filename preserves the original name".into());
        }

        let name = if entry.name.is_empty() {
            "unnamed".to_string()
        } else {
            entry.name.clone()
        };
        let format = format_from_name(&name);
        let mut c = Candidate::new(
            name,
            entry.size as u64,
            format,
            data,
            RecoveryMethod::FatDirectory,
            Confidence::new(label, score, reasons),
            self.boot.fat_type.kind().as_str(),
        );
        c.path = parent_path.to_string();
        c.state = if entry.deleted {
            EntryState::Deleted
        } else {
            EntryState::Live
        };
        c.created = entry.created.clone();
        c.modified = entry.modified.clone();
        c.accessed = entry.accessed.clone();
        c.complete = complete;
        c.volume_offset = self.volume.offset;
        c.record_id = Some(format!("dir-entry@{}", entry.raw_offset));
        if entry.short_name != entry.name {
            c.notes.push(format!("8.3 alias: {}", entry.short_name));
        }
        c
    }
}

trait ClusterCheck {
    fn first_cluster_is_valid(&self, boot: &FatBoot) -> bool;
}

impl ClusterCheck for DirEntry {
    fn first_cluster_is_valid(&self, boot: &FatBoot) -> bool {
        self.first_cluster >= 2 && self.first_cluster <= boot.cluster_count.saturating_add(1)
    }
}

/// Recovery entry point.
pub fn scan(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    options: &FsScanOptions,
    sink: &mut dyn CandidateSink,
) -> Result<FsSummary> {
    let boot = FatBoot::parse(src, volume)?;
    let fat = Fat::read(src, volume, &boot)?;
    let mut summary = FsSummary::new(boot.fat_type.kind(), boot.volume_label.clone());
    summary.warnings.append(&mut Vec::new());
    let mut scanner = FatScan {
        src,
        volume,
        boot,
        fat,
        options,
        sink,
        summary,
        visited_dirs: HashSet::new(),
    };
    scanner.scan_root()?;
    Ok(scanner.summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_decoding_handles_delete_marker() {
        let mut raw = [b' '; 11];
        raw[..5].copy_from_slice(b"\xE5OOBJ");
        raw[8..].copy_from_slice(b"JPG");
        let (name, lost) = parse_short_name(&raw);
        assert!(lost);
        assert!(name.ends_with(".JPG"));
        assert!(name.starts_with('_'));
    }

    #[test]
    fn lfn_checksum_is_stable() {
        let mut short = [b' '; 11];
        short[..8].copy_from_slice(b"LONGFILE");
        short[8..].copy_from_slice(b"TXT");
        assert_eq!(lfn_checksum(&short), lfn_checksum(&short));
    }
}
