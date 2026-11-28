//! NTFS Master File Table recovery.
//!
//! Every NTFS file, directory and metadata object is described by a 1024-byte
//! (typically) `FILE` record in the Master File Table. When a file is deleted,
//! Windows clears the in-use flag in the record header and frees the clusters,
//! but the `$FILE_NAME`, `$STANDARD_INFORMATION` and `$DATA` attributes usually
//! remain until the record is reused. Relift reads the MFT runlist from record
//! 0, applies the update-sequence fixup to each record, reconstructs resident
//! data or non-resident data runs, and resolves parent directory paths.
//!
//! Deliberately not implemented here: `$ATTRIBUTE_LIST` continuation records,
//! NTFS compression, EFS encryption, `$UsnJrnl` replay, and named alternate
//! data streams. Those cases are reported as unsupported instead of being
//! emitted as if the bytes were complete.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::candidate::{
    format_from_name, Candidate, Confidence, ConfidenceLabel, DataLocation, EntryState,
    RecoveryMethod, Segment,
};
use crate::error::{Error, Result};
use crate::fs::{
    read_at_volume, CandidateSink, FsInfo, FsKind, FsScanOptions, FsSummary, VolumeRef,
};
use crate::source::ReadOnlySource;
use crate::util::{filetime_to_iso, u16_le, u32_le, u64_le, u8_at, utf16le_to_string};

const MAX_MFT_RECORDS: u64 = 1_000_000;
const MAX_RUNS: usize = 1_000_000;
const MAX_NAME_UNITS: usize = 255;

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_VOLUME_NAME: u32 = 0x60;
const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;

const ROOT_RECORD: u64 = 5;

#[derive(Debug, Clone)]
struct NtfsBoot {
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    cluster_size: u64,
    total_sectors: u64,
    cluster_count: u64,
    mft_lcn: u64,
    mft_mirror_lcn: u64,
    record_size: u64,
    index_record_size: u64,
    serial: u64,
    warnings: Vec<String>,
}

impl NtfsBoot {
    fn parse(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Result<Self> {
        if volume.length < 512 {
            return Err(Error::invalid("volume too small for an NTFS boot sector"));
        }
        let bs = read_at_volume(src, volume, 0, 512)?;
        if bs.len() < 512 {
            return Err(Error::invalid("truncated NTFS boot sector"));
        }
        if bs.get(3..11) != Some(b"NTFS    ") {
            return Err(Error::invalid("missing NTFS OEM signature"));
        }
        let mut warnings = Vec::new();
        let bytes_per_sector = u16_le(&bs, 11).unwrap_or(0) as u32;
        let sectors_per_cluster = u8_at(&bs, 13).unwrap_or(0) as u32;
        if !(512..=4096).contains(&bytes_per_sector)
            || !bytes_per_sector.is_power_of_two()
            || sectors_per_cluster == 0
            || sectors_per_cluster > 128
            || !sectors_per_cluster.is_power_of_two()
        {
            return Err(Error::invalid(
                "NTFS BPB has invalid sector or cluster size",
            ));
        }
        let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
        if cluster_size > 2 * 1024 * 1024 {
            return Err(Error::invalid("NTFS cluster size is unreasonably large"));
        }
        let total_sectors = u64_le(&bs, 40).unwrap_or(0);
        if total_sectors == 0 {
            return Err(Error::invalid("NTFS BPB declares zero sectors"));
        }
        let volume_bytes = total_sectors
            .checked_mul(bytes_per_sector as u64)
            .ok_or_else(|| Error::Corrupt("NTFS volume size overflow".into()))?;
        if volume_bytes > volume.length {
            warnings.push(format!(
                "NTFS boot sector describes {volume_bytes} bytes but the volume is {} bytes; using the smaller bound",
                volume.length
            ));
        }
        let mft_lcn = u64_le(&bs, 48).unwrap_or(0);
        let mft_mirror_lcn = u64_le(&bs, 56).unwrap_or(0);
        let record_clusters = u8_at(&bs, 64).unwrap_or(0) as i8;
        let index_clusters = u8_at(&bs, 68).unwrap_or(0) as i8;
        let record_size = record_size_from(record_clusters, cluster_size)?;
        let index_record_size = record_size_from(index_clusters, cluster_size)?;
        if !(512..=64 * 1024).contains(&record_size) || !record_size.is_power_of_two() {
            return Err(Error::invalid("NTFS MFT record size is out of range"));
        }
        let cluster_count = volume_bytes / cluster_size;
        if mft_lcn >= cluster_count {
            return Err(Error::invalid("NTFS MFT LCN is outside the volume"));
        }
        if mft_mirror_lcn >= cluster_count {
            warnings.push("NTFS MFT mirror LCN is outside the volume".into());
        }
        let serial = u64_le(&bs, 72).unwrap_or(0);
        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            cluster_size,
            total_sectors,
            cluster_count,
            mft_lcn,
            mft_mirror_lcn,
            record_size,
            index_record_size,
            serial,
            warnings,
        })
    }

    fn cluster_offset(&self, lcn: u64) -> Option<u64> {
        lcn.checked_mul(self.cluster_size)
    }

    fn info(&self) -> FsInfo {
        let mut details = BTreeMap::new();
        details.insert("bytes_per_sector".into(), self.bytes_per_sector.to_string());
        details.insert(
            "sectors_per_cluster".into(),
            self.sectors_per_cluster.to_string(),
        );
        details.insert("cluster_size".into(), self.cluster_size.to_string());
        details.insert("mft_record_size".into(), self.record_size.to_string());
        details.insert("mft_lcn".into(), self.mft_lcn.to_string());
        details.insert("total_sectors".into(), self.total_sectors.to_string());
        details.insert("serial".into(), format!("{:016X}", self.serial));
        FsInfo {
            kind: FsKind::Ntfs,
            label: None,
            cluster_size: Some(self.cluster_size),
            details,
            warnings: self.warnings.clone(),
        }
    }
}

fn record_size_from(clusters: i8, cluster_size: u64) -> Result<u64> {
    let size = if clusters < 0 {
        let shift = (-(clusters as i32)) as u32;
        if shift == 0 || shift > 24 {
            return Err(Error::invalid("NTFS record-size shift is out of range"));
        }
        1u64 << shift
    } else if clusters > 0 {
        (clusters as u64)
            .checked_mul(cluster_size)
            .ok_or_else(|| Error::Corrupt("NTFS record size overflow".into()))?
    } else {
        return Err(Error::invalid("NTFS record size field is zero"));
    };
    Ok(size)
}

#[derive(Debug, Clone)]
struct NtfsName {
    name: String,
    parent: u64,
    parent_sequence: u16,
    namespace: u8,
    created: Option<String>,
    modified: Option<String>,
    accessed: Option<String>,
    real_size: u64,
    allocated_size: u64,
    flags: u32,
}

impl NtfsName {
    fn priority(&self) -> u8 {
        match self.namespace {
            3 => 4, // Win32 + DOS
            1 => 3, // Win32
            0 => 2, // POSIX
            2 => 1, // DOS-only alias
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
struct Run {
    vcn: u64,
    clusters: u64,
    lcn: Option<u64>, // None = sparse
}

#[derive(Debug, Clone)]
struct NtfsData {
    resident: bool,
    resident_bytes: Vec<u8>,
    real_size: u64,
    allocated_size: u64,
    initialized_size: u64,
    runs: Vec<Run>,
    compressed: bool,
    encrypted: bool,
    sparse: bool,
    named: bool,
}

impl NtfsData {
    fn empty() -> Self {
        Self {
            resident: true,
            resident_bytes: Vec::new(),
            real_size: 0,
            allocated_size: 0,
            initialized_size: 0,
            runs: Vec::new(),
            compressed: false,
            encrypted: false,
            sparse: false,
            named: false,
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedRecord {
    number: u64,
    sequence: u16,
    in_use: bool,
    is_dir: bool,
    base_record: u64,
    fixup_ok: bool,
    names: Vec<NtfsName>,
    data: Option<NtfsData>,
    standard_created: Option<String>,
    standard_modified: Option<String>,
    standard_accessed: Option<String>,
    volume_name: Option<String>,
    warnings: Vec<String>,
}

impl ParsedRecord {
    fn primary_name(&self) -> Option<&NtfsName> {
        self.names.iter().max_by_key(|n| n.priority())
    }

    fn best_times(&self) -> (Option<String>, Option<String>, Option<String>) {
        if self.standard_created.is_some()
            || self.standard_modified.is_some()
            || self.standard_accessed.is_some()
        {
            return (
                self.standard_created.clone(),
                self.standard_modified.clone(),
                self.standard_accessed.clone(),
            );
        }
        match self.primary_name() {
            Some(n) => (n.created.clone(), n.modified.clone(), n.accessed.clone()),
            None => (None, None, None),
        }
    }
}

fn apply_fixup(record: &mut [u8], sector_size: usize) -> bool {
    let Some(usa_off) = u16_le(record, 4).map(usize::from) else {
        return false;
    };
    let Some(usa_count) = u16_le(record, 6).map(usize::from) else {
        return false;
    };
    if usa_count < 2 || usa_off + usa_count * 2 > record.len() {
        return false;
    }
    let expected = record.len() / sector_size + 1;
    if usa_count != expected {
        return false;
    }
    let Some(usn) = u16_le(record, usa_off) else {
        return false;
    };
    for i in 1..usa_count {
        let pos = i * sector_size - 2;
        let Some(found) = u16_le(record, pos) else {
            return false;
        };
        if found != usn {
            return false;
        }
        let replacement = u16_le(record, usa_off + i * 2).unwrap_or(0);
        record[pos..pos + 2].copy_from_slice(&replacement.to_le_bytes());
    }
    true
}

fn parse_record(
    mut record: Vec<u8>,
    number: u64,
    boot: &NtfsBoot,
    volume: &VolumeRef,
) -> Option<ParsedRecord> {
    let sector_size = boot.bytes_per_sector as usize;
    if record.len() < 48 || record.get(0..4) != Some(b"FILE") {
        return None;
    }
    let fixup_ok = apply_fixup(&mut record, sector_size);
    let mut warnings = Vec::new();
    if !fixup_ok {
        warnings.push("update-sequence fixup did not validate".to_string());
        return Some(ParsedRecord {
            number,
            sequence: u16_le(&record, 16).unwrap_or(0),
            in_use: u16_le(&record, 22).unwrap_or(0) & 1 != 0,
            is_dir: u16_le(&record, 22).unwrap_or(0) & 2 != 0,
            base_record: u64_le(&record, 32).unwrap_or(0) & 0x0000_FFFF_FFFF_FFFF,
            fixup_ok: false,
            names: Vec::new(),
            data: None,
            standard_created: None,
            standard_modified: None,
            standard_accessed: None,
            volume_name: None,
            warnings,
        });
    }

    let flags = u16_le(&record, 22).unwrap_or(0);
    let base_record = u64_le(&record, 32).unwrap_or(0) & 0x0000_FFFF_FFFF_FFFF;
    let attrs_offset = u16_le(&record, 20).unwrap_or(0) as usize;
    let used_size = (u32_le(&record, 24).unwrap_or(0) as usize).min(record.len());
    if attrs_offset < 48 || attrs_offset >= used_size {
        warnings.push("attribute offset is outside the used record size".into());
        return Some(ParsedRecord {
            number,
            sequence: u16_le(&record, 16).unwrap_or(0),
            in_use: flags & 1 != 0,
            is_dir: flags & 2 != 0,
            base_record,
            fixup_ok: true,
            names: Vec::new(),
            data: None,
            standard_created: None,
            standard_modified: None,
            standard_accessed: None,
            volume_name: None,
            warnings,
        });
    }

    let mut names = Vec::new();
    let mut data: Option<NtfsData> = None;
    let mut volume_name = None;
    let mut standard_created = None;
    let mut standard_modified = None;
    let mut standard_accessed = None;

    let mut at = attrs_offset;
    let mut steps = 0usize;
    while at + 4 <= used_size && steps < 4096 {
        steps += 1;
        let attr_type = u32_le(&record, at).unwrap_or(ATTR_END);
        if attr_type == ATTR_END || attr_type == 0 {
            break;
        }
        let attr_len = u32_le(&record, at + 4).unwrap_or(0) as usize;
        if attr_len < 24
            || at
                .checked_add(attr_len)
                .map(|e| e > used_size)
                .unwrap_or(true)
        {
            warnings.push(format!(
                "attribute at record offset {at:#x} has an invalid length {attr_len}"
            ));
            break;
        }
        let attr = &record[at..at + attr_len];
        let non_resident = u8_at(attr, 8).unwrap_or(0) != 0;
        let name_len = u8_at(attr, 9).unwrap_or(0) as usize;
        let name_off = u16_le(attr, 10).unwrap_or(0) as usize;
        let attr_name = if name_len > 0 && name_off + name_len * 2 <= attr.len() {
            utf16le_to_string(&attr[name_off..name_off + name_len * 2])
        } else {
            String::new()
        };

        match attr_type {
            ATTR_STANDARD_INFORMATION if !non_resident => {
                if let Some(value) = resident_value(attr, &record) {
                    if value.len() >= 32 {
                        standard_created = filetime_to_iso(u64_le(value, 0).unwrap_or(0));
                        standard_modified = filetime_to_iso(u64_le(value, 8).unwrap_or(0));
                        standard_accessed = filetime_to_iso(u64_le(value, 24).unwrap_or(0));
                    }
                }
            }
            ATTR_FILE_NAME if !non_resident => {
                if let Some(value) = resident_value(attr, &record) {
                    if let Some(name) = parse_file_name(value) {
                        names.push(name);
                    } else {
                        warnings.push("$FILE_NAME attribute is malformed".into());
                    }
                }
            }
            ATTR_VOLUME_NAME if !non_resident => {
                if let Some(value) = resident_value(attr, &record) {
                    let name = utf16le_to_string(value);
                    if !name.is_empty() {
                        volume_name = Some(name);
                    }
                }
            }
            ATTR_DATA => {
                let candidate = if non_resident {
                    parse_nonresident_data(attr, boot, volume, number)
                } else {
                    resident_value(attr, &record).map(|value| {
                        let mut d = NtfsData::empty();
                        d.resident_bytes = value.to_vec();
                        d.real_size = value.len() as u64;
                        d.allocated_size = value.len() as u64;
                        d.initialized_size = value.len() as u64;
                        d.named = !attr_name.is_empty();
                        d
                    })
                };
                match candidate {
                    Some(mut d) => {
                        if !attr_name.is_empty() {
                            d.named = true;
                        }
                        if data.is_none() || !d.named {
                            data = Some(d);
                        }
                    }
                    None => warnings.push("$DATA attribute could not be parsed".into()),
                }
            }
            _ => {}
        }
        let aligned = (attr_len + 7) & !7;
        if aligned == 0 {
            break;
        }
        at = match at.checked_add(aligned) {
            Some(v) => v,
            None => break,
        };
    }

    Some(ParsedRecord {
        number,
        sequence: u16_le(&record, 16).unwrap_or(0),
        in_use: flags & 1 != 0,
        is_dir: flags & 2 != 0,
        base_record,
        fixup_ok: true,
        names,
        data,
        standard_created,
        standard_modified,
        standard_accessed,
        volume_name,
        warnings,
    })
}

/// Resident attribute values are stored inside the attribute record; their
/// value offset is relative to the start of the attribute, not the MFT record.
fn resident_value<'a>(attr: &'a [u8], _record: &'a [u8]) -> Option<&'a [u8]> {
    if u8_at(attr, 8).unwrap_or(1) != 0 {
        return None;
    }
    let value_len = u32_le(attr, 16)? as usize;
    let value_off = u16_le(attr, 20)? as usize;
    let end = value_off.checked_add(value_len)?;
    if value_off < 24 || end > attr.len() || value_len > 1024 * 1024 {
        return None;
    }
    attr.get(value_off..end)
}

fn parse_file_name(value: &[u8]) -> Option<NtfsName> {
    if value.len() < 66 {
        return None;
    }
    let parent_ref = u64_le(value, 0)?;
    let parent = parent_ref & 0x0000_FFFF_FFFF_FFFF;
    let parent_sequence = (parent_ref >> 48) as u16;
    let name_len = u8_at(value, 64)? as usize;
    if name_len == 0 || name_len > MAX_NAME_UNITS {
        return None;
    }
    let end = 66usize.checked_add(name_len * 2)?;
    if end > value.len() {
        return None;
    }
    let name = utf16le_to_string(&value[66..end]);
    if name.is_empty() {
        return None;
    }
    Some(NtfsName {
        name,
        parent,
        parent_sequence,
        namespace: u8_at(value, 65).unwrap_or(0),
        created: filetime_to_iso(u64_le(value, 8).unwrap_or(0)),
        modified: filetime_to_iso(u64_le(value, 16).unwrap_or(0)),
        accessed: filetime_to_iso(u64_le(value, 32).unwrap_or(0)),
        real_size: u64_le(value, 48).unwrap_or(0),
        allocated_size: u64_le(value, 40).unwrap_or(0),
        flags: u32_le(value, 56).unwrap_or(0),
    })
}

fn parse_nonresident_data(
    attr: &[u8],
    boot: &NtfsBoot,
    volume: &VolumeRef,
    _record_number: u64,
) -> Option<NtfsData> {
    if attr.len() < 64 {
        return None;
    }
    let flags = u16_le(attr, 12).unwrap_or(0);
    let lowest_vcn = u64_le(attr, 16).unwrap_or(0);
    let mapping_off = u16_le(attr, 32).unwrap_or(0) as usize;
    let allocated_size = u64_le(attr, 40).unwrap_or(0);
    let real_size = u64_le(attr, 48).unwrap_or(0);
    let initialized_size = u64_le(attr, 56).unwrap_or(0);
    if mapping_off < 64 || mapping_off >= attr.len() {
        return None;
    }
    let mut runs = Vec::new();
    let mut at = mapping_off;
    let mut vcn = lowest_vcn;
    let mut lcn: i64 = 0;
    while at < attr.len() && runs.len() < MAX_RUNS {
        let header = attr[at];
        if header == 0 {
            break;
        }
        let len_bytes = (header & 0x0F) as usize;
        let off_bytes = (header >> 4) as usize;
        at += 1;
        if len_bytes == 0
            || len_bytes > 8
            || off_bytes > 8
            || at + len_bytes + off_bytes > attr.len()
        {
            return None;
        }
        let cluster_count = read_unsigned(&attr[at..at + len_bytes]);
        at += len_bytes;
        if cluster_count == 0 {
            return None;
        }
        let sparse = off_bytes == 0;
        let lcn_value = if sparse {
            None
        } else {
            let delta = read_signed(&attr[at..at + off_bytes]);
            at += off_bytes;
            let next = lcn.checked_add(delta)?;
            if next < 0 {
                return None;
            }
            lcn = next;
            Some(lcn as u64)
        };
        if let Some(start) = lcn_value {
            let end = start.checked_add(cluster_count)?;
            if end > boot.cluster_count {
                return None;
            }
            boot.cluster_offset(start)
                .map(|v| v.saturating_add(volume.offset))?;
        }
        runs.push(Run {
            vcn,
            clusters: cluster_count,
            lcn: lcn_value,
        });
        vcn = vcn.checked_add(cluster_count)?;
        if at >= attr.len() {
            break;
        }
    }
    // A continuation record carries lowest_vcn > 0. It is attached by
    // $ATTRIBUTE_LIST, which this build does not follow.
    if lowest_vcn > 0 {
        return None;
    }
    Some(NtfsData {
        resident: false,
        resident_bytes: Vec::new(),
        real_size,
        allocated_size,
        initialized_size,
        runs,
        compressed: flags & 0x0001 != 0,
        encrypted: flags & 0x4000 != 0,
        sparse: flags & 0x8000 != 0,
        named: false,
    })
}

fn read_unsigned(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for (i, b) in bytes.iter().enumerate().take(8) {
        value |= (*b as u64) << (i * 8);
    }
    value
}

fn read_signed(bytes: &[u8]) -> i64 {
    let mut raw = 0u64;
    for (i, b) in bytes.iter().enumerate().take(8) {
        raw |= (*b as u64) << (i * 8);
    }
    let shift = 64 - 8 * bytes.len().min(8) as u32;
    ((raw << shift) as i64) >> shift
}

struct MftReader<'a> {
    src: &'a dyn ReadOnlySource,
    segments: Vec<Segment>,
    size: u64,
    record_size: u64,
    resident: Option<Vec<u8>>,
}

impl<'a> MftReader<'a> {
    fn open(src: &'a dyn ReadOnlySource, volume: &VolumeRef, boot: &NtfsBoot) -> Result<Self> {
        let abs = volume
            .offset
            .checked_add(
                boot.cluster_offset(boot.mft_lcn)
                    .ok_or_else(|| Error::Corrupt("NTFS MFT offset overflow".into()))?,
            )
            .ok_or_else(|| Error::Corrupt("NTFS MFT absolute offset overflow".into()))?;
        let record_size = boot.record_size as usize;
        if record_size > crate::source::MAX_BUFFERED_READ as usize {
            return Err(Error::unsupported("NTFS MFT record is too large"));
        }
        let mut record0 = vec![0u8; record_size];
        src.read_exact_at(abs, &mut record0)?;
        let parsed = parse_record(record0, 0, boot, volume)
            .ok_or_else(|| Error::Corrupt("NTFS MFT record 0 is not a FILE record".into()))?;
        if !parsed.fixup_ok {
            return Err(Error::Corrupt(
                "NTFS MFT record 0 failed the fixup check".into(),
            ));
        }
        let data = parsed
            .data
            .ok_or_else(|| Error::Corrupt("NTFS MFT record 0 has no $DATA attribute".into()))?;
        if data.encrypted || data.compressed {
            return Err(Error::unsupported(
                "NTFS MFT is compressed or encrypted; this build cannot read it",
            ));
        }
        let mut segments = Vec::new();
        let mut size = data.real_size;
        let mut resident = None;
        if data.resident {
            size = data.resident_bytes.len() as u64;
            resident = Some(data.resident_bytes.clone());
        } else {
            let mut logical = 0u64;
            let mut runs = data.runs.clone();
            runs.sort_by_key(|r| r.vcn);
            for run in &runs {
                let run_vcn_bytes = run
                    .vcn
                    .checked_mul(boot.cluster_size)
                    .ok_or_else(|| Error::Corrupt("NTFS MFT VCN overflow".into()))?;
                if run_vcn_bytes > logical {
                    let gap = run_vcn_bytes - logical;
                    segments.push(Segment::zero(gap));
                    logical = run_vcn_bytes;
                }
                let len = run
                    .clusters
                    .checked_mul(boot.cluster_size)
                    .ok_or_else(|| Error::Corrupt("NTFS MFT run length overflow".into()))?;
                match run.lcn {
                    Some(lcn) => {
                        let rel = boot
                            .cluster_offset(lcn)
                            .ok_or_else(|| Error::Corrupt("NTFS MFT run LCN overflow".into()))?;
                        let absolute = volume
                            .offset
                            .checked_add(rel)
                            .ok_or_else(|| Error::Corrupt("NTFS MFT run offset overflow".into()))?;
                        segments.push(Segment::data(absolute, len));
                    }
                    None => segments.push(Segment::zero(len)),
                }
                logical = logical.saturating_add(len);
            }
            if size == 0 {
                size = logical;
            }
        }
        if size == 0 {
            return Err(Error::invalid("NTFS MFT is empty"));
        }
        Ok(Self {
            src,
            segments,
            size,
            record_size: boot.record_size,
            resident,
        })
    }

    fn record_count(&self) -> u64 {
        self.size / self.record_size
    }

    fn read_record(&self, index: u64) -> Result<Vec<u8>> {
        let offset = index
            .checked_mul(self.record_size)
            .ok_or_else(|| Error::Corrupt("NTFS MFT record offset overflow".into()))?;
        if offset
            .checked_add(self.record_size)
            .map(|e| e > self.size)
            .unwrap_or(true)
        {
            return Err(Error::Corrupt(
                "NTFS MFT record index is out of range".into(),
            ));
        }
        if let Some(bytes) = &self.resident {
            let start = offset as usize;
            let end = start + self.record_size as usize;
            return Ok(bytes[start..end].to_vec());
        }
        read_segments(self.src, &self.segments, offset, self.record_size)
    }
}

fn read_segments(
    src: &dyn ReadOnlySource,
    segments: &[Segment],
    offset: u64,
    len: u64,
) -> Result<Vec<u8>> {
    if len > crate::source::MAX_BUFFERED_READ {
        return Err(Error::unsupported("requested stream read is too large"));
    }
    let mut out = vec![0u8; len as usize];
    let mut wanted = offset;
    let mut logical = 0u64;
    let mut filled = 0usize;
    for segment in segments {
        let seg_len = segment.length();
        let seg_end = logical
            .checked_add(seg_len)
            .ok_or_else(|| Error::Corrupt("stream segment overflow".into()))?;
        if wanted >= seg_end {
            logical = seg_end;
            continue;
        }
        let within = wanted.saturating_sub(logical);
        let take = (seg_len - within).min(len - filled as u64) as usize;
        if take == 0 {
            break;
        }
        match segment {
            Segment::Data { offset, length: _ } => {
                let abs = offset
                    .checked_add(within)
                    .ok_or_else(|| Error::Corrupt("stream data offset overflow".into()))?;
                src.read_exact_at(abs, &mut out[filled..filled + take])?;
            }
            Segment::Zero { .. } => {}
        }
        filled += take;
        wanted = wanted.saturating_add(take as u64);
        logical = seg_end;
        if filled == out.len() {
            break;
        }
    }
    if filled != out.len() {
        return Err(Error::Corrupt(format!(
            "stream ends before requested range ({filled} of {} bytes read)",
            out.len()
        )));
    }
    Ok(out)
}

fn segments_from_data(
    data: &NtfsData,
    boot: &NtfsBoot,
    volume: &VolumeRef,
) -> (Vec<Segment>, bool, Option<String>) {
    if data.resident {
        // Resident data is not addressed by physical extent.
        return (Vec::new(), true, None);
    }
    let mut segments = Vec::new();
    let mut logical = 0u64;
    let mut gaps = false;
    let mut runs = data.runs.clone();
    runs.sort_by_key(|r| r.vcn);
    for run in &runs {
        let run_off = match run.vcn.checked_mul(boot.cluster_size) {
            Some(v) => v,
            None => return (segments, false, Some("run VCN overflow".into())),
        };
        if run_off > logical {
            gaps = true;
            segments.push(Segment::zero(run_off - logical));
            logical = run_off;
        }
        let len = match run.clusters.checked_mul(boot.cluster_size) {
            Some(v) => v,
            None => return (segments, false, Some("run length overflow".into())),
        };
        match run.lcn {
            Some(lcn) => match boot
                .cluster_offset(lcn)
                .and_then(|rel| volume.offset.checked_add(rel))
            {
                Some(abs) => segments.push(Segment::data(abs, len)),
                None => return (segments, false, Some("run LCN is out of range".into())),
            },
            None => segments.push(Segment::zero(len)),
        }
        logical = logical.saturating_add(len);
    }
    let mapped = logical;
    let complete = !gaps && mapped >= data.real_size;
    let note = if gaps {
        Some("data runs contain a logical gap; missing bytes were emitted as zeroes".into())
    } else if mapped < data.real_size {
        Some("data runs cover less than the recorded file size".into())
    } else {
        None
    };
    (segments, complete, note)
}

fn data_location(
    data: &NtfsData,
    boot: &NtfsBoot,
    volume: &VolumeRef,
) -> (DataLocation, bool, Option<String>) {
    if data.encrypted {
        return (
            DataLocation::None,
            false,
            Some("EFS-encrypted $DATA is not decrypted by this build".into()),
        );
    }
    if data.compressed {
        return (
            DataLocation::None,
            false,
            Some("NTFS-compressed $DATA is not decompressed by this build".into()),
        );
    }
    if data.resident {
        return (
            DataLocation::resident(data.resident_bytes.clone()),
            true,
            None,
        );
    }
    let (segments, complete, note) = segments_from_data(data, boot, volume);
    if segments.is_empty() {
        (
            DataLocation::None,
            false,
            Some(note.unwrap_or_else(|| "no data runs are present".into())),
        )
    } else {
        (DataLocation::segments(segments), complete, note)
    }
}

/// Read the volume's `$Bitmap` attribute from MFT record 6 and return its raw
/// allocation bits. Bit `lcn` of the bitmap is 1 when cluster `lcn` is currently
/// allocated and 0 when it is free. Returns `Ok(None)` when the bitmap cannot be
/// located or parsed, so callers can report "reuse status unknown" instead of
/// guessing.
///
/// The bitmap can be large (volume size / cluster size / 8 bytes). Reads are
/// bounded so a corrupt or enormous `$DATA` attribute cannot exhaust memory.
fn read_volume_bitmap(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    boot: &NtfsBoot,
) -> Result<Option<Vec<u8>>> {
    let mft_offset = volume
        .offset
        .checked_add(
            boot.cluster_offset(boot.mft_lcn)
                .ok_or_else(|| Error::Corrupt("NTFS MFT offset overflow".into()))?,
        )
        .ok_or_else(|| Error::Corrupt("NTFS MFT absolute offset overflow".into()))?;
    let record6_off = mft_offset
        .checked_add(
            6u64.checked_mul(boot.record_size)
                .ok_or_else(|| Error::Corrupt("NTFS record 6 offset overflow".into()))?,
        )
        .ok_or_else(|| Error::Corrupt("NTFS record 6 absolute offset overflow".into()))?;
    let record_size = boot.record_size as usize;
    let mut record = vec![0u8; record_size];
    if src.read_exact_at(record6_off, &mut record).is_err() {
        return Ok(None);
    }
    let Some(parsed) = parse_record(record, 6, boot, volume) else {
        return Ok(None);
    };
    let Some(data) = parsed.data else {
        return Ok(None);
    };
    if data.encrypted || data.compressed {
        return Ok(None);
    }
    const MAX_BITMAP_BYTES: u64 = 256 * 1024 * 1024;
    if data.real_size > MAX_BITMAP_BYTES {
        return Ok(None);
    }
    let bytes = if data.resident {
        data.resident_bytes
    } else {
        let (segments, _complete, _note) = segments_from_data(&data, boot, volume);
        if segments.is_empty() {
            return Ok(None);
        }
        let mut out = Vec::with_capacity(data.real_size.min(MAX_BITMAP_BYTES) as usize);
        let mut filled = 0u64;
        for segment in &segments {
            if filled >= data.real_size {
                break;
            }
            let remaining = data.real_size - filled;
            match segment {
                Segment::Data { offset, length } => {
                    let take = (*length).min(remaining) as usize;
                    let mut buf = vec![0u8; take];
                    src.read_exact_at(*offset, &mut buf)?;
                    out.extend_from_slice(&buf);
                    filled += take as u64;
                }
                Segment::Zero { length } => {
                    let take = (*length).min(remaining) as usize;
                    out.extend(std::iter::repeat_n(0u8, take));
                    filled += take as u64;
                }
            }
        }
        out.resize(data.real_size as usize, 0);
        out
    };
    Ok(Some(bytes))
}

/// Assess whether a deleted file's data clusters have been reallocated.
///
/// NTFS marks a deleted file's clusters free in `$Bitmap` immediately after
/// deletion. If any of those clusters are currently allocated, another object
/// has probably taken them over, so the bytes at those LCNs are no longer the
/// original file's content. Reports the finding through `reasons` and adjusts
/// `label`/`score`. A bitmap that cannot be read is reported as unknown and does
/// not by itself change the classification.
fn assess_cluster_reuse(
    bitmap: &Option<Vec<u8>>,
    data: &NtfsData,
    reasons: &mut Vec<String>,
    label: &mut ConfidenceLabel,
    score: &mut u8,
) {
    let Some(bitmap) = bitmap else {
        reasons.push(
            "could not read the volume allocation bitmap; cluster-reuse status unknown".into(),
        );
        return;
    };
    const LIMIT: u64 = 4096;
    let mut reused = 0u64;
    let mut inspected = 0u64;
    for run in &data.runs {
        let Some(lcn) = run.lcn else {
            continue;
        };
        let end = lcn.saturating_add(run.clusters);
        let mut cluster = lcn;
        while cluster < end && inspected < LIMIT {
            let byte = cluster / 8;
            let bit = cluster % 8;
            let allocated = bitmap
                .get(byte as usize)
                .map(|b| b & (1u8 << bit) != 0)
                .unwrap_or(false);
            if allocated {
                reused += 1;
            }
            inspected += 1;
            cluster += 1;
        }
        if inspected >= LIMIT {
            break;
        }
    }
    if reused > 0 {
        reasons.push(format!(
            "allocation bitmap marks {reused} of {inspected} inspected data clusters as currently allocated; the clusters were likely reallocated to another object, so recovered bytes may not be the original content",
        ));
        *label = ConfidenceLabel::Partial;
        *score = (*score).min(45);
    } else if inspected > 0 {
        reasons.push(
            "allocation bitmap shows all inspected data clusters are free; original content is likely intact but unprotected".into(),
        );
    }
}

fn resolve_parent_path(
    record: u64,
    summaries: &HashMap<u64, RecordSummary>,
    memo: &mut HashMap<u64, String>,
    visiting: &mut HashSet<u64>,
) -> (String, bool) {
    if record == ROOT_RECORD || record == 0 {
        return (String::new(), false);
    }
    if let Some(path) = memo.get(&record) {
        return (path.clone(), false);
    }
    if !visiting.insert(record) {
        return (String::new(), true);
    }
    let Some(summary) = summaries.get(&record) else {
        visiting.remove(&record);
        return (String::new(), true);
    };
    let (mut parent_path, cycle) = resolve_parent_path(summary.parent, summaries, memo, visiting);
    if !parent_path.is_empty() {
        parent_path.push('/');
    }
    parent_path.push_str(&summary.name);
    visiting.remove(&record);
    memo.insert(record, parent_path.clone());
    (parent_path, cycle)
}

#[derive(Debug, Clone)]
struct RecordSummary {
    name: String,
    parent: u64,
    in_use: bool,
    is_dir: bool,
    sequence: u16,
}

/// Identify an NTFS volume.
pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    NtfsBoot::parse(src, volume).ok().map(|b| b.info())
}

/// Recovery entry point.
pub fn scan(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    options: &FsScanOptions,
    sink: &mut dyn CandidateSink,
) -> Result<FsSummary> {
    let boot = NtfsBoot::parse(src, volume)?;
    let mft = MftReader::open(src, volume, &boot)?;
    let mut summary = FsSummary::new(FsKind::Ntfs, None);
    summary.warnings.extend(boot.warnings.clone());

    let total_records = mft.record_count();
    let scan_records = total_records.min(MAX_MFT_RECORDS);
    if total_records > scan_records {
        summary.warnings.push(format!(
            "MFT has {total_records} records; this build scans the first {scan_records}. Records after the limit were not examined"
        ));
        summary.truncated = true;
    }

    let mut summaries: HashMap<u64, RecordSummary> = HashMap::new();
    for index in 0..scan_records {
        if options.is_cancelled() {
            break;
        }
        let record = match mft.read_record(index) {
            Ok(r) => r,
            Err(e) => {
                summary
                    .warnings
                    .push(format!("could not read MFT record {index}: {e}"));
                continue;
            }
        };
        let Some(parsed) = parse_record(record, index, &boot, volume) else {
            continue;
        };
        if let Some(ref label) = parsed.volume_name {
            if summary.label.is_none() {
                summary.label = Some(label.clone());
            }
        }
        if parsed.base_record != 0 {
            continue;
        }
        if let Some(primary) = parsed.primary_name() {
            summaries.insert(
                index,
                RecordSummary {
                    name: primary.name.clone(),
                    parent: primary.parent,
                    in_use: parsed.in_use,
                    is_dir: parsed.is_dir,
                    sequence: parsed.sequence,
                },
            );
        }
    }
    if !summaries.contains_key(&ROOT_RECORD) {
        summary.warnings.push(
            "NTFS root directory record 5 is missing; recovered paths may be incomplete".into(),
        );
    }

    // Read the volume allocation bitmap once. It tells us whether a deleted
    // file's data clusters have been reallocated by another object, which is
    // the single most important correctness signal for deleted-file recovery.
    let bitmap = read_volume_bitmap(src, volume, &boot).ok().flatten();

    let mut path_memo: HashMap<u64, String> = HashMap::new();
    let mut visiting = HashSet::new();
    for index in 0..scan_records {
        if options.is_cancelled() {
            break;
        }
        if summary.emitted as usize >= options.max_candidates {
            summary.truncated = true;
            break;
        }
        let record = match mft.read_record(index) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let Some(parsed) = parse_record(record, index, &boot, volume) else {
            continue;
        };
        if parsed.base_record != 0 {
            // Extension record; its base record owns the attributes.
            continue;
        }
        if parsed.is_dir {
            if parsed.in_use {
                summary.live_entries += 1;
            } else {
                summary.deleted_entries += 1;
            }
            continue;
        }
        let Some(primary) = parsed.primary_name() else {
            continue;
        };
        if parsed.in_use {
            summary.live_entries += 1;
        } else {
            summary.deleted_entries += 1;
        }
        if parsed.in_use && !options.include_live {
            continue;
        }
        // Records 0..15 are NTFS metadata objects. They are enumerated for the
        // parent map but are not user data, so they are not emitted.
        if index < 16 {
            continue;
        }

        let (mut parent_path, cycle) =
            resolve_parent_path(primary.parent, &summaries, &mut path_memo, &mut visiting);
        if cycle {
            parent_path.clear();
        }
        let mut reasons = vec![if parsed.in_use {
            "live MFT record (forensic listing)".to_string()
        } else {
            "MFT record is not in use".to_string()
        }];
        let mut label = if parsed.in_use {
            ConfidenceLabel::Verified
        } else {
            ConfidenceLabel::Likely
        };
        let mut score: u8 = if parsed.in_use { 96 } else { 86 };
        let mut complete;
        let mut data;
        let mut size = primary.real_size;

        if !parsed.fixup_ok {
            label = ConfidenceLabel::Partial;
            score = score.min(35);
            reasons.push("update-sequence fixup did not validate; the record may be torn".into());
        }
        if parsed.warnings.len() > 8 {
            summary.warnings.push(format!(
                "MFT record {index} produced {} parser warnings; using the first ones",
                parsed.warnings.len()
            ));
        }
        for warning in parsed.warnings.iter().take(8) {
            reasons.push(warning.clone());
        }

        match &parsed.data {
            Some(d) => {
                size = if d.real_size != 0 { d.real_size } else { size };
                let (location, data_complete, note) = data_location(d, &boot, volume);
                data = location;
                complete = data_complete;
                if let Some(note) = note {
                    reasons.push(note);
                    if !data.is_none() {
                        label = ConfidenceLabel::Partial;
                        score = score.min(62);
                    }
                }
                if d.resident {
                    reasons.push("data is resident inside the MFT record".into());
                    score = score.saturating_add(3);
                } else if !parsed.in_use {
                    // Deleted non-resident data: check whether its clusters have
                    // been reallocated by another object. This is the decisive
                    // signal for whether the recovered bytes are original content.
                    reasons.push("data is described by non-resident runs".into());
                    assess_cluster_reuse(&bitmap, d, &mut reasons, &mut label, &mut score);
                } else {
                    reasons.push("data is described by non-resident runs".into());
                }
                if d.encrypted || d.compressed {
                    label = ConfidenceLabel::Unverified;
                    score = 25;
                    complete = false;
                } else if d.initialized_size < d.real_size {
                    reasons.push(format!(
                        "initialized size {} is shorter than the file size {}; the tail was never written",
                        d.initialized_size, d.real_size
                    ));
                }
                if d.named {
                    reasons.push(
                        "only a named alternate data stream was found; unnamed data is unavailable"
                            .into(),
                    );
                    data = DataLocation::None;
                    complete = false;
                    label = ConfidenceLabel::Unverified;
                    score = 30;
                }
            }
            None => {
                reasons.push("no $DATA attribute is present; content cannot be located".into());
                label = ConfidenceLabel::Unverified;
                score = 25;
                data = DataLocation::None;
                complete = primary.real_size == 0;
            }
        }

        if primary.namespace == 2 {
            reasons.push("only a DOS 8.3 alias was preserved".into());
            score = score.min(55);
            label = ConfidenceLabel::Partial;
        } else if primary.namespace == 1 || primary.namespace == 3 {
            reasons.push("$FILE_NAME is a Win32 name".into());
        }
        if case_insensitive_eq(&primary.name, "$MFT") {
            reasons.push("system metadata file".into());
        }
        if let Some(parent) = summaries.get(&primary.parent) {
            if !parent.is_dir {
                reasons.push("parent reference does not point at a directory".into());
                label = ConfidenceLabel::Partial;
                score = score.min(60);
            }
            if parent.sequence != primary.parent_sequence {
                reasons.push(format!(
                    "parent reference sequence {} does not match the current parent sequence {}",
                    primary.parent_sequence, parent.sequence
                ));
            }
            if !parent.in_use {
                reasons.push("the parent directory record is itself deleted".into());
            }
        } else if primary.parent != ROOT_RECORD {
            reasons.push("parent record is unavailable; the path is incomplete".into());
        }
        if cycle {
            reasons.push("the parent chain contains a cycle; recovered path is empty".into());
            label = ConfidenceLabel::Partial;
            score = score.min(55);
        }

        let format = format_from_name(&primary.name);
        let method = match &parsed.data {
            Some(d) if d.resident => RecoveryMethod::NtfsMftResident,
            Some(_) => RecoveryMethod::NtfsMftRuns,
            None => RecoveryMethod::MetadataOnly,
        };
        let mut candidate = Candidate::new(
            primary.name.clone(),
            size,
            format,
            data,
            method,
            Confidence::new(label, score, reasons),
            "ntfs",
        );
        candidate.path = parent_path;
        candidate.state = if parsed.in_use {
            EntryState::Live
        } else {
            EntryState::Deleted
        };
        candidate.complete = complete;
        candidate.volume_offset = volume.offset;
        candidate.record_id = Some(format!("mft:{index}"));
        let (created, modified, accessed) = parsed.best_times();
        candidate.created = created;
        candidate.modified = modified;
        candidate.accessed = accessed;
        candidate.validation.push("MFT FILE record parsed".into());
        if parsed.fixup_ok {
            candidate
                .validation
                .push("update-sequence fixup valid".into());
        }
        candidate
            .notes
            .push(format!("MFT sequence {}", parsed.sequence));
        if primary.real_size != size {
            candidate.notes.push(format!(
                "$FILE_NAME size {} differs from $DATA size {}",
                primary.real_size, size
            ));
        }
        sink.emit(candidate)?;
        summary.emitted += 1;
    }

    Ok(summary)
}

fn case_insensitive_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_run_deltas_decode() {
        assert_eq!(read_signed(&[0x10]), 16);
        assert_eq!(read_signed(&[0xF0]), -16);
        assert_eq!(read_signed(&[0xFF, 0xFF]), -1);
        assert_eq!(read_signed(&[0x00, 0x01]), 256);
    }

    #[test]
    fn fixture_mft_is_readable() {
        let image = relift_testkit::ntfs_image();
        let dir = std::env::temp_dir().join(format!("relift-ntfs-debug-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("ntfs.img");
        std::fs::write(&path, image.image()).unwrap();
        let src = crate::source::FileSource::open(&path).unwrap();
        let volume = VolumeRef::new(0, src.len());
        let boot = NtfsBoot::parse(&src, &volume).unwrap();
        let mft = MftReader::open(&src, &volume, &boot).unwrap();
        assert_eq!(mft.record_count(), 64);
        let rec = mft.read_record(5).unwrap();
        let parsed = parse_record(rec, 5, &boot, &volume).unwrap();
        assert!(parsed.fixup_ok, "fixup failed");
        assert_eq!(parsed.names.len(), 1, "names {:?}", parsed.names);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_size_encoding() {
        assert_eq!(record_size_from(-10, 4096).unwrap(), 1024);
        assert_eq!(record_size_from(2, 4096).unwrap(), 8192);
        assert!(record_size_from(0, 4096).is_err());
    }

    /// A deleted non-resident file whose clusters are all free in the bitmap
    /// stays Likely; reallocated clusters downgrade it to Partial.
    fn make_runs(lcns: &[(u64, u64)]) -> Vec<Run> {
        lcns.iter()
            .map(|(lcn, clusters)| Run {
                vcn: 0,
                clusters: *clusters,
                lcn: Some(*lcn),
            })
            .collect()
    }

    #[test]
    fn cluster_reuse_downgrades_to_partial() {
        let mut label = ConfidenceLabel::Likely;
        let mut score = 86u8;
        let mut reasons = Vec::new();
        // Cluster 10 is allocated in the bitmap (byte 1, bit 2).
        let mut bitmap = vec![0u8; 16];
        bitmap[1] |= 1 << 2;
        let data = NtfsData {
            resident: false,
            resident_bytes: Vec::new(),
            real_size: 4096,
            allocated_size: 4096,
            initialized_size: 4096,
            runs: make_runs(&[(10, 8)]),
            compressed: false,
            encrypted: false,
            sparse: false,
            named: false,
        };
        assess_cluster_reuse(&Some(bitmap), &data, &mut reasons, &mut label, &mut score);
        assert_eq!(label, ConfidenceLabel::Partial);
        assert!(score <= 45);
        assert!(reasons.iter().any(|r| r.contains("allocated")));
    }

    #[test]
    fn cluster_free_keeps_likely() {
        let mut label = ConfidenceLabel::Likely;
        let mut score = 86u8;
        let mut reasons = Vec::new();
        // All-zero bitmap: clusters are free.
        let bitmap = vec![0u8; 16];
        let data = NtfsData {
            resident: false,
            resident_bytes: Vec::new(),
            real_size: 4096,
            allocated_size: 4096,
            initialized_size: 4096,
            runs: make_runs(&[(10, 8)]),
            compressed: false,
            encrypted: false,
            sparse: false,
            named: false,
        };
        assess_cluster_reuse(&Some(bitmap), &data, &mut reasons, &mut label, &mut score);
        assert_eq!(label, ConfidenceLabel::Likely);
        assert!(reasons.iter().any(|r| r.contains("free")));
    }

    #[test]
    fn bitmap_unavailable_reports_unknown() {
        let mut label = ConfidenceLabel::Likely;
        let mut score = 86u8;
        let mut reasons = Vec::new();
        let data = NtfsData {
            resident: false,
            resident_bytes: Vec::new(),
            real_size: 4096,
            allocated_size: 4096,
            initialized_size: 4096,
            runs: make_runs(&[(10, 8)]),
            compressed: false,
            encrypted: false,
            sparse: false,
            named: false,
        };
        assess_cluster_reuse(&None, &data, &mut reasons, &mut label, &mut score);
        assert_eq!(label, ConfidenceLabel::Likely);
        assert!(reasons.iter().any(|r| r.contains("unknown")));
    }
}
