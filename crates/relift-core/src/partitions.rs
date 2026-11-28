//! Partition table parsing and lost partition detection.
//!
//! This module handles MBR, GPT, and APM partition tables, plus
//! filesystem signature scanning for lost/deleted partition recovery.

use std::collections::HashSet;

use crate::error::{Error, Result};
use crate::fs::{probe, FsInfo, FsKind};
use crate::source::ReadOnlySource;

/// Maximum partition entries to scan for (prevents runaway on corrupt disks)
const MAX_PARTITIONS: usize = 128;
/// Maximum sectors to scan for filesystem signatures (default: first 1TB)
const MAX_SIGNATURE_SCAN_SECTORS: u64 = 2_000_000_000; // ~1 TB at 512-byte sectors
/// Sector size for scanning alignment
const SECTOR_SIZE: u64 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionScheme {
    None,
    Mbr,
    Gpt,
    Apm,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PartitionTable {
    pub scheme: PartitionScheme,
    pub volumes: Vec<Volume>,
    pub warnings: Vec<String>,
}

impl PartitionTable {
    pub fn none() -> Self {
        Self {
            scheme: PartitionScheme::None,
            volumes: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Volume {
    pub index: u32,
    pub offset: u64,
    pub length: u64,
    pub scheme: PartitionScheme,
    pub type_hint: String,
    pub label: Option<String>,
    pub filesystem: Option<FsInfo>,
    pub bootable: bool,
    pub partition_type: Option<u8>,
    pub partition_guid: Option<String>,
}

impl Volume {
    pub fn new_simple(index: u32, offset: u64, length: u64, label: Option<String>) -> Self {
        Self {
            index,
            offset,
            length,
            scheme: PartitionScheme::None,
            type_hint: "whole source".into(),
            label,
            filesystem: None,
            bootable: false,
            partition_type: None,
            partition_guid: None,
        }
    }
}

/// Read partition table from source (MBR + GPT + APM)
pub fn read_partition_table(src: &dyn ReadOnlySource) -> Result<PartitionTable> {
    let mut table = PartitionTable::none();
    let source_len = src.len();

    if source_len < 512 {
        return Ok(table);
    }

    // Try GPT first (most modern)
    if let Ok(gpt) = read_gpt(src, source_len) {
        table = gpt;
        if !table.volumes.is_empty() {
            return Ok(table);
        }
        // GPT protective MBR exists but no partitions - fall through to MBR
    }

    // Try MBR
    if let Ok(mbr) = read_mbr(src, source_len) {
        if table.scheme == PartitionScheme::None || mbr.volumes.len() > table.volumes.len() {
            table = mbr;
        }
    }

    // Try APM (Mac)
    if table.volumes.is_empty() {
        if let Ok(apm) = read_apm(src, source_len) {
            table = apm;
        }
    }

    // If still no volumes, treat whole source as one volume
    if table.volumes.is_empty() {
        table
            .volumes
            .push(Volume::new_simple(0, 0, source_len, None));
    }

    // Probe filesystem on each volume
    for vol in &mut table.volumes {
        if let Some(fs) = probe(src, &crate::fs::VolumeRef::new(vol.offset, vol.length)) {
            vol.filesystem = Some(fs.clone());
            vol.type_hint = fs.kind.as_str().to_string();
        }
    }

    Ok(table)
}

/// Read MBR partition table
fn read_mbr(src: &dyn ReadOnlySource, source_len: u64) -> Result<PartitionTable> {
    let sector = src.read_vec(0, 512)?;
    if sector.len() < 512 || sector[510] != 0x55 || sector[511] != 0xAA {
        return Err(Error::Corrupt("Invalid MBR signature".into()));
    }

    let mut table = PartitionTable {
        scheme: PartitionScheme::Mbr,
        volumes: Vec::new(),
        warnings: Vec::new(),
    };

    for i in 0..4 {
        let entry_off = 446 + i * 16;
        if entry_off + 16 > sector.len() {
            break;
        }
        let entry = &sector[entry_off..entry_off + 16];

        let bootable = entry[0] == 0x80;
        let part_type = entry[4];
        if part_type == 0x00 {
            continue; // Empty
        }

        let start_lba = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as u64;
        let num_sectors = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;

        if num_sectors == 0 {
            continue;
        }

        let Some(offset) = start_lba.checked_mul(SECTOR_SIZE) else {
            table.warnings.push(format!(
                "MBR partition {}: start sector overflows byte offset",
                i
            ));
            continue;
        };
        let Some(length) = num_sectors.checked_mul(SECTOR_SIZE) else {
            table.warnings.push(format!(
                "MBR partition {}: sector count overflows byte length",
                i
            ));
            continue;
        };

        if offset >= source_len || length > source_len - offset {
            table.warnings.push(format!(
                "MBR partition {}: exceeds source bounds (offset={}, length={}, source={})",
                i, offset, length, source_len
            ));
            continue;
        }

        let type_hint = mbr_type_name(part_type).to_string();

        table.volumes.push(Volume {
            index: table.volumes.len() as u32,
            offset,
            length,
            scheme: PartitionScheme::Mbr,
            type_hint,
            label: None,
            filesystem: None,
            bootable,
            partition_type: Some(part_type),
            partition_guid: None,
        });
    }

    // Check for extended partitions and parse logical drives
    for i in 0..table.volumes.len() {
        if is_extended_partition(table.volumes[i].partition_type.unwrap_or(0)) {
            if let Ok(logical) = parse_extended_partition(src, &table.volumes[i], source_len) {
                table.volumes.extend(logical);
            }
        }
    }

    // Re-index
    for (idx, vol) in table.volumes.iter_mut().enumerate() {
        vol.index = idx as u32;
    }

    Ok(table)
}

fn is_extended_partition(part_type: u8) -> bool {
    matches!(part_type, 0x05 | 0x0F | 0x85 | 0xC5)
}

fn parse_extended_partition(
    src: &dyn ReadOnlySource,
    extended: &Volume,
    source_len: u64,
) -> Result<Vec<Volume>> {
    let mut logical = Vec::new();
    let mut current_ebr_offset = extended.offset;
    let mut visited = HashSet::new();

    for _ in 0..MAX_PARTITIONS {
        if !visited.insert(current_ebr_offset)
            || current_ebr_offset > source_len
            || source_len - current_ebr_offset < 512
        {
            break;
        }
        let sector = src.read_vec(current_ebr_offset, 512)?;
        if sector.len() < 512 || sector[510] != 0x55 || sector[511] != 0xAA {
            break;
        }

        // First entry: logical partition
        let entry1 = &sector[446..462];
        let part_type1 = entry1[4];
        let start_lba1 = u32::from_le_bytes([entry1[8], entry1[9], entry1[10], entry1[11]]) as u64;
        let num_sectors1 =
            u32::from_le_bytes([entry1[12], entry1[13], entry1[14], entry1[15]]) as u64;

        if part_type1 != 0x00 && num_sectors1 > 0 {
            let offset = start_lba1
                .checked_mul(SECTOR_SIZE)
                .and_then(|relative| current_ebr_offset.checked_add(relative));
            let length = num_sectors1.checked_mul(SECTOR_SIZE);
            if let (Some(offset), Some(length)) = (offset, length) {
                if offset < source_len && length <= source_len - offset {
                    logical.push(Volume {
                        index: 0, // Will be re-indexed
                        offset,
                        length,
                        scheme: PartitionScheme::Mbr,
                        type_hint: mbr_type_name(part_type1).to_string(),
                        label: None,
                        filesystem: None,
                        bootable: false,
                        partition_type: Some(part_type1),
                        partition_guid: None,
                    });
                }
            }
        }

        // Second entry: next EBR
        let entry2 = &sector[462..478];
        let part_type2 = entry2[4];
        let start_lba2 = u32::from_le_bytes([entry2[8], entry2[9], entry2[10], entry2[11]]) as u64;

        if !is_extended_partition(part_type2) || start_lba2 == 0 {
            break;
        }

        let Some(next_offset) = start_lba2
            .checked_mul(SECTOR_SIZE)
            .and_then(|relative| extended.offset.checked_add(relative))
        else {
            break;
        };
        current_ebr_offset = next_offset;
        if logical.len() >= MAX_PARTITIONS {
            break;
        }
    }

    Ok(logical)
}

fn mbr_type_name(t: u8) -> &'static str {
    match t {
        0x01 => "FAT12",
        0x04 => "FAT16 <32M",
        0x06 => "FAT16",
        0x07 => "NTFS/exFAT",
        0x0B => "FAT32 CHS",
        0x0C => "FAT32 LBA",
        0x0E => "FAT16 LBA",
        0x0F => "Extended LBA",
        0x11 => "Hidden FAT12",
        0x14 => "Hidden FAT16 <32M",
        0x16 => "Hidden FAT16",
        0x17 => "Hidden NTFS/exFAT",
        0x1B => "Hidden FAT32 CHS",
        0x1C => "Hidden FAT32 LBA",
        0x1E => "Hidden FAT16 LBA",
        0x27 => "Windows RE",
        0x42 => "LDM (Dynamic Disk)",
        0x82 => "Linux swap / Solaris",
        0x83 => "Linux",
        0x84 => "Hibernation",
        0x85 => "Linux Extended",
        0x86 => "NTFS volume set",
        0x87 => "NTFS volume set",
        0xA5 => "FreeBSD",
        0xA6 => "OpenBSD",
        0xA8 => "macOS UFS",
        0xA9 => "NetBSD",
        0xAB => "macOS Boot",
        0xAF => "macOS HFS/HFS+",
        0xEE => "GPT Protective",
        0xEF => "EFI System",
        0xFB => "VMware VMFS",
        0xFC => "VMware Swap",
        _ => "Unknown",
    }
}

/// Read GPT partition table
fn read_gpt(src: &dyn ReadOnlySource, source_len: u64) -> Result<PartitionTable> {
    if source_len < 1024 {
        return Err(Error::Corrupt("Source too small for GPT".into()));
    }

    // Read protective MBR (sector 0)
    let mbr = src.read_vec(0, 512)?;
    if mbr.len() < 512 || mbr[510] != 0x55 || mbr[511] != 0xAA {
        return Err(Error::Corrupt("Invalid protective MBR".into()));
    }

    // Check for GPT signature in protective MBR (type 0xEE)
    let has_protective = mbr[450] == 0xEE; // First partition entry type

    // Read GPT header (sector 1)
    let header = src.read_vec(512, 512)?;
    if header.len() < 92 {
        return Err(Error::Corrupt("GPT header too small".into()));
    }
    if &header[0..8] != b"EFI PART" {
        if !has_protective {
            return Err(Error::Corrupt("No GPT signature".into()));
        }
        // Has protective MBR but no GPT header - might be hybrid MBR
        return Err(Error::Corrupt("GPT header missing".into()));
    }

    let header_size = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as u64;
    if !(92..=512).contains(&header_size) {
        return Err(Error::Corrupt("Invalid GPT header size".into()));
    }

    let current_lba = u64::from_le_bytes([
        header[24], header[25], header[26], header[27], header[28], header[29], header[30],
        header[31],
    ]);
    if current_lba != 1 {
        return Err(Error::Corrupt("GPT header not at LBA 1".into()));
    }

    let _first_usable = u64::from_le_bytes([
        header[40], header[41], header[42], header[43], header[44], header[45], header[46],
        header[47],
    ]);
    let _last_usable = u64::from_le_bytes([
        header[48], header[49], header[50], header[51], header[52], header[53], header[54],
        header[55],
    ]);

    let partition_entry_lba = u64::from_le_bytes([
        header[72], header[73], header[74], header[75], header[76], header[77], header[78],
        header[79],
    ]);
    let num_entries = u32::from_le_bytes([header[80], header[81], header[82], header[83]]) as u64;
    let entry_size = u32::from_le_bytes([header[84], header[85], header[86], header[87]]) as u64;

    if !(128..=4096).contains(&entry_size)
        || !entry_size.is_multiple_of(8)
        || num_entries > 128 * 1024
    {
        return Err(Error::Corrupt("Invalid GPT entry size/count".into()));
    }

    // Read partition entries
    let entries_offset = partition_entry_lba
        .checked_mul(SECTOR_SIZE)
        .ok_or_else(|| Error::Corrupt("GPT entry offset overflows".into()))?;
    let entries_size = num_entries
        .checked_mul(entry_size)
        .ok_or_else(|| Error::Corrupt("GPT entry array size overflows".into()))?;
    if entries_offset > source_len {
        return Err(Error::Corrupt(
            "GPT entry array is outside the source".into(),
        ));
    }
    let bounded_entries_size = entries_size
        .min(128 * 1024)
        .min(source_len - entries_offset);
    let entries_data = src.read_vec(entries_offset, bounded_entries_size)?;

    let mut table = PartitionTable {
        scheme: PartitionScheme::Gpt,
        volumes: Vec::new(),
        warnings: Vec::new(),
    };
    if bounded_entries_size < entries_size {
        table.warnings.push(format!(
            "GPT partition entry array was truncated to {} bytes",
            bounded_entries_size
        ));
    }

    for i in 0..num_entries as usize {
        let entry_off = i * entry_size as usize;
        if entry_off + 128 > entries_data.len() {
            break;
        }
        let entry = &entries_data[entry_off..entry_off + 128];

        // Partition type GUID (first 16 bytes)
        let type_guid = &entry[0..16];
        if type_guid.iter().all(|&b| b == 0) {
            continue; // Unused entry
        }

        // Unique partition GUID
        let part_guid = &entry[16..32];

        let first_lba = u64::from_le_bytes([
            entry[32], entry[33], entry[34], entry[35], entry[36], entry[37], entry[38], entry[39],
        ]);
        let last_lba = u64::from_le_bytes([
            entry[40], entry[41], entry[42], entry[43], entry[44], entry[45], entry[46], entry[47],
        ]);

        if last_lba < first_lba {
            continue;
        }

        let Some(offset) = first_lba.checked_mul(SECTOR_SIZE) else {
            table.warnings.push(format!(
                "GPT partition {}: start LBA overflows byte offset",
                i
            ));
            continue;
        };
        let Some(length) = last_lba
            .checked_sub(first_lba)
            .and_then(|sectors| sectors.checked_add(1))
            .and_then(|sectors| sectors.checked_mul(SECTOR_SIZE))
        else {
            table
                .warnings
                .push(format!("GPT partition {}: sector range overflows", i));
            continue;
        };

        if offset >= source_len || length > source_len - offset {
            table
                .warnings
                .push(format!("GPT partition {}: exceeds source bounds", i));
            continue;
        }

        // Partition name (UTF-16LE, 72 bytes = 36 chars)
        let name_bytes = &entry[56..128];
        let name = String::from_utf16_lossy(
            &name_bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|chunk| u16::from_le_bytes(*chunk))
                .take_while(|&character| character != 0)
                .collect::<Vec<_>>(),
        );

        let attrs = u64::from_le_bytes([
            entry[48], entry[49], entry[50], entry[51], entry[52], entry[53], entry[54], entry[55],
        ]);
        let bootable = attrs & 1 != 0; // Bit 0: platform required

        let type_hint = gpt_type_name(type_guid);

        table.volumes.push(Volume {
            index: table.volumes.len() as u32,
            offset,
            length,
            scheme: PartitionScheme::Gpt,
            type_hint: type_hint.to_string(),
            label: if name.is_empty() { None } else { Some(name) },
            filesystem: None,
            bootable,
            partition_type: None,
            partition_guid: Some(format_guid(part_guid)),
        });
    }

    // Check for backup GPT at end of disk
    let backup_header_lba = (source_len / SECTOR_SIZE) - 1;
    if backup_header_lba > 1 {
        let backup_header = src.read_vec(backup_header_lba * SECTOR_SIZE, 512)?;
        if backup_header.len() >= 8 && &backup_header[0..8] == b"EFI PART" {
            table
                .warnings
                .push("Backup GPT header found at end of disk".into());
        }
    }

    Ok(table)
}

fn gpt_type_name(guid: &[u8]) -> &'static str {
    // Common GPT type GUIDs (little-endian)
    match guid {
        // EFI System Partition
        b"\x28\x73\x2A\xC1\x1F\xF8\xD2\x11\xBA\x4B\x00\xA0\xC9\x3E\xC9\x3B" => "EFI System",
        // Microsoft Reserved
        b"\x3C\x8E\x2A\xE3\x24\xD2\x11\xE2\xBF\x8B\x00\x1C\x42\x1C\x05\x3A" => "MS Reserved",
        // Microsoft Basic Data (NTFS/exFAT/FAT32)
        b"\xA2\xA0\xD0\xEB\xE5\xB9\x33\x44\x87\xC0\x68\xB6\xB7\x26\x99\xC7" => "MS Basic Data",
        // Linux Filesystem
        b"\xAF\x3D\xC6\x0F\x83\x84\x72\x47\x8E\x79\x3D\x69\xD8\x47\x7D\xE4" => "Linux Filesystem",
        // Linux Swap
        b"\x06\x57\xFD\x6D\x9A\x4A\x63\x44\x85\x88\x5C\x6F\x3A\x0B\x3B\x2E" => "Linux Swap",
        // Linux LVM
        b"\x6D\xFD\xA8\xE6\x3C\xE1\x4C\x4D\x8A\x9E\x00\x5A\x9F\x3B\x3A\x0D" => "Linux LVM",
        // Linux RAID
        b"\xA1\x9D\x88\xA5\x2D\x4C\xE5\x4A\x9D\x6E\x00\x5A\x9F\x3B\x3A\x0D" => "Linux RAID",
        // macOS HFS+
        b"\x48\x46\x53\x2B\x00\x00\xAA\x11\xAA\x11\x00\x30\x65\x43\xEC\xAC" => "macOS HFS+",
        // macOS APFS
        b"\x6B\x7A\x7D\x7C\xFA\x33\xF3\x4E\xA2\x6A\x00\x5A\x9F\x3B\x3A\x0D" => "macOS APFS",
        // Windows Recovery
        b"\x1D\x22\xE9\xC7\x16\x43\x63\x44\x85\x88\x5C\x6F\x3A\x0B\x3B\x2E" => "Windows Recovery",
        // FreeBSD
        b"\x3E\x5F\xB3\x51\x4E\x7E\x8C\x44\x8A\x9E\x00\x5A\x9F\x3B\x3A\x0D" => "FreeBSD",
        // OpenBSD
        b"\x82\x8C\x8E\x82\x8D\x8E\x82\x8D\x82\x8D\x82\x8D\x82\x8D\x82\x8D" => "OpenBSD",
        // NetBSD
        b"\x4E\x45\x54\x42\x53\x44\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00" => "NetBSD",
        // VMware VMFS
        b"\xAA\x31\xE3\x9F\xE5\xE1\x64\x4E\x8A\x9E\x00\x5A\x9F\x3B\x3A\x0D" => "VMware VMFS",
        // VMware vSAN
        b"\xBB\x86\x3F\x3C\xF5\xE1\x64\x4E\x8A\x9E\x00\x5A\x9F\x3B\x3A\x0D" => "VMware vSAN",
        _ => "Unknown",
    }
}

fn format_guid(bytes: &[u8]) -> String {
    if bytes.len() != 16 {
        return "invalid".into();
    }
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[3], bytes[2], bytes[1], bytes[0],
        bytes[5], bytes[4],
        bytes[7], bytes[6],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Read APM (Apple Partition Map)
fn read_apm(src: &dyn ReadOnlySource, source_len: u64) -> Result<PartitionTable> {
    if source_len < 1024 {
        return Err(Error::Corrupt("Source too small for APM".into()));
    }

    // APM starts at sector 1 (block 1)
    let sector = src.read_vec(512, 512)?;
    if sector.len() < 512 {
        return Err(Error::Corrupt("APM sector too small".into()));
    }

    // Check for APM signature "PM"
    if &sector[0..2] != b"PM" {
        return Err(Error::Corrupt("No APM signature".into()));
    }

    let mut table = PartitionTable {
        scheme: PartitionScheme::Apm,
        volumes: Vec::new(),
        warnings: Vec::new(),
    };

    let partition_count = u16::from_be_bytes([sector[2], sector[3]]) as usize;
    let partition_start = u32::from_be_bytes([sector[4], sector[5], sector[6], sector[7]]) as u64;
    let partition_size = u32::from_be_bytes([sector[8], sector[9], sector[10], sector[11]]) as u64;

    if partition_size < 512 || partition_count > 64 {
        return Err(Error::Corrupt("Invalid APM parameters".into()));
    }

    for i in 0..partition_count {
        let entry_offset = partition_start * 512 + i as u64 * partition_size;
        if entry_offset + partition_size > source_len {
            break;
        }
        let entry = src.read_vec(entry_offset, partition_size.min(512))?;
        if entry.len() < 64 {
            continue;
        }

        if &entry[0..2] != b"PM" {
            continue;
        }

        let part_name = String::from_utf8_lossy(&entry[48..80])
            .trim_end_matches('\0')
            .to_string();
        let part_type = String::from_utf8_lossy(&entry[80..112])
            .trim_end_matches('\0')
            .to_string();
        let start_sector = u32::from_be_bytes([entry[12], entry[13], entry[14], entry[15]]) as u64;
        let num_sectors = u32::from_be_bytes([entry[16], entry[17], entry[18], entry[19]]) as u64;

        if num_sectors == 0 {
            continue;
        }

        let offset = start_sector * 512;
        let length = num_sectors * 512;

        if offset >= source_len || offset + length > source_len {
            continue;
        }

        table.volumes.push(Volume {
            index: table.volumes.len() as u32,
            offset,
            length,
            scheme: PartitionScheme::Apm,
            type_hint: part_type,
            label: if part_name.is_empty() {
                None
            } else {
                Some(part_name)
            },
            filesystem: None,
            bootable: false,
            partition_type: None,
            partition_guid: None,
        });
    }

    Ok(table)
}

/// Scan for lost partitions by filesystem signature scanning
///
/// This is the key feature that TestDisk provides - scanning the entire
/// disk for filesystem signatures to find deleted/lost partitions.
pub fn scan_lost_partitions(
    src: &dyn ReadOnlySource,
    max_scan_bytes: Option<u64>,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<Volume>> {
    let source_len = src.len();
    let scan_limit = max_scan_bytes
        .unwrap_or(MAX_SIGNATURE_SCAN_SECTORS.saturating_mul(SECTOR_SIZE))
        .min(source_len);

    let mut found = Vec::new();
    let mut scanned = 0u64;
    const CHUNK: usize = 16 * 1024 * 1024; // 16 MB chunks

    // Known filesystem signatures with their offsets within the volume
    struct FsSignature {
        name: &'static str,
        kind: FsKind,
        offset: u64,
        signature: &'static [u8],
    }

    const SIGNATURES: &[FsSignature] = &[
        FsSignature {
            name: "NTFS",
            kind: FsKind::Ntfs,
            offset: 3,
            signature: b"NTFS    ",
        },
        FsSignature {
            name: "exFAT",
            kind: FsKind::ExFat,
            offset: 3,
            signature: b"EXFAT   ",
        },
        FsSignature {
            name: "FAT32",
            kind: FsKind::Fat32,
            offset: 82,
            signature: b"FAT32   ",
        },
        FsSignature {
            name: "FAT16",
            kind: FsKind::Fat16,
            offset: 54,
            signature: b"FAT16   ",
        },
        FsSignature {
            name: "FAT12",
            kind: FsKind::Fat12,
            offset: 54,
            signature: b"FAT12   ",
        },
        FsSignature {
            name: "ext2/3/4",
            kind: FsKind::Ext4,
            offset: 0x438,
            signature: b"\x53\xEF",
        }, // Magic at superblock offset 0x38
        FsSignature {
            name: "HFS+",
            kind: FsKind::HfsPlus,
            offset: 1024,
            signature: b"H+",
        },
        FsSignature {
            name: "APFS",
            kind: FsKind::Apfs,
            offset: 0,
            signature: b"NXSB",
        },
        FsSignature {
            name: "Btrfs",
            kind: FsKind::Btrfs,
            offset: 0x10040,
            signature: b"_BHRfS_M",
        },
        FsSignature {
            name: "XFS",
            kind: FsKind::Xfs,
            offset: 0,
            signature: b"XFSB",
        },
        FsSignature {
            name: "ReFS",
            kind: FsKind::Refs,
            offset: 3,
            signature: b"ReFS",
        },
        FsSignature {
            name: "ISO 9660",
            kind: FsKind::Iso9660,
            offset: 0x8001,
            signature: b"CD001",
        },
    ];

    let max_probe_span = SIGNATURES
        .iter()
        .map(|s| s.offset.saturating_add(s.signature.len() as u64))
        .max()
        .unwrap_or(1);
    let overlap = max_probe_span.div_ceil(SECTOR_SIZE) * SECTOR_SIZE;
    let step = CHUNK as u64 - overlap;
    let mut sector_buf = vec![0u8; CHUNK];

    while scanned < scan_limit {
        let candidate_span = step.min(scan_limit - scanned);
        let wanted = candidate_span
            .saturating_add(max_probe_span)
            .min(source_len - scanned) as usize;
        let mut actual_read = 0usize;
        while actual_read < wanted {
            let n = src.read_at(
                scanned + actual_read as u64,
                &mut sector_buf[actual_read..wanted],
            )?;
            if n == 0 {
                break;
            }
            actual_read += n;
        }
        if actual_read == 0 {
            break;
        }

        // Scan source-aligned sector starts. The overlapping tail guarantees
        // signatures with large in-volume offsets are visible in this chunk.
        let mut sector_offset = 0u64;
        while sector_offset < candidate_span {
            let abs_offset = scanned + sector_offset;
            if abs_offset >= scan_limit {
                break;
            }

            for sig in SIGNATURES {
                let relative = (sector_offset + sig.offset) as usize;
                let end = relative + sig.signature.len();
                if end > actual_read {
                    continue;
                }
                if &sector_buf[relative..end] == sig.signature {
                    let vol_ref = crate::fs::VolumeRef::new(
                        abs_offset,
                        source_len.saturating_sub(abs_offset),
                    );
                    if let Some(fs_info) = probe(src, &vol_ref) {
                        if fs_info.kind == sig.kind {
                            let is_duplicate = found
                                .iter()
                                .any(|v: &Volume| v.offset == abs_offset && v.length > 0);
                            if !is_duplicate {
                                let estimated_len =
                                    estimate_partition_size(src, abs_offset, sig.kind, source_len)
                                        .min(source_len.saturating_sub(abs_offset));
                                if estimated_len > 0 {
                                    found.push(Volume {
                                        index: found.len() as u32,
                                        offset: abs_offset,
                                        length: estimated_len,
                                        scheme: PartitionScheme::None,
                                        type_hint: format!(
                                            "{} (found by signature scan)",
                                            sig.name
                                        ),
                                        label: None,
                                        filesystem: Some(fs_info),
                                        bootable: false,
                                        partition_type: None,
                                        partition_guid: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            sector_offset += SECTOR_SIZE;
        }

        scanned = scanned.saturating_add(step).min(scan_limit);
        progress(scanned, scan_limit);
    }

    // Sort by offset
    found.sort_by_key(|v| v.offset);

    // Merge overlapping/adjacent partitions (keep the one with stronger evidence)
    let mut merged: Vec<Volume> = Vec::new();
    for vol in found {
        if let Some(last) = merged.last_mut() {
            if vol.offset < last.offset.saturating_add(last.length) {
                // Overlapping - keep the one with filesystem info
                if vol.filesystem.is_some() && last.filesystem.is_none() {
                    *last = vol;
                }
                continue;
            }
        }
        merged.push(vol);
    }

    // Re-index
    for (idx, vol) in merged.iter_mut().enumerate() {
        vol.index = idx as u32;
    }

    Ok(merged)
}

fn estimate_partition_size(
    src: &dyn ReadOnlySource,
    offset: u64,
    kind: FsKind,
    source_len: u64,
) -> u64 {
    // Try to read filesystem-specific size information
    let _vol_ref = crate::fs::VolumeRef::new(offset, source_len.saturating_sub(offset));

    match kind {
        FsKind::Ntfs => {
            // NTFS: read $Boot sector for total sectors
            if let Ok(boot) = src.read_vec(offset, 512) {
                if boot.len() >= 48 {
                    let total_sectors = u64::from_le_bytes([
                        boot[40], boot[41], boot[42], boot[43], boot[44], boot[45], boot[46],
                        boot[47],
                    ]);
                    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]) as u64;
                    let sectors_per_cluster = boot[13] as u64;
                    if bytes_per_sector > 0 && sectors_per_cluster > 0 {
                        if let Some(size) = total_sectors.checked_mul(bytes_per_sector) {
                            if size > 0 {
                                return size.min(source_len.saturating_sub(offset));
                            }
                        }
                    }
                }
            }
        }
        FsKind::ExFat => {
            if let Ok(boot) = src.read_vec(offset, 512) {
                if boot.len() >= 88 {
                    let total_sectors = u64::from_le_bytes([
                        boot[72], boot[73], boot[74], boot[75], boot[76], boot[77], boot[78],
                        boot[79],
                    ]);
                    if let Some(bytes_per_sector) = 1u64.checked_shl(boot[108] as u32) {
                        if let Some(size) = total_sectors.checked_mul(bytes_per_sector) {
                            if size > 0 {
                                return size.min(source_len.saturating_sub(offset));
                            }
                        }
                    }
                }
            }
        }
        FsKind::Fat12 | FsKind::Fat16 | FsKind::Fat32 => {
            if let Ok(boot) = src.read_vec(offset, 512) {
                if boot.len() >= 36 {
                    let total_sectors16 = u16::from_le_bytes([boot[19], boot[20]]) as u64;
                    let total_sectors32 =
                        u32::from_le_bytes([boot[32], boot[33], boot[34], boot[35]]) as u64;
                    let total_sectors = if total_sectors16 > 0 {
                        total_sectors16
                    } else {
                        total_sectors32
                    };
                    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]) as u64;
                    if bytes_per_sector > 0 && total_sectors > 0 {
                        if let Some(size) = total_sectors.checked_mul(bytes_per_sector) {
                            return size.min(source_len.saturating_sub(offset));
                        }
                    }
                }
            }
        }
        FsKind::Ext2 | FsKind::Ext3 | FsKind::Ext4 => {
            // ext superblock at offset 1024
            if let Ok(sb) = src.read_vec(offset + 1024, 1024) {
                if sb.len() >= 20 {
                    let blocks_count = u32::from_le_bytes([sb[4], sb[5], sb[6], sb[7]]) as u64;
                    let log_block_size = sb[24] as u64;
                    if let Some(block_size) = 1024u64.checked_shl(log_block_size as u32) {
                        if blocks_count > 0 && block_size > 0 {
                            if let Some(size) = blocks_count.checked_mul(block_size) {
                                return size.min(source_len.saturating_sub(offset));
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    // Default: assume partition extends to next found partition or end of disk
    source_len - offset
}

/// Attempt to reconstruct a damaged partition table
pub fn repair_partition_table(
    _src: &dyn ReadOnlySource,
    lost_partitions: &[Volume],
) -> Result<PartitionTable> {
    // This would write a new MBR/GPT - for now, just return the detected layout
    // as a "virtual" partition table for scanning purposes
    let mut table = PartitionTable {
        scheme: PartitionScheme::None,
        volumes: lost_partitions.to_vec(),
        warnings: vec!["Reconstructed from signature scan - not written to disk".into()],
    };

    for (idx, vol) in table.volumes.iter_mut().enumerate() {
        vol.index = idx as u32;
    }

    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileSource;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn reads_mbr_with_primary_partitions() {
        let mut file = NamedTempFile::new().unwrap();
        // Disk needs to be large enough for partitions:
        // Part 1: 2048 + 200000 = 202048 sectors
        // Part 2: 202048 + 100000 = 302048 sectors
        // Total: 302048 sectors * 512 = ~154 MB
        let total_sectors = 302048 + 1000; // extra space
        let mut disk = vec![0u8; total_sectors * 512];

        // MBR signature
        disk[510] = 0x55;
        disk[511] = 0xAA;

        // Partition 1: NTFS at LBA 2048, 200000 sectors
        disk[446] = 0x80; // bootable
        disk[450] = 0x07; // NTFS
        disk[454..458].copy_from_slice(&2048u32.to_le_bytes()); // start LBA
        disk[458..462].copy_from_slice(&200000u32.to_le_bytes()); // num sectors

        // Partition 2: FAT32 at LBA 202048, 100000 sectors
        disk[462] = 0x00;
        disk[466] = 0x0C; // FAT32 LBA
        disk[470..474].copy_from_slice(&202048u32.to_le_bytes());
        disk[474..478].copy_from_slice(&100000u32.to_le_bytes());

        file.write_all(&disk).unwrap();
        file.flush().unwrap();

        let src = FileSource::open(file.path()).unwrap();
        let table = read_partition_table(&src).unwrap();

        assert_eq!(table.scheme, PartitionScheme::Mbr);
        assert_eq!(table.volumes.len(), 2);
        assert_eq!(table.volumes[0].offset, 2048 * 512);
        assert_eq!(table.volumes[0].length, 200000 * 512);
        assert!(table.volumes[0].bootable);
        assert_eq!(table.volumes[1].offset, 202048 * 512);
    }

    #[test]
    fn reads_gpt_partition_table() {
        let mut file = NamedTempFile::new().unwrap();
        // Disk needs to be large enough for partitions:
        // Part 1: 34 + 2048 = 2082 sectors
        // Part 2: 2082 + 100000 = 102082 sectors
        // Total: 102082 sectors * 512 = ~52 MB + GPT structures
        let total_sectors = 102082 + 1000; // extra space
        let mut disk = vec![0u8; total_sectors * 512];

        // Protective MBR
        disk[510] = 0x55;
        disk[511] = 0xAA;
        disk[450] = 0xEE; // GPT protective

        // GPT Header at LBA 1
        disk[512..520].copy_from_slice(b"EFI PART");
        disk[524..528].copy_from_slice(&92u32.to_le_bytes()); // header size
        disk[536..544].copy_from_slice(&1u64.to_le_bytes()); // current LBA
        disk[552..560].copy_from_slice(&34u64.to_le_bytes()); // first usable
        disk[560..568].copy_from_slice(&(total_sectors as u64 - 34).to_le_bytes()); // last usable
        disk[584..592].copy_from_slice(&2u64.to_le_bytes()); // partition entry LBA
        disk[592..596].copy_from_slice(&4u32.to_le_bytes()); // num entries
        disk[596..600].copy_from_slice(&128u32.to_le_bytes()); // entry size

        // Entry 1: EFI System (LBA 34..+2048)
        let entry_off = 2 * 512; // LBA 2
        disk[entry_off..entry_off + 16]
            .copy_from_slice(b"\x28\x73\x2A\xC1\x1F\xF8\xD2\x11\xBA\x4B\x00\xA0\xC9\x3E\xC9\x3B");
        disk[entry_off + 32..entry_off + 40].copy_from_slice(&34u64.to_le_bytes());
        disk[entry_off + 40..entry_off + 48]
            .copy_from_slice(&((34 + 2048 - 1) as u64).to_le_bytes());
        // Name: "EFI" (UTF-16LE, 36 chars = 72 bytes)
        let name_utf16: Vec<u8> = "EFI"
            .encode_utf16()
            .chain(std::iter::repeat(0))
            .take(36)
            .flat_map(|c| c.to_le_bytes())
            .collect();
        disk[entry_off + 56..entry_off + 56 + 72].copy_from_slice(&name_utf16);

        // Entry 2: Microsoft Basic Data (LBA 2082..+100000)
        let entry_off2 = entry_off + 128;
        disk[entry_off2..entry_off2 + 16]
            .copy_from_slice(b"\xA2\xA0\xD0\xEB\xE5\xB9\x33\x44\x87\xC0\x68\xB6\xB7\x26\x99\xC7");
        disk[entry_off2 + 32..entry_off2 + 40].copy_from_slice(&2082u64.to_le_bytes());
        disk[entry_off2 + 40..entry_off2 + 48]
            .copy_from_slice(&((2082 + 100000 - 1) as u64).to_le_bytes());
        // Name: "Windows" (UTF-16LE, 36 chars = 72 bytes)
        let name2_utf16: Vec<u8> = "Windows"
            .encode_utf16()
            .chain(std::iter::repeat(0))
            .take(36)
            .flat_map(|c| c.to_le_bytes())
            .collect();
        disk[entry_off2 + 56..entry_off2 + 56 + 72].copy_from_slice(&name2_utf16);

        file.write_all(&disk).unwrap();
        file.flush().unwrap();

        let src = FileSource::open(file.path()).unwrap();
        let table = read_partition_table(&src).unwrap();

        assert_eq!(table.scheme, PartitionScheme::Gpt);
        assert_eq!(table.volumes.len(), 2);
        assert_eq!(table.volumes[0].type_hint, "EFI System");
        assert_eq!(table.volumes[1].type_hint, "MS Basic Data");
    }

    #[test]
    fn lost_partition_scan_checks_sector_signatures_from_the_chunk_buffer() {
        let mut file = NamedTempFile::new().unwrap();
        let mut disk = vec![0u8; 1024 * 1024];
        let start = 4 * SECTOR_SIZE as usize;
        disk[start + 3..start + 11].copy_from_slice(b"NTFS    ");
        disk[start + 11..start + 13].copy_from_slice(&512u16.to_le_bytes());
        disk[start + 13] = 8;
        disk[start + 40..start + 48].copy_from_slice(&2000u64.to_le_bytes());
        disk[start + 48..start + 56].copy_from_slice(&2u64.to_le_bytes());
        disk[start + 56..start + 64].copy_from_slice(&3u64.to_le_bytes());
        disk[start + 64] = (-10i8) as u8;
        disk[start + 68] = (-12i8) as u8;
        disk[start + 510..start + 512].copy_from_slice(&[0x55, 0xaa]);
        file.write_all(&disk).unwrap();
        file.flush().unwrap();

        let src = FileSource::open(file.path()).unwrap();
        let mut progress_updates = Vec::new();
        let mut progress = |done, total| progress_updates.push((done, total));
        let volumes = scan_lost_partitions(&src, None, &mut progress).unwrap();

        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].offset, start as u64);
        assert_eq!(volumes[0].filesystem.as_ref().unwrap().kind, FsKind::Ntfs);
        assert_eq!(volumes[0].length, 2000 * 512);
        assert!(!progress_updates.is_empty());
    }

    #[test]
    fn gpt_partition_range_overflow_is_reported_and_skipped() {
        let mut file = NamedTempFile::new().unwrap();
        let mut disk = vec![0u8; 256 * 512];
        disk[510] = 0x55;
        disk[511] = 0xAA;
        disk[450] = 0xEE;
        disk[512..520].copy_from_slice(b"EFI PART");
        disk[524..528].copy_from_slice(&92u32.to_le_bytes());
        disk[536..544].copy_from_slice(&1u64.to_le_bytes());
        disk[584..592].copy_from_slice(&2u64.to_le_bytes());
        disk[592..596].copy_from_slice(&1u32.to_le_bytes());
        disk[596..600].copy_from_slice(&128u32.to_le_bytes());
        disk[1024] = 1; // Non-empty partition type GUID
        disk[1056..1064].copy_from_slice(&1u64.to_le_bytes());
        disk[1064..1072].copy_from_slice(&u64::MAX.to_le_bytes());

        file.write_all(&disk).unwrap();
        let src = FileSource::open(file.path()).unwrap();
        let table = read_gpt(&src, disk.len() as u64).unwrap();

        assert!(table.volumes.is_empty());
        assert!(table
            .warnings
            .iter()
            .any(|warning| warning.contains("overflows")));
    }

    #[test]
    fn extended_partition_ebr_cycles_are_stopped() {
        let mut file = NamedTempFile::new().unwrap();
        let mut disk = vec![0u8; 16 * 512];
        for lba in [1usize, 2] {
            let offset = lba * 512;
            disk[offset + 510] = 0x55;
            disk[offset + 511] = 0xAA;
            disk[offset + 462 + 4] = 0x0F;
            disk[offset + 462 + 8..offset + 462 + 12].copy_from_slice(&1u32.to_le_bytes());
        }
        file.write_all(&disk).unwrap();
        let src = FileSource::open(file.path()).unwrap();
        let extended = Volume {
            index: 0,
            offset: 512,
            length: disk.len() as u64 - 512,
            scheme: PartitionScheme::Mbr,
            type_hint: "Extended".into(),
            label: None,
            filesystem: None,
            bootable: false,
            partition_type: Some(0x0F),
            partition_guid: None,
        };

        let logical = parse_extended_partition(&src, &extended, disk.len() as u64).unwrap();
        assert!(logical.is_empty());
    }
}
