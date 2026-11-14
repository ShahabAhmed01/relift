//! MBR and GPT partition table parsing.
//!
//! Relift never writes a partition table. Lost-partition support means reading
//! and exposing the contents of a partition that the current table does not
//! describe; it does not mean "repairing" the user's disk.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::source::ReadOnlySource;
use crate::util::{u16_le, u32_le, u64_le, utf16le_to_string};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionScheme {
    Mbr,
    Gpt,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub index: u32,
    pub offset: u64,
    pub length: u64,
    pub scheme: PartitionScheme,
    pub type_hint: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

const MAX_PARTITIONS: usize = 128;

/// Parse a partition table from the start of `src`. Returns `None` scheme and
/// an empty list when no table is present (a bare filesystem image, for
/// example). Parsing is deliberately conservative: impossible extents are
/// rejected and reported, never guessed.
pub fn read_partition_table(src: &dyn ReadOnlySource) -> Result<PartitionTable> {
    let len = src.len();
    if len < 512 {
        return Ok(PartitionTable::none());
    }
    let head_len = len.min(1024 * 1024);
    let head = src.read_vec(0, head_len)?;
    if head.len() < 512 {
        return Ok(PartitionTable::none());
    }

    let mut warnings = Vec::new();

    // GPT: a protective MBR is expected, but some tools write a bare GPT.
    if let Some(table) = try_gpt(src, &head, &mut warnings)? {
        return Ok(table);
    }

    let mbr_ok = u16_le(&head, 510) == Some(0xAA55);
    if !mbr_ok {
        return Ok(PartitionTable::none());
    }

    let mut volumes = Vec::new();
    let mut index = 0u32;
    for i in 0..4 {
        let off = 446 + i * 16;
        let ptype = head[off + 4];
        let start_lba = u32_le(&head, off + 8).unwrap_or(0) as u64;
        let sectors = u32_le(&head, off + 12).unwrap_or(0) as u64;
        if ptype == 0 || sectors == 0 {
            continue;
        }
        let offset = start_lba.saturating_mul(512);
        let length = sectors.saturating_mul(512);
        if ptype == 0xEE {
            // Protective MBR without a parseable GPT: expose it as a warning,
            // not as a fake NTFS/FAT volume.
            warnings.push(
                "protective GPT partition found, but the GPT header could not be parsed".into(),
            );
            continue;
        }
        if ptype == 0x05 || ptype == 0x0F || ptype == 0x85 {
            parse_ebr(src, offset, length, &mut volumes, &mut index, &mut warnings)?;
            continue;
        }
        if !valid_extent(offset, length, len) {
            warnings.push(format!(
                "MBR partition {i} has an impossible extent (offset {offset:#x}, length {length:#x}); ignored"
            ));
            continue;
        }
        volumes.push(Volume {
            index,
            offset,
            length,
            scheme: PartitionScheme::Mbr,
            type_hint: mbr_type_name(ptype),
            label: None,
        });
        index += 1;
    }

    if volumes.is_empty() {
        Ok(PartitionTable {
            scheme: PartitionScheme::None,
            volumes,
            warnings,
        })
    } else {
        Ok(PartitionTable {
            scheme: PartitionScheme::Mbr,
            volumes,
            warnings,
        })
    }
}

fn valid_extent(offset: u64, length: u64, total: u64) -> bool {
    offset
        .checked_add(length)
        .map(|end| end <= total && length > 0)
        .unwrap_or(false)
}

fn parse_ebr(
    src: &dyn ReadOnlySource,
    base: u64,
    extended_len: u64,
    volumes: &mut Vec<Volume>,
    index: &mut u32,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let total = src.len();
    let mut ebr_offset = base;
    for _ in 0..MAX_PARTITIONS {
        if volumes.len() >= MAX_PARTITIONS {
            warnings.push("extended partition chain truncated at 128 logical volumes".into());
            return Ok(());
        }
        if !valid_extent(ebr_offset, 512, total) {
            warnings.push(format!("EBR at {ebr_offset:#x} lies outside the source"));
            return Ok(());
        }
        let sector = src.read_vec(ebr_offset, 512)?;
        if u16_le(&sector, 510) != Some(0xAA55) {
            warnings.push(format!("EBR at {ebr_offset:#x} has no 0x55AA signature"));
            return Ok(());
        }
        let e0 = &sector[446..462];
        let e1 = &sector[462..478];
        let ptype = e0[4];
        let start = u32_le(e0, 8).unwrap_or(0) as u64;
        let sectors = u32_le(e0, 12).unwrap_or(0) as u64;
        if ptype != 0 && sectors > 0 {
            let offset = ebr_offset.saturating_add(start.saturating_mul(512));
            let length = sectors.saturating_mul(512);
            if valid_extent(offset, length, total) {
                volumes.push(Volume {
                    index: *index,
                    offset,
                    length,
                    scheme: PartitionScheme::Mbr,
                    type_hint: mbr_type_name(ptype),
                    label: None,
                });
                *index += 1;
            } else {
                warnings.push(format!(
                    "logical partition at {offset:#x} has an impossible extent; ignored"
                ));
            }
        }
        let next = u32_le(e1, 8).unwrap_or(0) as u64;
        let next_sectors = u32_le(e1, 12).unwrap_or(0) as u64;
        if next == 0 || next_sectors == 0 {
            return Ok(());
        }
        let next_offset = base.saturating_add(next.saturating_mul(512));
        if next_offset <= ebr_offset || next_offset >= base.saturating_add(extended_len) {
            warnings.push("extended partition chain is cyclic or out of bounds".into());
            return Ok(());
        }
        ebr_offset = next_offset;
    }
    warnings.push("extended partition chain depth limit reached".into());
    Ok(())
}

fn try_gpt(
    src: &dyn ReadOnlySource,
    head: &[u8],
    warnings: &mut Vec<String>,
) -> Result<Option<PartitionTable>> {
    // GPT normally lives at LBA 1 (512-byte sectors). Some images use 4K.
    let candidates = [512usize, 4096usize];
    for header_off in candidates {
        if header_off + 92 > head.len() {
            continue;
        }
        if &head[header_off..header_off + 8] != b"EFI PART" {
            continue;
        }
        let header = &head[header_off..header_off + 92];
        let header_size = u32_le(header, 12).unwrap_or(92) as usize;
        if !(92..=512).contains(&header_size) {
            warnings.push("GPT header size is invalid".into());
            return Ok(None);
        }
        let stored_crc = u32_le(header, 16).unwrap_or(0);
        let mut crc_buf = header[..header_size].to_vec();
        crc_buf[16..20].fill(0);
        let actual_crc = crc32fast::hash(&crc_buf);
        if stored_crc != 0 && stored_crc != actual_crc {
            warnings.push("GPT header CRC does not match; partition list may be unreliable".into());
        }
        let entry_lba = u64_le(header, 72).unwrap_or(0);
        let num_entries = u32_le(header, 80).unwrap_or(0);
        let entry_size = u32_le(header, 84).unwrap_or(0);
        if num_entries == 0 || num_entries as usize > 4096 || !(128..=4096).contains(&entry_size) {
            warnings.push("GPT entry array has invalid dimensions".into());
            return Ok(None);
        }
        let array_off = entry_lba.saturating_mul(512);
        let array_len = (num_entries as u64).saturating_mul(entry_size as u64);
        if array_off
            .checked_add(array_len)
            .map(|end| end > src.len())
            .unwrap_or(true)
        {
            warnings.push("GPT entry array lies outside the source".into());
            return Ok(None);
        }
        if array_len > 8 * 1024 * 1024 {
            warnings.push("GPT entry array is unreasonably large; ignored".into());
            return Ok(None);
        }
        let array = src.read_vec(array_off, array_len)?;
        let mut volumes = Vec::new();
        for i in 0..num_entries as usize {
            let start = i * entry_size as usize;
            let entry = &array[start..start + entry_size as usize];
            if entry[..16].iter().all(|b| *b == 0) {
                continue;
            }
            let first_lba = u64_le(entry, 32).unwrap_or(0);
            let last_lba = u64_le(entry, 40).unwrap_or(0);
            if last_lba < first_lba {
                warnings.push(format!(
                    "GPT partition {i} has last_lba < first_lba; ignored"
                ));
                continue;
            }
            let offset = first_lba.saturating_mul(512);
            let length = (last_lba - first_lba + 1).saturating_mul(512);
            if !valid_extent(offset, length, src.len()) {
                warnings.push(format!(
                    "GPT partition {i} has an impossible extent (offset {offset:#x}, length {length:#x}); ignored"
                ));
                continue;
            }
            let name_bytes = &entry[56..entry_size.min(128) as usize];
            let label = utf16le_to_string(name_bytes)
                .trim_end_matches('\u{0}')
                .trim()
                .to_string();
            volumes.push(Volume {
                index: i as u32,
                offset,
                length,
                scheme: PartitionScheme::Gpt,
                type_hint: gpt_type_name(&entry[..16]),
                label: if label.is_empty() { None } else { Some(label) },
            });
        }
        return Ok(Some(PartitionTable {
            scheme: PartitionScheme::Gpt,
            volumes,
            warnings: warnings.clone(),
        }));
    }
    Ok(None)
}

fn mbr_type_name(t: u8) -> String {
    let name = match t {
        0x01 => "FAT12",
        0x04 => "FAT16 <32M",
        0x05 => "Extended",
        0x06 => "FAT16",
        0x07 => "NTFS/exFAT",
        0x0B => "FAT32 CHS",
        0x0C => "FAT32 LBA",
        0x0E => "FAT16 LBA",
        0x0F => "Extended LBA",
        0x11 => "Hidden FAT12",
        0x12 => "Compaq diagnostics",
        0x27 => "Windows recovery",
        0x42 => "Windows dynamic",
        0x82 => "Linux swap",
        0x83 => "Linux",
        0x85 => "Linux extended",
        0x8E => "Linux LVM",
        0xA5 => "FreeBSD",
        0xA8 => "macOS UFS",
        0xAB => "macOS boot",
        0xAF => "macOS HFS+",
        0xEE => "GPT protective",
        0xEF => "EFI system",
        _ => return format!("MBR type 0x{t:02X}"),
    };
    name.to_string()
}

fn gpt_type_name(guid: &[u8]) -> String {
    // Mixed-endian GUID comparison against the canonical type GUIDs.
    const BASIC_DATA: [u8; 16] = [
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
        0xC7,
    ];
    const EFI_SYSTEM: [u8; 16] = [
        0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9,
        0x3B,
    ];
    const LINUX_DATA: [u8; 16] = [
        0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D,
        0xE4,
    ];
    const APPLE_HFS: [u8; 16] = [
        0x00, 0x53, 0x46, 0x48, 0x00, 0x00, 0xAA, 0x11, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43, 0xEC,
        0xAC,
    ];
    const APPLE_APFS: [u8; 16] = [
        0xEF, 0x57, 0x34, 0x7C, 0x00, 0x00, 0xAA, 0x11, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43, 0xEC,
        0xAC,
    ];
    let s = match guid {
        g if g == BASIC_DATA => "Microsoft basic data",
        g if g == EFI_SYSTEM => "EFI system",
        g if g == LINUX_DATA => "Linux filesystem",
        g if g == APPLE_HFS => "Apple HFS+",
        g if g == APPLE_APFS => "Apple APFS",
        _ => "GPT partition",
    };
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileSource;
    use std::io::Write;

    fn image(bytes: &[u8]) -> (std::path::PathBuf, FileSource) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "relift-part-{}-{serial}-{}.img",
            std::process::id(),
            bytes.len()
        ));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        drop(f);
        let src = FileSource::open(&path).unwrap();
        (path, src)
    }

    #[test]
    fn no_table_returns_none() {
        let (path, src) = image(&[0u8; 4096]);
        let t = read_partition_table(&src).unwrap();
        assert_eq!(t.scheme, PartitionScheme::None);
        assert!(t.volumes.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn mbr_partition_is_parsed_and_bounds_checked() {
        let mut img = vec![0u8; 4 * 1024 * 1024];
        img[446 + 4] = 0x0C; // FAT32 LBA
        img[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
        img[446 + 12..446 + 16].copy_from_slice(&2048u32.to_le_bytes());
        img[510] = 0x55;
        img[511] = 0xAA;
        let (path, src) = image(&img);
        let t = read_partition_table(&src).unwrap();
        assert_eq!(t.scheme, PartitionScheme::Mbr);
        assert_eq!(t.volumes.len(), 1);
        assert_eq!(t.volumes[0].offset, 2048 * 512);
        assert_eq!(t.volumes[0].length, 2048 * 512);
        assert!(t.volumes[0].type_hint.contains("FAT32"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn impossible_partition_is_ignored() {
        let mut img = vec![0u8; 4096];
        img[446 + 4] = 0x07;
        img[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes());
        img[446 + 12..446 + 16].copy_from_slice(&u32::MAX.to_le_bytes());
        img[510] = 0x55;
        img[511] = 0xAA;
        let (path, src) = image(&img);
        let t = read_partition_table(&src).unwrap();
        assert!(t.volumes.is_empty());
        assert!(
            t.warnings.iter().any(|w| w.contains("impossible")),
            "warnings: {:?}",
            t.warnings
        );
        let _ = std::fs::remove_file(path);
    }
}
