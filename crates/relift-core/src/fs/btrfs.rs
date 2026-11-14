//! Btrfs metadata recovery.
//!
//! Btrfs is a copy-on-write filesystem with a tree-of-trees structure. The root
//! tree points to subvolume trees, which contain file and directory items.
//! Deleted files can be recovered from older tree roots (snapshots) or by
//! scanning for unreferenced items in the current tree.

use std::collections::HashMap;

use crate::candidate::{
    Candidate, Confidence, ConfidenceLabel, DataLocation, EntryState, RecoveryMethod, Segment,
};
use crate::error::{Error, Result};
use crate::fs::{
    read_at_volume, CandidateSink, FsInfo, FsKind, FsScanOptions, FsSummary, VolumeRef,
};
use crate::source::ReadOnlySource;
use crate::util::{u16_le, u32_le, u64_le, u8_at};

const BTRFS_MAGIC: &[u8] = b"_BHRfS_M";
const BTRFS_MAX_ROOTS: usize = 256;

#[derive(Debug, Clone)]
struct BtrfsSuperblock {
    csum: [u8; 32],
    fsid: [u8; 16],
    bytenr: u64,
    flags: u64,
    magic: [u8; 8],
    generation: u64,
    root: u64,
    chunk_root: u64,
    log_root: u64,
    log_root_transid: u64,
    total_bytes: u64,
    bytes_used: u64,
    root_dir_objectid: u64,
    num_devices: u64,
    sectorsize: u32,
    nodesize: u32,
    leafsize: u32,
    stripesize: u32,
    sys_chunk_array_size: u32,
    chunk_root_generation: u64,
    compat_flags: u64,
    compat_ro_flags: u64,
    incompat_flags: u64,
    csum_type: u16,
    root_level: u8,
    chunk_root_level: u8,
    log_root_level: u8,
    warnings: Vec<String>,
}

impl BtrfsSuperblock {
    fn parse(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Result<Self> {
        if volume.length < 0x10000 + 4096 {
            return Err(Error::invalid("volume too small for Btrfs superblock"));
        }
        let buf = read_at_volume(src, volume, 0x10000, 4096)?;
        if buf.len() < 4096 {
            return Err(Error::invalid("truncated Btrfs superblock"));
        }
        if &buf[0x10000..0x10008] != BTRFS_MAGIC && &buf[256..264] != BTRFS_MAGIC {
            return Err(Error::invalid("missing Btrfs magic"));
        }

        let mut warnings = Vec::new();
        let bytenr = u64_le(&buf, 256).unwrap_or(0);
        let flags = u64_le(&buf, 264).unwrap_or(0);
        let magic = buf[272..280].try_into().unwrap();
        let generation = u64_le(&buf, 280).unwrap_or(0);
        let root = u64_le(&buf, 288).unwrap_or(0);
        let chunk_root = u64_le(&buf, 296).unwrap_or(0);
        let log_root = u64_le(&buf, 304).unwrap_or(0);
        let log_root_transid = u64_le(&buf, 312).unwrap_or(0);
        let total_bytes = u64_le(&buf, 320).unwrap_or(0);
        let bytes_used = u64_le(&buf, 328).unwrap_or(0);
        let root_dir_objectid = u64_le(&buf, 336).unwrap_or(0);
        let num_devices = u64_le(&buf, 344).unwrap_or(0);
        let sectorsize = u32_le(&buf, 352).unwrap_or(4096);
        let nodesize = u32_le(&buf, 356).unwrap_or(16384);
        let leafsize = u32_le(&buf, 360).unwrap_or(16384);
        let stripesize = u32_le(&buf, 364).unwrap_or(4096);
        let sys_chunk_array_size = u32_le(&buf, 368).unwrap_or(0);
        let chunk_root_generation = u64_le(&buf, 376).unwrap_or(0);
        let compat_flags = u64_le(&buf, 384).unwrap_or(0);
        let compat_ro_flags = u64_le(&buf, 392).unwrap_or(0);
        let incompat_flags = u64_le(&buf, 400).unwrap_or(0);
        let csum_type = u16_le(&buf, 408).unwrap_or(0);
        let root_level = u8_at(&buf, 410).unwrap_or(0);
        let chunk_root_level = u8_at(&buf, 411).unwrap_or(0);
        let log_root_level = u8_at(&buf, 412).unwrap_or(0);

        if incompat_flags & 0x1 != 0 {
            warnings
                .push("Btrfs has mixed block groups (RAID56); recovery may be incomplete".into());
        }
        if incompat_flags & 0x2 != 0 {
            warnings.push("Btrfs uses compression; compressed extents need decompression".into());
        }
        if incompat_flags & 0x10 != 0 {
            warnings.push("Btrfs has skinny metadata extents".into());
        }

        Ok(Self {
            csum: buf[0..32].try_into().unwrap(),
            fsid: buf[32..48].try_into().unwrap(),
            bytenr,
            flags,
            magic,
            generation,
            root,
            chunk_root,
            log_root,
            log_root_transid,
            total_bytes,
            bytes_used,
            root_dir_objectid,
            num_devices,
            sectorsize,
            nodesize,
            leafsize,
            stripesize,
            sys_chunk_array_size,
            chunk_root_generation,
            compat_flags,
            compat_ro_flags,
            incompat_flags,
            csum_type,
            root_level,
            chunk_root_level,
            log_root_level,
            warnings,
        })
    }
}

#[derive(Debug, Clone)]
struct BtrfsKey {
    objectid: u64,
    type_: u8,
    offset: u64,
}

impl BtrfsKey {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 17 > buf.len() {
            return None;
        }
        let objectid = u64_le(buf, offset)?;
        let type_ = u8_at(buf, offset + 8)?;
        let offset_val = u64_le(buf, offset + 9)?;
        Some(Self {
            objectid,
            type_,
            offset: offset_val,
        })
    }
}

#[derive(Debug, Clone)]
struct BtrfsItem {
    key: BtrfsKey,
    data_offset: u32,
    data_size: u32,
}

impl BtrfsItem {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 25 > buf.len() {
            return None;
        }
        let key = BtrfsKey::parse(buf, offset)?;
        let data_offset = u32_le(buf, offset + 17)?;
        let data_size = u32_le(buf, offset + 21)?;
        Some(Self {
            key,
            data_offset,
            data_size,
        })
    }
}

#[derive(Debug, Clone)]
struct BtrfsLeafHeader {
    csum: [u8; 32],
    fsid: [u8; 16],
    bytenr: u64,
    flags: u64,
    generation: u64,
    owner: u64,
    nritems: u32,
    level: u8,
}

impl BtrfsLeafHeader {
    fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 111 {
            return None;
        }
        Some(Self {
            csum: buf[0..32].try_into().unwrap(),
            fsid: buf[32..48].try_into().unwrap(),
            bytenr: u64_le(buf, 48)?,
            flags: u64_le(buf, 56)?,
            generation: u64_le(buf, 64)?,
            owner: u64_le(buf, 72)?,
            nritems: u32_le(buf, 80)?,
            level: u8_at(buf, 84)?,
        })
    }
}

#[derive(Debug, Clone)]
struct BtrfsInodeItem {
    generation: u64,
    transid: u64,
    size: u64,
    nbytes: u64,
    block_group: u64,
    nlink: u32,
    uid: u32,
    gid: u32,
    mode: u32,
    rdev: u64,
    flags: u64,
    sequence: u64,
    reserved: [u64; 4],
    atime: BtrfsTimespec,
    ctime: BtrfsTimespec,
    mtime: BtrfsTimespec,
    otime: BtrfsTimespec,
}

#[derive(Debug, Clone)]
struct BtrfsTimespec {
    sec: u64,
    nsec: u32,
}

impl BtrfsInodeItem {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 176 > buf.len() {
            return None;
        }
        Some(Self {
            generation: u64_le(buf, offset)?,
            transid: u64_le(buf, offset + 8)?,
            size: u64_le(buf, offset + 16)?,
            nbytes: u64_le(buf, offset + 24)?,
            block_group: u64_le(buf, offset + 32)?,
            nlink: u32_le(buf, offset + 40)?,
            uid: u32_le(buf, offset + 44)?,
            gid: u32_le(buf, offset + 48)?,
            mode: u32_le(buf, offset + 52)?,
            rdev: u64_le(buf, offset + 56)?,
            flags: u64_le(buf, offset + 64)?,
            sequence: u64_le(buf, offset + 72)?,
            reserved: [
                u64_le(buf, offset + 80)?,
                u64_le(buf, offset + 88)?,
                u64_le(buf, offset + 96)?,
                u64_le(buf, offset + 104)?,
            ],
            atime: BtrfsTimespec {
                sec: u64_le(buf, offset + 112)?,
                nsec: u32_le(buf, offset + 120)?,
            },
            ctime: BtrfsTimespec {
                sec: u64_le(buf, offset + 124)?,
                nsec: u32_le(buf, offset + 132)?,
            },
            mtime: BtrfsTimespec {
                sec: u64_le(buf, offset + 136)?,
                nsec: u32_le(buf, offset + 144)?,
            },
            otime: BtrfsTimespec {
                sec: u64_le(buf, offset + 152)?,
                nsec: u32_le(buf, offset + 160)?,
            },
        })
    }
}

#[derive(Debug, Clone)]
struct BtrfsDirItem {
    location: BtrfsKey,
    transid: u64,
    data_len: u16,
    name_len: u16,
    type_: u8,
}

impl BtrfsDirItem {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 33 > buf.len() {
            return None;
        }
        let location = BtrfsKey::parse(buf, offset)?;
        let transid = u64_le(buf, offset + 17)?;
        let data_len = u16_le(buf, offset + 25)?;
        let name_len = u16_le(buf, offset + 27)?;
        let type_ = u8_at(buf, offset + 29)?;
        Some(Self {
            location,
            transid,
            data_len,
            name_len,
            type_,
        })
    }
}

#[derive(Debug, Clone)]
struct BtrfsExtentItem {
    generation: u64,
    ram_bytes: u64,
}

impl BtrfsExtentItem {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 16 > buf.len() {
            return None;
        }
        Some(Self {
            generation: u64_le(buf, offset)?,
            ram_bytes: u64_le(buf, offset + 8)?,
        })
    }
}

pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    if volume.length < 0x10000 + 4096 {
        return None;
    }
    let buf = read_at_volume(src, volume, 0x10000, 4096).ok()?;
    if buf.len() < 4096 {
        return None;
    }
    let magic_pos = if &buf[256..264] == BTRFS_MAGIC {
        256
    } else if &buf[0x10000..0x10008] == BTRFS_MAGIC {
        0x10000
    } else {
        return None;
    };

    let sb = BtrfsSuperblock::parse(src, volume).ok()?;
    let mut details = std::collections::BTreeMap::new();
    details.insert("sectorsize".into(), sb.sectorsize.to_string());
    details.insert("nodesize".into(), sb.nodesize.to_string());
    details.insert("leafsize".into(), sb.leafsize.to_string());
    details.insert("stripesize".into(), sb.stripesize.to_string());
    details.insert("root_objectid".into(), sb.root.to_string());
    details.insert("chunk_root".into(), sb.chunk_root.to_string());
    details.insert("generation".into(), sb.generation.to_string());
    details.insert("total_bytes".into(), sb.total_bytes.to_string());
    details.insert("bytes_used".into(), sb.bytes_used.to_string());

    let mut warnings = sb.warnings.clone();
    warnings.push(
        "Btrfs metadata recovery is experimental; extent tree and device tree not fully parsed"
            .into(),
    );

    Some(FsInfo {
        kind: FsKind::Btrfs,
        label: None,
        cluster_size: Some(sb.sectorsize as u64),
        details,
        warnings,
    })
}

pub fn scan(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    options: &FsScanOptions,
    sink: &mut dyn CandidateSink,
) -> Result<FsSummary> {
    let Some(sb) = BtrfsSuperblock::parse(src, volume) else {
        return Ok(FsSummary {
            warnings: vec!["failed to parse Btrfs superblock".into()],
            ..Default::default()
        });
    };

    let mut summary = FsSummary::new(FsKind::Btrfs, None);
    let mut path_map: HashMap<u64, String> = HashMap::new();
    path_map.insert(sb.root_dir_objectid, "/".to_string());

    let nodesize = sb.nodesize as usize;
    let volume_offset = volume.offset;
    let block_size = sb.sectorsize as u64;

    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![sb.root];

    while let Some(tree_bytenr) = stack.pop() {
        if options.is_cancelled() {
            summary.truncated = true;
            break;
        }
        if !visited.insert(tree_bytenr) {
            continue;
        }
        if summary.emitted as usize >= options.max_candidates {
            summary.truncated = true;
            break;
        }

        let node_offset = tree_bytenr;
        if node_offset + nodesize as u64 > volume.length {
            continue;
        }
        let buf = read_at_volume(src, volume, node_offset, nodesize as u64)?;
        let Some(header) = BtrfsLeafHeader::parse(&buf) else {
            continue;
        };

        if header.level == 0 {
            let mut offset = 111;
            for _ in 0..header.nritems {
                if options.is_cancelled() {
                    summary.truncated = true;
                    break;
                }
                if summary.emitted as usize >= options.max_candidates {
                    summary.truncated = true;
                    break;
                }
                if offset + 25 > buf.len() {
                    break;
                }
                let Some(item) = BtrfsItem::parse(&buf, offset) else {
                    break;
                };
                offset += 25;

                let data_start = 111 + header.nritems as usize * 25 + item.data_offset as usize;
                if data_start + item.data_size as usize > buf.len() {
                    continue;
                }
                let data = &buf[data_start..data_start + item.data_size as usize];

                match item.key.type_ {
                    1 => {
                        if let Some(inode) = BtrfsInodeItem::parse(data, 0) {
                            if inode.nlink == 0 || options.include_live {
                                summary.deleted_entries += if inode.nlink == 0 { 1 } else { 0 };
                                summary.live_entries += if inode.nlink > 0 { 1 } else { 0 };

                                let path =
                                    build_btrfs_path(&path_map, inode.flags, item.key.objectid);
                                let name = format!("inode_{}", item.key.objectid);

                                emit_btrfs_candidate(
                                    sink,
                                    &mut summary,
                                    &name,
                                    &path,
                                    &inode,
                                    volume_offset,
                                    block_size,
                                    item.key.objectid,
                                    inode.nlink == 0,
                                )?;
                            }
                        }
                    }
                    0x80 => {
                        if let Some(dir_item) = BtrfsDirItem::parse(data, 0) {
                            if data.len() >= 33 + dir_item.name_len as usize {
                                let name_bytes = &data[33..33 + dir_item.name_len as usize];
                                if let Ok(name) = std::str::from_utf8(name_bytes) {
                                    let parent_path = path_map
                                        .get(&dir_item.location.objectid)
                                        .cloned()
                                        .unwrap_or_else(|| "/".to_string());
                                    let full_path = format!("{}/{}", parent_path, name);
                                    path_map.insert(dir_item.location.objectid, full_path);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            let item_size = 111 + header.nritems as usize * 25;
            if item_size + 8 * header.nritems as usize <= buf.len() {
                for i in 0..header.nritems {
                    let ptr_off = item_size + i as usize * 8;
                    if ptr_off + 8 <= buf.len() {
                        let child_bytenr = u64_le(&buf, ptr_off).unwrap_or(0);
                        if child_bytenr != 0 {
                            stack.push(child_bytenr);
                        }
                    }
                }
            }
        }
    }

    Ok(summary)
}

fn build_btrfs_path(path_map: &HashMap<u64, String>, flags: u64, objectid: u64) -> String {
    if flags & 0x1 != 0 {
        // Subvolume
        return format!("/subvol_{}", objectid);
    }
    path_map
        .get(&objectid)
        .cloned()
        .unwrap_or_else(|| format!("/inode_{}", objectid))
}

fn emit_btrfs_candidate(
    sink: &mut dyn CandidateSink,
    summary: &mut FsSummary,
    name: &str,
    path: &str,
    inode: &BtrfsInodeItem,
    volume_offset: u64,
    block_size: u64,
    inode_id: u64,
    is_deleted: bool,
) -> Result<()> {
    let total_size = inode.size;
    let data_location = DataLocation::Raw {
        offset: 0,
        length: total_size,
    };

    let confidence = if is_deleted {
        Confidence::new(
            ConfidenceLabel::Verified,
            75,
            vec![
                "Btrfs inode with nlink=0 (deleted)".into(),
                "Extent references may be preserved in tree".into(),
            ],
        )
    } else {
        Confidence::new(
            ConfidenceLabel::Verified,
            90,
            vec![
                "Btrfs live inode from tree".into(),
                "Full metadata available".into(),
            ],
        )
    };

    let mut candidate = Candidate::new(
        name.to_string(),
        total_size,
        "file",
        data_location,
        RecoveryMethod::Metadata,
        confidence,
        "btrfs",
    );
    candidate.path = path.to_string();
    candidate.state = if is_deleted {
        EntryState::Deleted
    } else {
        EntryState::Live
    };
    candidate.modified = Some(btrfs_time_to_iso(inode.mtime.sec, inode.mtime.nsec));
    candidate.accessed = Some(btrfs_time_to_iso(inode.atime.sec, inode.atime.nsec));
    candidate.created = Some(btrfs_time_to_iso(inode.otime.sec, inode.otime.nsec));
    candidate.mode = Some(inode.mode as u16);
    candidate.record_id = Some(format!("btrfs_inode_{}", inode_id));

    sink.emit(candidate)?;
    summary.emitted += 1;
    Ok(())
}

fn btrfs_time_to_iso(sec: u64, nsec: u32) -> String {
    if sec == 0 {
        return "1970-01-01T00:00:00Z".to_string();
    }
    let dt = chrono::DateTime::from_timestamp(sec as i64, nsec).unwrap_or_default();
    dt.to_rfc3339()
}
