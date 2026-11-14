//! exFAT metadata recovery.
//!
//! exFAT directory entries are 32-byte records grouped into sets: a file entry
//! followed by a stream extension and one or more file-name entries. Bit 7 of
//! the entry-type byte means "in use"; deleting a file clears it on every entry
//! in the set but leaves the name, length, timestamps, first cluster and (unless
//! the implementation rewrote it) the allocation flags intact. That is enough to
//! recover the file when the cluster heap has not been reused.
//!
//! The parser never trusts a deleted set blindly: it validates entry types,
//! secondary counts, name lengths, cluster bounds and data lengths, and it
//! reports when it had to fall back to contiguous allocation because the FAT
//! chain was cleared.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::candidate::{
    coalesce_extents, format_from_name, Candidate, Confidence, ConfidenceLabel, DataLocation,
    EntryState, Extent, RecoveryMethod,
};
use crate::error::{Error, Result};
use crate::fs::{
    read_at_volume, CandidateSink, FsInfo, FsKind, FsScanOptions, FsSummary, VolumeRef,
};
use crate::source::ReadOnlySource;
use crate::util::{u16_le, u32_le, u64_le, u8_at, utf16le_to_string};

const MAX_DIR_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DEPTH: u32 = 64;
const MAX_SECONDARIES: usize = 255;
const FAT_PAGE: u64 = 64 * 1024;
const FAT_CACHE_PAGES: usize = 32;

const ENTRY_FILE: u8 = 0x05;
const ENTRY_STREAM: u8 = 0x40;
const ENTRY_NAME: u8 = 0x41;
const ENTRY_LABEL: u8 = 0x03;
const ENTRY_BITMAP: u8 = 0x01;
const ENTRY_UPCASE: u8 = 0x02;

#[derive(Debug, Clone)]
struct ExfatBoot {
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    cluster_size: u64,
    volume_sectors: u64,
    fat_offset: u64,
    fat_length: u64,
    heap_offset: u64,
    cluster_count: u32,
    root_cluster: u32,
    num_fats: u32,
    serial: u32,
    revision: u16,
    label: Option<String>,
    warnings: Vec<String>,
}

impl ExfatBoot {
    fn parse(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Result<Self> {
        if volume.length < 512 {
            return Err(Error::invalid("volume too small for an exFAT boot sector"));
        }
        let bs = read_at_volume(src, volume, 0, 512)?;
        if bs.len() < 512 {
            return Err(Error::invalid("truncated exFAT boot sector"));
        }
        if bs.get(3..11) != Some(b"EXFAT   ") {
            return Err(Error::invalid("missing exFAT OEM signature"));
        }
        let mut warnings = Vec::new();

        let sector_shift = u8_at(&bs, 108).unwrap_or(0);
        if !(9..=12).contains(&sector_shift) {
            return Err(Error::invalid(format!(
                "exFAT bytes-per-sector shift {sector_shift} is outside 9..=12"
            )));
        }
        let bytes_per_sector = 1u32 << sector_shift;
        let cluster_shift = u8_at(&bs, 109).unwrap_or(0xFF);
        if sector_shift as u32 + cluster_shift as u32 > 25 {
            return Err(Error::invalid(format!(
                "exFAT sector+cluster shift {} exceeds 25",
                sector_shift as u32 + cluster_shift as u32
            )));
        }
        let sectors_per_cluster = 1u32 << cluster_shift;
        let cluster_size = (bytes_per_sector as u64) << cluster_shift;
        if cluster_size == 0 || cluster_size > 32 * 1024 * 1024 {
            return Err(Error::invalid("exFAT cluster size is out of range"));
        }

        let volume_sectors = u64_le(&bs, 72).unwrap_or(0);
        if volume_sectors == 0 {
            return Err(Error::invalid("exFAT volume declares zero sectors"));
        }
        let volume_bytes = volume_sectors
            .checked_mul(bytes_per_sector as u64)
            .ok_or_else(|| Error::Corrupt("exFAT volume size overflow".into()))?;
        if volume_bytes > volume.length {
            warnings.push(format!(
                "exFAT boot sector describes {volume_bytes} bytes but the volume is {} bytes; using the smaller bound",
                volume.length
            ));
        }

        let fat_offset_sectors = u32_le(&bs, 80).unwrap_or(0) as u64;
        let fat_length_sectors = u32_le(&bs, 84).unwrap_or(0) as u64;
        let heap_offset_sectors = u32_le(&bs, 88).unwrap_or(0) as u64;
        let cluster_count = u32_le(&bs, 92).unwrap_or(0);
        let root_cluster = u32_le(&bs, 96).unwrap_or(0);
        let num_fats = u8_at(&bs, 110).unwrap_or(0) as u32;
        let serial = u32_le(&bs, 100).unwrap_or(0);
        let revision = u16_le(&bs, 104).unwrap_or(0);

        if fat_offset_sectors == 0 || fat_length_sectors == 0 {
            return Err(Error::invalid("exFAT FAT offset/length is zero"));
        }
        if !(1..=2).contains(&num_fats) {
            return Err(Error::invalid("exFAT number of FATs is not 1 or 2"));
        }
        if cluster_count == 0 {
            return Err(Error::invalid("exFAT cluster count is zero"));
        }
        let first_data_sector = heap_offset_sectors;
        let fat_end = fat_offset_sectors
            .checked_add(fat_length_sectors.saturating_mul(num_fats as u64))
            .ok_or_else(|| Error::Corrupt("exFAT FAT range overflow".into()))?;
        if first_data_sector < fat_end {
            return Err(Error::invalid("exFAT cluster heap overlaps the FAT area"));
        }
        let heap_end = first_data_sector
            .checked_add(cluster_count as u64 * sectors_per_cluster as u64)
            .ok_or_else(|| Error::Corrupt("exFAT cluster heap range overflow".into()))?;
        if heap_end > volume_sectors {
            return Err(Error::invalid(
                "exFAT cluster heap extends beyond the volume",
            ));
        }
        if !(2..=cluster_count.saturating_add(1)).contains(&root_cluster) {
            return Err(Error::invalid(
                "exFAT root directory cluster is out of range",
            ));
        }

        let fat_length = fat_length_sectors
            .checked_mul(bytes_per_sector as u64)
            .ok_or_else(|| Error::Corrupt("exFAT FAT byte length overflow".into()))?;
        let fat_offset = fat_offset_sectors
            .checked_mul(bytes_per_sector as u64)
            .ok_or_else(|| Error::Corrupt("exFAT FAT byte offset overflow".into()))?;
        let heap_offset = heap_offset_sectors
            .checked_mul(bytes_per_sector as u64)
            .ok_or_else(|| Error::Corrupt("exFAT heap byte offset overflow".into()))?;

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size,
            volume_sectors,
            fat_offset,
            fat_length,
            heap_offset,
            cluster_count,
            root_cluster,
            num_fats,
            serial,
            revision,
            label: None,
            warnings,
        })
    }

    fn cluster_offset(&self, cluster: u32) -> Option<u64> {
        if !(2..=self.cluster_count.saturating_add(1)).contains(&cluster) {
            return None;
        }
        self.heap_offset
            .checked_add((cluster as u64 - 2).checked_mul(self.cluster_size)?)
    }

    fn info(&self) -> FsInfo {
        let mut details = BTreeMap::new();
        details.insert("bytes_per_sector".into(), self.bytes_per_sector.to_string());
        details.insert(
            "sectors_per_cluster".into(),
            self.sectors_per_cluster.to_string(),
        );
        details.insert("cluster_size".into(), self.cluster_size.to_string());
        details.insert("cluster_count".into(), self.cluster_count.to_string());
        details.insert("fat_copies".into(), self.num_fats.to_string());
        details.insert("serial".into(), format!("{:08X}", self.serial));
        details.insert(
            "revision".into(),
            format!("{}.{}", self.revision >> 8, self.revision & 0xFF),
        );
        FsInfo {
            kind: FsKind::ExFat,
            label: self.label.clone(),
            cluster_size: Some(self.cluster_size),
            details,
            warnings: self.warnings.clone(),
        }
    }
}

/// A paged reader over the first FAT. exFAT volumes can have a large FAT, so
/// only the pages needed while walking a directory are cached.
struct FatTable<'a> {
    src: &'a dyn ReadOnlySource,
    absolute_offset: u64,
    byte_len: u64,
    pages: HashMap<u64, Vec<u8>>,
}

impl<'a> FatTable<'a> {
    fn new(src: &'a dyn ReadOnlySource, volume: &VolumeRef, boot: &ExfatBoot) -> Result<Self> {
        let absolute_offset = volume
            .offset
            .checked_add(boot.fat_offset)
            .ok_or_else(|| Error::Corrupt("exFAT FAT offset overflow".into()))?;
        // One entry per cluster plus the two reserved entries. A FAT shorter
        // than that is truncated; the parser will treat missing entries as
        // unmapped instead of crashing.
        let minimum = (boot.cluster_count as u64 + 2)
            .checked_mul(4)
            .ok_or_else(|| Error::Corrupt("exFAT FAT size overflow".into()))?;
        let byte_len = boot
            .fat_length
            .min(volume.length.saturating_sub(boot.fat_offset));
        if byte_len < minimum {
            return Err(Error::invalid(format!(
                "exFAT FAT has {byte_len} bytes but needs at least {minimum} entries"
            )));
        }
        Ok(Self {
            src,
            absolute_offset,
            byte_len,
            pages: HashMap::new(),
        })
    }

    fn entry(&mut self, cluster: u32) -> Option<u32> {
        let off = (cluster as u64).checked_mul(4)?;
        if off.checked_add(4)? > self.byte_len {
            return None;
        }
        let page_index = off / FAT_PAGE;
        if !self.pages.contains_key(&page_index) {
            if self.pages.len() >= FAT_CACHE_PAGES {
                self.pages.clear();
            }
            let page_off = page_index * FAT_PAGE;
            let len = FAT_PAGE.min(self.byte_len - page_off);
            let bytes = self
                .src
                .read_vec(self.absolute_offset.checked_add(page_off)?, len)
                .ok()?;
            self.pages.insert(page_index, bytes);
        }
        let page = self.pages.get(&page_index)?;
        let at = (off - page_index * FAT_PAGE) as usize;
        u32_le(page, at)
    }

    fn is_eoc(value: u32) -> bool {
        value >= 0xFFFF_FFF8
    }

    fn is_bad(value: u32) -> bool {
        value == 0xFFFF_FFF7
    }
}

#[derive(Debug, Clone)]
struct EntrySet {
    deleted: bool,
    checksum_valid: bool,
    checksum_present: bool,
    name: String,
    name_complete: bool,
    attributes: u16,
    first_cluster: u32,
    data_length: u64,
    valid_data_length: u64,
    no_fat_chain: bool,
    created: Option<String>,
    modified: Option<String>,
    accessed: Option<String>,
    raw_offset: u64,
}

impl EntrySet {
    fn is_dir(&self) -> bool {
        self.attributes & 0x10 != 0
    }
}

fn set_checksum(entries: &[&[u8]]) -> u16 {
    let mut sum = 0u16;
    for (index, entry) in entries.iter().enumerate() {
        for (i, b) in entry.iter().enumerate() {
            if index == 0 && (i == 2 || i == 3) {
                continue;
            }
            sum = sum.rotate_right(1).wrapping_add(*b as u16);
        }
    }
    sum
}

fn exfat_datetime_to_iso(stamp: u32) -> Option<String> {
    if stamp == 0 {
        return None;
    }
    let second = ((stamp & 0x1F) * 2) as u8;
    let minute = ((stamp >> 5) & 0x3F) as u8;
    let hour = ((stamp >> 11) & 0x1F) as u8;
    let day = ((stamp >> 16) & 0x1F) as u8;
    let month = ((stamp >> 21) & 0x0F) as u8;
    let year = 1980 + ((stamp >> 25) & 0x7F) as u16;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

fn parse_name(entries: &[&[u8]], name_length: usize) -> (String, bool) {
    let mut units: Vec<u16> = Vec::with_capacity(name_length);
    let mut complete = true;
    for entry in entries {
        if entry[0] & 0x7F != ENTRY_NAME {
            continue;
        }
        for chunk in entry[2..32].chunks_exact(2) {
            if units.len() >= name_length {
                break;
            }
            units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        if units.len() >= name_length {
            break;
        }
    }
    if units.len() < name_length {
        complete = false;
    }
    let s = String::from_utf16_lossy(&units);
    (s.trim_matches('\u{0}').to_string(), complete)
}

fn parse_entry_set(
    data: &[u8],
    at: usize,
    base_abs: u64,
    warnings: &mut Vec<String>,
) -> Option<EntrySet> {
    let first = data.get(at..at.checked_add(32)?)?;
    let raw_type = first[0];
    if raw_type == 0 {
        return None;
    }
    let deleted = raw_type & 0x80 == 0;
    let kind = raw_type & 0x7F;
    if kind != ENTRY_FILE {
        return None;
    }
    let secondary_count = first[1] as usize;
    if secondary_count == 0 || secondary_count > MAX_SECONDARIES {
        warnings.push(format!(
            "exFAT entry set at {:#x} has an invalid secondary count {secondary_count}",
            base_abs + at as u64
        ));
        return None;
    }
    let total = secondary_count.checked_add(1)?.checked_mul(32)?;
    if at.checked_add(total)? > data.len() {
        warnings.push(format!(
            "exFAT entry set at {:#x} is truncated",
            base_abs + at as u64
        ));
        return None;
    }
    let entries: Vec<&[u8]> = (0..=secondary_count)
        .map(|i| &data[at + i * 32..at + i * 32 + 32])
        .collect();

    let stream_pos = entries.iter().position(|e| e[0] & 0x7F == ENTRY_STREAM);
    let Some(stream_pos) = stream_pos else {
        warnings.push(format!(
            "exFAT file entry at {:#x} has no stream extension",
            base_abs + at as u64
        ));
        return None;
    };
    let stream = entries[stream_pos];
    let name_length = stream[3] as usize;
    if name_length == 0 || name_length > 255 {
        warnings.push(format!(
            "exFAT entry set at {:#x} has an invalid name length {name_length}",
            base_abs + at as u64
        ));
        return None;
    }
    let (name, name_complete) = parse_name(&entries, name_length);
    if name.is_empty() {
        warnings.push(format!(
            "exFAT entry set at {:#x} has no decodable name",
            base_abs + at as u64
        ));
        return None;
    }

    let stored_checksum = u16_le(first, 2).unwrap_or(0);
    let computed_checksum = set_checksum(&entries);
    let checksum_valid = stored_checksum == computed_checksum;

    let created = exfat_datetime_to_iso(u32_le(first, 8).unwrap_or(0));
    let modified = exfat_datetime_to_iso(u32_le(first, 12).unwrap_or(0));
    let accessed = exfat_datetime_to_iso(u32_le(first, 16).unwrap_or(0));

    Some(EntrySet {
        deleted,
        checksum_valid,
        checksum_present: stored_checksum != 0,
        name,
        name_complete,
        attributes: u16_le(first, 4).unwrap_or(0),
        first_cluster: u32_le(stream, 20).unwrap_or(0),
        data_length: u64_le(stream, 24).unwrap_or(0),
        valid_data_length: u64_le(stream, 8).unwrap_or(0),
        no_fat_chain: stream[1] & 0x02 != 0,
        created,
        modified,
        accessed,
        raw_offset: base_abs + at as u64,
    })
}

struct ExfatScan<'a> {
    src: &'a dyn ReadOnlySource,
    volume: &'a VolumeRef,
    boot: ExfatBoot,
    fat: FatTable<'a>,
    options: &'a FsScanOptions,
    sink: &'a mut dyn CandidateSink,
    summary: FsSummary,
    visited_dirs: HashSet<u32>,
}

impl<'a> ExfatScan<'a> {
    fn read_cluster(&self, cluster: u32) -> Result<Vec<u8>> {
        let rel = self
            .boot
            .cluster_offset(cluster)
            .ok_or_else(|| Error::Corrupt(format!("exFAT cluster {cluster} is out of range")))?;
        let abs = self
            .volume
            .offset
            .checked_add(rel)
            .ok_or_else(|| Error::Corrupt("exFAT cluster offset overflow".into()))?;
        let mut buf = vec![0u8; self.boot.cluster_size as usize];
        self.src.read_exact_at(abs, &mut buf)?;
        Ok(buf)
    }

    fn contiguous_clusters(&self, start: u32, length: u64) -> (Vec<u32>, bool) {
        if !(2..=self.boot.cluster_count.saturating_add(1)).contains(&start) {
            return (Vec::new(), false);
        }
        let needed = length.div_ceil(self.boot.cluster_size);
        let available = (self.boot.cluster_count as u64 + 1).saturating_sub(start as u64);
        let take = needed.min(available);
        let mut out = Vec::with_capacity(take.min(1_000_000) as usize);
        for i in 0..take {
            out.push(start + i as u32);
        }
        (out, take >= needed)
    }

    fn chain_clusters(&mut self, start: u32, length: u64) -> (Vec<u32>, bool) {
        if !(2..=self.boot.cluster_count.saturating_add(1)).contains(&start) {
            return (Vec::new(), false);
        }
        let needed = length.div_ceil(self.boot.cluster_size);
        let max = needed.min(self.boot.cluster_count as u64);
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut cur = start;
        let mut exact = true;
        while (out.len() as u64) < max {
            if !(2..=self.boot.cluster_count.saturating_add(1)).contains(&cur) || !seen.insert(cur)
            {
                exact = false;
                break;
            }
            out.push(cur);
            let Some(next) = self.fat.entry(cur) else {
                exact = false;
                break;
            };
            if FatTable::is_eoc(next) {
                break;
            }
            if next == 0 || FatTable::is_bad(next) {
                exact = false;
                break;
            }
            cur = next;
        }
        let enough = exact && out.len() as u64 >= needed;
        (out, enough)
    }

    fn read_dir_data(&mut self, set: &EntrySet, is_root: bool) -> Result<(Vec<u8>, u64)> {
        let start = if is_root {
            self.boot.root_cluster
        } else {
            set.first_cluster
        };
        if start < 2 || start > self.boot.cluster_count.saturating_add(1) {
            return Err(Error::Corrupt(format!(
                "exFAT directory first cluster {start} is out of range"
            )));
        }
        let limit = if is_root {
            MAX_DIR_BYTES
        } else {
            set.data_length
                .max(self.boot.cluster_size)
                .min(MAX_DIR_BYTES)
        };
        let (clusters, _) = if set.no_fat_chain && !is_root {
            self.contiguous_clusters(start, limit)
        } else {
            self.chain_clusters(start, limit)
        };
        if clusters.is_empty() {
            return Err(Error::Corrupt(
                "exFAT directory has no readable clusters".into(),
            ));
        }
        let mut out = Vec::new();
        for c in clusters {
            if out.len() as u64 >= limit {
                break;
            }
            let cluster = self.read_cluster(c)?;
            let take = (limit - out.len() as u64).min(cluster.len() as u64) as usize;
            out.extend_from_slice(&cluster[..take]);
            if out.len() as u64 >= limit {
                break;
            }
        }
        let base = self
            .volume
            .offset
            .saturating_add(self.boot.cluster_offset(start).unwrap_or(0));
        Ok((out, base))
    }

    fn scan_root(&mut self) -> Result<()> {
        let root = EntrySet {
            deleted: false,
            checksum_valid: true,
            checksum_present: false,
            name: String::new(),
            name_complete: true,
            attributes: 0x10,
            first_cluster: self.boot.root_cluster,
            data_length: 0,
            valid_data_length: 0,
            no_fat_chain: false,
            created: None,
            modified: None,
            accessed: None,
            raw_offset: 0,
        };
        let (data, base) = self.read_dir_data(&root, true)?;
        self.scan_dir_data(&data, base, "", 0)
    }

    fn scan_dir_data(&mut self, data: &[u8], base: u64, path: &str, depth: u32) -> Result<()> {
        let mut at = 0usize;
        while at + 32 <= data.len() {
            if self.options.is_cancelled() {
                return Ok(());
            }
            if data[at] == 0 {
                break;
            }
            let kind = data[at] & 0x7F;
            if kind == ENTRY_LABEL {
                if data[at] & 0x80 != 0 && self.summary.label.is_none() {
                    let label = utf16le_to_string(&data[at + 2..at + 32]);
                    if !label.is_empty() {
                        self.summary.label = Some(label);
                    }
                }
                at += 32;
                continue;
            }
            if kind != ENTRY_FILE {
                at += 32;
                continue;
            }
            let Some(set) = parse_entry_set(data, at, base, &mut self.summary.warnings) else {
                at += 32;
                continue;
            };
            let total = (data[at + 1] as usize + 1) * 32;
            at = at.saturating_add(total);

            if set.deleted {
                self.summary.deleted_entries += 1;
            } else {
                self.summary.live_entries += 1;
            }
            if set.is_dir() {
                self.scan_subdirectory(&set, path, depth);
                continue;
            }
            if !set.deleted && !self.options.include_live {
                continue;
            }
            if self.summary.emitted as usize >= self.options.max_candidates {
                self.summary.truncated = true;
                return Ok(());
            }
            let candidate = self.make_candidate(&set, path);
            self.sink.emit(candidate)?;
            self.summary.emitted += 1;
        }
        Ok(())
    }

    fn scan_subdirectory(&mut self, set: &EntrySet, parent: &str, depth: u32) {
        if depth >= MAX_DEPTH {
            self.summary
                .warnings
                .push(format!("exFAT directory depth limit reached at {parent}"));
            return;
        }
        if !self.visited_dirs.insert(set.first_cluster) {
            self.summary.warnings.push(format!(
                "exFAT directory loop or duplicate cluster {} at {parent}",
                set.first_cluster
            ));
            return;
        }
        let sub_path = if parent.is_empty() {
            set.name.clone()
        } else {
            format!("{parent}/{}", set.name)
        };
        match self.read_dir_data(set, false) {
            Ok((data, base)) => {
                if let Err(e) = self.scan_dir_data(&data, base, &sub_path, depth + 1) {
                    self.summary
                        .warnings
                        .push(format!("could not scan exFAT directory {sub_path}: {e}"));
                }
            }
            Err(e) => self
                .summary
                .warnings
                .push(format!("could not read exFAT directory {sub_path}: {e}")),
        }
    }

    fn make_candidate(&mut self, set: &EntrySet, parent: &str) -> Candidate {
        let mut reasons = if set.deleted {
            vec!["deleted exFAT directory entry set".to_string()]
        } else {
            vec!["live exFAT directory entry set (forensic listing)".to_string()]
        };
        let mut label = if set.deleted {
            ConfidenceLabel::Likely
        } else {
            ConfidenceLabel::Verified
        };
        let mut score: u8 = if set.deleted { 82 } else { 95 };
        let mut complete = true;
        let mut data = DataLocation::None;

        if !set.name_complete {
            label = ConfidenceLabel::Partial;
            score = score.min(45);
            reasons.push("one or more file-name entries are missing or truncated".into());
        }
        if set.checksum_valid {
            reasons.push("entry-set checksum matches".into());
            score = score.saturating_add(2);
        } else if !set.deleted && set.checksum_present {
            label = ConfidenceLabel::Partial;
            score = score.min(50);
            reasons.push("entry-set checksum does not match".into());
        } else if set.deleted && set.checksum_present {
            reasons
                .push("entry-set checksum cannot be checked after the delete bit was set".into());
        }

        if set.data_length == 0 {
            data = DataLocation::resident(Vec::new());
            complete = true;
            label = ConfidenceLabel::Verified;
            score = score.max(92);
            reasons.push("zero-length file has no data clusters".into());
        } else if set.first_cluster < 2
            || set.first_cluster > self.boot.cluster_count.saturating_add(1)
        {
            complete = false;
            label = ConfidenceLabel::Unverified;
            score = 20;
            reasons.push("first cluster is unavailable; content cannot be located".into());
        } else {
            let (mut clusters, mut exact) = if set.no_fat_chain {
                self.contiguous_clusters(set.first_cluster, set.data_length)
            } else {
                let (chain, exact) = self.chain_clusters(set.first_cluster, set.data_length);
                if !exact && set.deleted {
                    let (contig, enough) =
                        self.contiguous_clusters(set.first_cluster, set.data_length);
                    if !contig.is_empty() {
                        reasons.push(
                            "FAT chain is cleared or incomplete; contiguous allocation was assumed"
                                .into(),
                        );
                        label = ConfidenceLabel::Partial;
                        score = score.min(58);
                        (contig, enough)
                    } else {
                        (chain, exact)
                    }
                } else {
                    (chain, exact)
                }
            };
            if set.no_fat_chain && set.deleted {
                reasons.push("exFAT NoFatChain flag marks the file as contiguous".into());
            }
            if (clusters.len() as u64) < set.data_length.div_ceil(self.boot.cluster_size) {
                complete = false;
                if label != ConfidenceLabel::Unverified {
                    label = ConfidenceLabel::Partial;
                    score = score.min(55);
                }
                reasons.push("cluster run is shorter than the recorded data length".into());
            }
            if clusters.is_empty() {
                data = DataLocation::None;
                complete = false;
                label = ConfidenceLabel::Unverified;
                score = 20;
                reasons.push("no data clusters could be reconstructed".into());
            } else {
                let mut extents = Vec::with_capacity(clusters.len());
                for c in &clusters {
                    if let Some(rel) = self.boot.cluster_offset(*c) {
                        if let Some(abs) = self.volume.offset.checked_add(rel) {
                            extents.push(Extent::new(abs, self.boot.cluster_size));
                        }
                    }
                }
                data = DataLocation::extents(coalesce_extents(extents));
                if set.valid_data_length < set.data_length {
                    reasons.push(format!(
                        "valid data length {} is shorter than data length {}; trailing bytes may be stale",
                        set.valid_data_length, set.data_length
                    ));
                }
            }
            if exact {
                reasons.push("cluster run covers the recorded data length".into());
            }
        }

        let format = format_from_name(&set.name);
        let mut candidate = Candidate::new(
            set.name.clone(),
            set.data_length,
            format,
            data,
            RecoveryMethod::ExfatDirectory,
            Confidence::new(label, score, reasons),
            "exfat",
        );
        candidate.path = parent.to_string();
        candidate.state = if set.deleted {
            EntryState::Deleted
        } else {
            EntryState::Live
        };
        candidate.complete = complete;
        candidate.volume_offset = self.volume.offset;
        candidate.record_id = Some(format!("exfat-entry@{}", set.raw_offset));
        candidate.created = set.created.clone();
        candidate.modified = set.modified.clone();
        candidate.accessed = set.accessed.clone();
        candidate.validation.push(if set.checksum_valid {
            "entry-set checksum valid".into()
        } else {
            "entry-set checksum not usable".into()
        });
        candidate
    }
}

/// Identify an exFAT volume.
pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    let mut boot = ExfatBoot::parse(src, volume).ok()?;
    if let Ok(label) = read_volume_label(src, volume, &boot) {
        boot.label = label;
    }
    Some(boot.info())
}

fn read_volume_label(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    boot: &ExfatBoot,
) -> Result<Option<String>> {
    let mut fat = FatTable::new(src, volume, boot)?;
    let mut visited = HashSet::new();
    let mut cur = boot.root_cluster;
    let mut remaining = MAX_DIR_BYTES;
    let mut data = Vec::new();
    while remaining > 0 {
        if !visited.insert(cur) {
            break;
        }
        let rel = boot
            .cluster_offset(cur)
            .ok_or_else(|| Error::Corrupt("exFAT root cluster is out of range".into()))?;
        let abs = volume
            .offset
            .checked_add(rel)
            .ok_or_else(|| Error::Corrupt("exFAT root offset overflow".into()))?;
        let n = boot.cluster_size.min(remaining) as usize;
        let mut cluster = vec![0u8; n];
        src.read_exact_at(abs, &mut cluster)?;
        data.extend_from_slice(&cluster);
        remaining -= n as u64;
        let Some(next) = fat.entry(cur) else {
            break;
        };
        if FatTable::is_eoc(next) || next == 0 || FatTable::is_bad(next) {
            break;
        }
        if next == cur {
            break;
        }
        cur = next;
    }
    let mut at = 0usize;
    while at + 32 <= data.len() {
        if data[at] == 0 {
            break;
        }
        if data[at] & 0x7F == ENTRY_LABEL {
            if data[at] & 0x80 != 0 {
                let label = utf16le_to_string(&data[at + 2..at + 32]);
                if !label.is_empty() {
                    return Ok(Some(label));
                }
            }
            at += 32;
            continue;
        }
        if data[at] & 0x7F == ENTRY_FILE {
            let count = (data[at + 1] as usize + 1) * 32;
            at = at.saturating_add(count.max(32));
            continue;
        }
        at += 32;
    }
    Ok(None)
}

/// Recovery entry point.
pub fn scan(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    options: &FsScanOptions,
    sink: &mut dyn CandidateSink,
) -> Result<FsSummary> {
    let mut boot = ExfatBoot::parse(src, volume)?;
    if let Ok(label) = read_volume_label(src, volume, &boot) {
        boot.label = label;
    }
    let summary = FsSummary::new(FsKind::ExFat, boot.label.clone());
    let fat = FatTable::new(src, volume, &boot)?;
    let mut scanner = ExfatScan {
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
    fn timestamp_decoding_rejects_invalid_parts() {
        // 2024-01-02 03:04:06
        let stamp = ((2024 - 1980) << 25) | (1 << 21) | (2 << 16) | (3 << 11) | (4 << 5) | 3;
        assert_eq!(
            exfat_datetime_to_iso(stamp).as_deref(),
            Some("2024-01-02T03:04:06Z")
        );
        assert_eq!(exfat_datetime_to_iso(0), None);
        assert_eq!(exfat_datetime_to_iso(u32::MAX), None);
    }

    #[test]
    fn set_checksum_skips_the_checksum_field() {
        let mut first = [0u8; 32];
        first[0] = ENTRY_FILE;
        first[1] = 1;
        let mut stream = [0u8; 32];
        stream[0] = ENTRY_STREAM;
        let reference = set_checksum(&[&first, &stream]);
        first[2] = 0xAA;
        first[3] = 0xBB;
        assert_eq!(set_checksum(&[&first, &stream]), reference);
    }
}
