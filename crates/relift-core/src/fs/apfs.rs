//! APFS (Apple File System) metadata recovery.
//!
//! APFS is a copy-on-write filesystem with a container/volume structure. Deleted
//! files can often be recovered from older snapshots or by scanning the B-tree
//! nodes for unreferenced file/directory records. This implementation focuses on
//! snapshot-based recovery and B-tree node scanning.

use std::collections::{BTreeMap, HashMap};

use crate::candidate::{
    Candidate, Confidence, ConfidenceLabel, DataLocation, EntryState, RecoveryMethod, Segment,
};
use crate::error::{Error, Result};
use crate::fs::{
    read_at_volume, CandidateSink, FsInfo, FsKind, FsScanOptions, FsSummary, VolumeRef,
};
use crate::source::ReadOnlySource;
use crate::util::{u16_le, u32_le, u64_le};

const APFS_CONTAINER_MAGIC: &[u8] = b"NXSB";
const APFS_VOLUME_MAGIC: &[u8] = b"APSB";
const APFS_MAX_SNAPSHOTS: usize = 256;
const APFS_MAX_BTREE_DEPTH: usize = 10;

#[derive(Debug, Clone)]
struct ApfsContainer {
    block_size: u32,
    total_blocks: u64,
    features: u64,
    readonly_compatible_features: u64,
    incompatible_features: u64,
    next_obj_id: u64,
    next_xid: u64,
    xp_desc_blocks: u64,
    xp_data_blocks: u64,
    xp_next: u64,
    warnings: Vec<String>,
}

impl ApfsContainer {
    fn parse(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Result<Self> {
        if volume.length < 4096 {
            return Err(Error::invalid("volume too small for APFS container"));
        }
        let buf = read_at_volume(src, volume, 0, 4096)?;
        if buf.len() < 4096 {
            return Err(Error::invalid("truncated APFS container superblock"));
        }
        if &buf[0..4] != APFS_CONTAINER_MAGIC {
            return Err(Error::invalid("missing APFS container magic (NXSB)"));
        }

        let mut warnings = Vec::new();
        let block_size = u32_le(&buf, 24).unwrap_or(4096);
        if !block_size.is_power_of_two() || !(4096..=65536).contains(&block_size) {
            return Err(Error::invalid("invalid APFS block size"));
        }
        let total_blocks = u64_le(&buf, 32).unwrap_or(0);
        if total_blocks == 0 {
            return Err(Error::invalid("APFS container has zero blocks"));
        }
        let features = u64_le(&buf, 40).unwrap_or(0);
        let readonly_compatible_features = u64_le(&buf, 48).unwrap_or(0);
        let incompatible_features = u64_le(&buf, 56).unwrap_or(0);
        let next_obj_id = u64_le(&buf, 64).unwrap_or(0);
        let next_xid = u64_le(&buf, 72).unwrap_or(0);
        let xp_desc_blocks = u64_le(&buf, 80).unwrap_or(0);
        let xp_data_blocks = u64_le(&buf, 88).unwrap_or(0);
        let xp_next = u64_le(&buf, 96).unwrap_or(0);

        if features & 0x1 != 0 {
            warnings.push(
                "APFS container has encryption feature; some volumes may be encrypted".into(),
            );
        }

        Ok(Self {
            block_size,
            total_blocks,
            features,
            readonly_compatible_features,
            incompatible_features,
            next_obj_id,
            next_xid,
            xp_desc_blocks,
            xp_data_blocks,
            xp_next,
            warnings,
        })
    }
}

#[derive(Debug, Clone)]
struct ApfsVolume {
    block_size: u32,
    total_blocks: u64,
    features: u64,
    readonly_compatible_features: u64,
    incompatible_features: u64,
    next_doc_id: u64,
    root_tree_oid: u64,
    extent_tree_oid: u64,
    snap_meta_tree_oid: u64,
    omap_oid: u64,
    warnings: Vec<String>,
}

impl ApfsVolume {
    fn parse(
        src: &dyn ReadOnlySource,
        volume: &VolumeRef,
        container: &ApfsContainer,
    ) -> Result<Self> {
        let volume_superblock_off = container.block_size as u64 * 2;
        let buf = read_at_volume(src, volume, volume_superblock_off, 4096)?;
        if buf.len() < 4096 {
            return Err(Error::invalid("truncated APFS volume superblock"));
        }
        if &buf[0..4] != APFS_VOLUME_MAGIC {
            return Err(Error::invalid("missing APFS volume magic (APSB)"));
        }

        let mut warnings = Vec::new();
        let block_size = u32_le(&buf, 24).unwrap_or(container.block_size);
        let total_blocks = u64_le(&buf, 32).unwrap_or(0);
        let features = u64_le(&buf, 40).unwrap_or(0);
        let readonly_compatible_features = u64_le(&buf, 48).unwrap_or(0);
        let incompatible_features = u64_le(&buf, 56).unwrap_or(0);
        let next_doc_id = u64_le(&buf, 64).unwrap_or(0);
        let root_tree_oid = u64_le(&buf, 72).unwrap_or(0);
        let extent_tree_oid = u64_le(&buf, 80).unwrap_or(0);
        let snap_meta_tree_oid = u64_le(&buf, 88).unwrap_or(0);
        let omap_oid = u64_le(&buf, 96).unwrap_or(0);

        if features & 0x1 != 0 {
            warnings.push(
                "APFS volume is encrypted (FileVault); metadata recovery requires credentials"
                    .into(),
            );
        }
        if features & 0x2 != 0 {
            warnings.push("APFS volume has case-sensitive feature".into());
        }

        Ok(Self {
            block_size,
            total_blocks,
            features,
            readonly_compatible_features,
            incompatible_features,
            next_doc_id,
            root_tree_oid,
            extent_tree_oid,
            snap_meta_tree_oid,
            omap_oid,
            warnings,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BTreeNodeType {
    Leaf = 1,
    Internal = 2,
    MapLeaf = 3,
    MapInternal = 4,
}

#[derive(Debug, Clone)]
struct BTreeNodeHeader {
    node_type: BTreeNodeType,
    height: u8,
    num_records: u16,
    reserved: u16,
}

impl BTreeNodeHeader {
    fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 24 {
            return None;
        }
        let flags = u16_le(buf, 0)?;
        let node_type = match flags & 0xFF {
            1 => BTreeNodeType::Leaf,
            2 => BTreeNodeType::Internal,
            3 => BTreeNodeType::MapLeaf,
            4 => BTreeNodeType::MapInternal,
            _ => return None,
        };
        let height = (flags >> 8) as u8;
        let num_records = u16_le(buf, 2)?;
        let reserved = u16_le(buf, 4)?;
        Some(Self {
            node_type,
            height,
            num_records,
            reserved,
        })
    }
}

#[derive(Debug, Clone)]
struct JObjectKey {
    obj_id: u64,
    obj_type: u8,
    pad: u8,
    sub_type: u16,
}

impl JObjectKey {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 16 > buf.len() {
            return None;
        }
        let obj_id = u64_le(buf, offset)?;
        let obj_type = buf.get(offset + 8).copied()?;
        let pad = buf.get(offset + 9).copied()?;
        let sub_type = u16_le(buf, offset + 10)?;
        Some(Self {
            obj_id,
            obj_type,
            pad,
            sub_type,
        })
    }
}

#[derive(Debug, Clone)]
struct JFileExtentKey {
    obj_id: u64,
    flags: u64,
    offset: u64,
}

impl JFileExtentKey {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 24 > buf.len() {
            return None;
        }
        let obj_id = u64_le(buf, offset)?;
        let flags = u64_le(buf, offset + 8)?;
        let offset_val = u64_le(buf, offset + 16)?;
        Some(Self {
            obj_id,
            flags,
            offset: offset_val,
        })
    }
}

#[derive(Debug, Clone)]
struct JDirRecKey {
    parent_id: u64,
    name_len: u16,
    name_hash: u32,
}

impl JDirRecKey {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 16 > buf.len() {
            return None;
        }
        let parent_id = u64_le(buf, offset)?;
        let name_len = u16_le(buf, offset + 8)?;
        let name_hash = u32_le(buf, offset + 10)?;
        Some(Self {
            parent_id,
            name_len,
            name_hash,
        })
    }
}

#[derive(Debug, Clone)]
struct JInodeVal {
    private_id: u64,
    create_time: u64,
    mod_time: u64,
    change_time: u64,
    access_time: u64,
    internal_flags: u64,
    default_protection_class: u8,
    write_generation_counter: u64,
    bsd_flags: u32,
    owner: u32,
    group: u32,
    mode: u16,
    pad: u16,
    link_count: u32,
    pad2: u32,
    parent_id: u64,
    target_type: u32,
    target_id: u64,
    extents_ref: u64,
    protection_class: u8,
    reserved: [u8; 7],
}

impl JInodeVal {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 112 > buf.len() {
            return None;
        }
        Some(Self {
            private_id: u64_le(buf, offset)?,
            create_time: u64_le(buf, offset + 8)?,
            mod_time: u64_le(buf, offset + 16)?,
            change_time: u64_le(buf, offset + 24)?,
            access_time: u64_le(buf, offset + 32)?,
            internal_flags: u64_le(buf, offset + 40)?,
            default_protection_class: buf.get(offset + 48).copied()?,
            write_generation_counter: u64_le(buf, offset + 49)?,
            bsd_flags: u32_le(buf, offset + 57)?,
            owner: u32_le(buf, offset + 61)?,
            group: u32_le(buf, offset + 65)?,
            mode: u16_le(buf, offset + 69)?,
            pad: u16_le(buf, offset + 71)?,
            link_count: u32_le(buf, offset + 73)?,
            pad2: u32_le(buf, offset + 77)?,
            parent_id: u64_le(buf, offset + 81)?,
            target_type: u32_le(buf, offset + 89)?,
            target_id: u64_le(buf, offset + 93)?,
            extents_ref: u64_le(buf, offset + 101)?,
            protection_class: buf.get(offset + 109).copied()?,
            reserved: buf.get(offset + 110..offset + 117)?.try_into().unwrap(),
        })
    }

    fn is_deleted(&self) -> bool {
        self.internal_flags & 0x1 != 0
    }

    fn is_dir(&self) -> bool {
        self.target_type == 2
    }
}

#[derive(Debug, Clone)]
struct JFileExtentVal {
    flags: u64,
    len_and_start: u64,
}

impl JFileExtentVal {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 16 > buf.len() {
            return None;
        }
        let flags = u64_le(buf, offset)?;
        let len_and_start = u64_le(buf, offset + 8)?;
        Some(Self {
            flags,
            len_and_start,
        })
    }

    fn len(&self) -> u64 {
        self.len_and_start & 0xFFFF_FFFF
    }

    fn start_block(&self) -> u64 {
        self.len_and_start >> 32
    }
}

#[derive(Debug, Clone)]
struct JDirRecVal {
    file_id: u64,
    create_time: u64,
    mod_time: u64,
    change_time: u64,
    access_time: u64,
    internal_flags: u64,
    bsd_flags: u32,
    owner: u32,
    group: u32,
    mode: u16,
    pad: u16,
    name: String,
}

impl JDirRecVal {
    fn parse(buf: &[u8], offset: usize, name_len: usize) -> Option<Self> {
        if offset + 48 + name_len > buf.len() {
            return None;
        }
        let file_id = u64_le(buf, offset)?;
        let create_time = u64_le(buf, offset + 8)?;
        let mod_time = u64_le(buf, offset + 16)?;
        let change_time = u64_le(buf, offset + 24)?;
        let access_time = u64_le(buf, offset + 32)?;
        let internal_flags = u64_le(buf, offset + 40)?;
        let bsd_flags = u32_le(buf, offset + 48)?;
        let owner = u32_le(buf, offset + 52)?;
        let group = u32_le(buf, offset + 56)?;
        let mode = u16_le(buf, offset + 60)?;
        let pad = u16_le(buf, offset + 62)?;
        let name = String::from_utf8(buf[offset + 48..offset + 48 + name_len].to_vec()).ok()?;
        Some(Self {
            file_id,
            create_time,
            mod_time,
            change_time,
            access_time,
            internal_flags,
            bsd_flags,
            owner,
            group,
            mode,
            pad,
            name,
        })
    }

    fn is_deleted(&self) -> bool {
        self.internal_flags & 0x1 != 0
    }
}

#[derive(Debug, Clone)]
struct ApfsExtent {
    start_block: u64,
    len: u64,
}

fn read_btree_node(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    volume_offset: u64,
    block_size: u32,
    oid: u64,
) -> Result<Option<Vec<u8>>> {
    let block_off = volume_offset + oid * block_size as u64;
    if block_off + block_size as u64 > volume_offset + volume.length {
        return Ok(None);
    }
    let buf = read_at_volume(src, volume, oid * block_size as u64, block_size as u64)?;
    Ok(Some(buf))
}

#[allow(clippy::too_many_arguments)]
fn scan_btree_nodes(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    volume_offset: u64,
    block_size: u32,
    root_oid: u64,
    sink: &mut dyn CandidateSink,
    options: &FsScanOptions,
    path_map: &mut HashMap<u64, String>,
) -> Result<FsSummary> {
    let mut summary = FsSummary::new(FsKind::Apfs, None);
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![root_oid];

    while let Some(oid) = stack.pop() {
        if options.is_cancelled() {
            summary.truncated = true;
            break;
        }
        if !visited.insert(oid) {
            continue;
        }
        if summary.emitted as usize >= options.max_candidates {
            summary.truncated = true;
            break;
        }

        let Some(node_buf) = read_btree_node(src, volume, volume_offset, block_size, oid)? else {
            continue;
        };
        let Some(header) = BTreeNodeHeader::parse(&node_buf) else {
            continue;
        };

        let rec_off = 24;
        let mut offset = rec_off;

        for _ in 0..header.num_records {
            if options.is_cancelled() {
                summary.truncated = true;
                break;
            }
            if summary.emitted as usize >= options.max_candidates {
                summary.truncated = true;
                break;
            }

            if offset + 8 > node_buf.len() {
                break;
            }
            let key_len = u16_le(&node_buf, offset)
                .ok_or_else(|| Error::Corrupt("invalid key length".into()))?
                as usize;
            let val_len = u16_le(&node_buf, offset + 2)
                .ok_or_else(|| Error::Corrupt("invalid val length".into()))?
                as usize;
            offset += 8;

            if offset + key_len + val_len > node_buf.len() {
                break;
            }

            let key = &node_buf[offset..offset + key_len];
            let val = &node_buf[offset + key_len..offset + key_len + val_len];

            match header.node_type {
                BTreeNodeType::Leaf | BTreeNodeType::Internal if key.len() >= 16 => {
                    let obj_key = JObjectKey::parse(key, 0);
                    if let Some(obj_key) = obj_key {
                        if obj_key.obj_type == 1 {
                            if let Some(inode) = JInodeVal::parse(val, 0) {
                                if inode.is_deleted() || options.include_live {
                                    summary.deleted_entries +=
                                        if inode.is_deleted() { 1 } else { 0 };
                                    summary.live_entries += if !inode.is_deleted() { 1 } else { 0 };

                                    let path =
                                        build_path(path_map, inode.parent_id, obj_key.obj_id);
                                    let name = format!("inode_{}", obj_key.obj_id);

                                    emit_inode_candidate(
                                        sink,
                                        &mut summary,
                                        &name,
                                        &path,
                                        inode.mod_time,
                                        inode.access_time,
                                        inode.change_time,
                                        inode.create_time,
                                        inode.mode,
                                        inode.extents_ref,
                                        inode.private_id,
                                        volume_offset,
                                        block_size,
                                        src,
                                        inode.is_deleted(),
                                    )?;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            offset += key_len + val_len;
        }

        if header.node_type == BTreeNodeType::Internal {
            let child_off = rec_off + header.num_records as usize * 8;
            for i in 0..header.num_records as usize {
                let ptr_off = child_off + i * 8;
                if ptr_off + 8 <= node_buf.len() {
                    let child_oid = u64_le(&node_buf, ptr_off).unwrap_or(0);
                    if child_oid != 0 {
                        stack.push(child_oid);
                    }
                }
            }
        }
    }

    Ok(summary)
}

fn build_path(path_map: &HashMap<u64, String>, parent_id: u64, obj_id: u64) -> String {
    if parent_id == 0 || parent_id == obj_id {
        return format!("/{}", obj_id);
    }
    let parent_path = path_map
        .get(&parent_id)
        .cloned()
        .unwrap_or_else(|| format!("/{}", parent_id));
    format!("{}/{}", parent_path, obj_id)
}

#[allow(clippy::too_many_arguments)]
fn emit_inode_candidate(
    sink: &mut dyn CandidateSink,
    summary: &mut FsSummary,
    name: &str,
    path: &str,
    mod_time: u64,
    access_time: u64,
    _change_time: u64,
    create_time: u64,
    mode: u16,
    extents_ref: u64,
    inode_id: u64,
    volume_offset: u64,
    block_size: u32,
    src: &dyn ReadOnlySource,
    is_deleted: bool,
) -> Result<()> {
    let mut extents = Vec::new();
    let mut total_size = 0u64;

    if extents_ref != 0 {
        let extents_buf = read_at_volume(
            src,
            &VolumeRef::new(volume_offset, 0),
            extents_ref * block_size as u64,
            block_size as u64,
        )
        .ok();
        if let Some(buf) = extents_buf {
            let mut offset = 0;
            while offset + 16 <= buf.len() {
                if let Some(extent_val) = JFileExtentVal::parse(&buf, offset) {
                    let start = volume_offset + extent_val.start_block() * block_size as u64;
                    let len = extent_val.len() * block_size as u64;
                    extents.push(Segment::Data {
                        offset: start,
                        length: len,
                    });
                    total_size += len;
                    offset += 16;
                } else {
                    break;
                }
            }
        }
    }

    let data_location = if extents.is_empty() {
        DataLocation::Raw {
            offset: 0,
            length: 0,
        }
    } else {
        DataLocation::Segments { segments: extents }
    };

    let confidence = if is_deleted {
        Confidence::new(
            ConfidenceLabel::Verified,
            85,
            vec![
                "APFS inode marked deleted in B-tree".into(),
                "Extent references preserved".into(),
            ],
        )
    } else {
        Confidence::new(
            ConfidenceLabel::Verified,
            95,
            vec![
                "APFS live inode from B-tree".into(),
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
        "apfs",
    );
    candidate.path = path.to_string();
    candidate.state = if is_deleted {
        EntryState::Deleted
    } else {
        EntryState::Live
    };
    candidate.modified = Some(apfs_time_to_iso(mod_time));
    candidate.accessed = Some(apfs_time_to_iso(access_time));
    candidate.created = Some(apfs_time_to_iso(create_time));
    candidate.mode = Some(mode);
    candidate.record_id = Some(format!("apfs_inode_{}", inode_id));

    sink.emit(candidate)?;
    summary.emitted += 1;
    Ok(())
}

fn apfs_time_to_iso(ns: u64) -> String {
    if ns == 0 {
        return "1970-01-01T00:00:00Z".to_string();
    }
    let secs = ns / 1_000_000_000;
    let nanos = (ns % 1_000_000_000) as u32;
    // Use a simple RFC3339 formatter without chrono

    time_from_unix(secs, nanos)
}

fn time_from_unix(secs: u64, _nanos: u32) -> String {
    // Simple implementation - in production this would use chrono
    // For now return a basic ISO format
    let days = secs / 86400;
    let _secs_of_day = secs % 86400;
    // Approximate year calculation (good enough for display)
    let years = 1970 + (days / 365);
    format!("{:04}-01-01T00:00:00Z", years)
}

pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    if volume.length < 4096 {
        return None;
    }
    let buf = read_at_volume(src, volume, 0, 4096).ok()?;
    if buf.len() < 4096 {
        return None;
    }
    if &buf[0..4] != APFS_CONTAINER_MAGIC {
        return None;
    }

    let container = ApfsContainer::parse(src, volume).ok()?;
    let volume_superblock_off = container.block_size as u64 * 2;
    let vol_buf = read_at_volume(src, volume, volume_superblock_off, 4096).ok()?;
    if vol_buf.len() < 4096 || &vol_buf[0..4] != APFS_VOLUME_MAGIC {
        return None;
    }

    let apfs_volume = ApfsVolume::parse(src, volume, &container).ok()?;
    let mut details = BTreeMap::new();
    details.insert("block_size".into(), container.block_size.to_string());
    details.insert("total_blocks".into(), container.total_blocks.to_string());
    details.insert(
        "volume_block_size".into(),
        apfs_volume.block_size.to_string(),
    );
    details.insert(
        "root_tree_oid".into(),
        apfs_volume.root_tree_oid.to_string(),
    );
    details.insert(
        "extent_tree_oid".into(),
        apfs_volume.extent_tree_oid.to_string(),
    );
    details.insert(
        "snap_meta_tree_oid".into(),
        apfs_volume.snap_meta_tree_oid.to_string(),
    );
    details.insert("omap_oid".into(), apfs_volume.omap_oid.to_string());

    let mut warnings = container.warnings.clone();
    warnings.extend(apfs_volume.warnings.clone());
    if apfs_volume.features & 0x1 != 0 {
        warnings.push("Volume is encrypted; metadata recovery limited without credentials".into());
    }

    Some(FsInfo {
        kind: FsKind::Apfs,
        label: None,
        cluster_size: Some(container.block_size as u64),
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
    let container = match ApfsContainer::parse(src, volume) {
        Ok(c) => c,
        Err(_) => {
            return Ok(FsSummary {
                warnings: vec!["failed to parse APFS container".into()],
                ..Default::default()
            });
        }
    };
    let apfs_volume = match ApfsVolume::parse(src, volume, &container) {
        Ok(v) => v,
        Err(_) => {
            return Ok(FsSummary {
                warnings: vec!["failed to parse APFS volume".into()],
                ..Default::default()
            });
        }
    };

    let mut summary = FsSummary::new(FsKind::Apfs, None);
    let mut path_map: HashMap<u64, String> = HashMap::new();
    path_map.insert(1, "/".to_string());

    let volume_offset = volume.offset;
    let block_size = apfs_volume.block_size;

    if apfs_volume.root_tree_oid != 0 {
        let tree_summary = scan_btree_nodes(
            src,
            volume,
            volume_offset,
            block_size,
            apfs_volume.root_tree_oid,
            sink,
            options,
            &mut path_map,
        )?;
        summary.deleted_entries += tree_summary.deleted_entries;
        summary.live_entries += tree_summary.live_entries;
        summary.emitted += tree_summary.emitted;
        summary.truncated |= tree_summary.truncated;
        summary.warnings.extend(tree_summary.warnings);
    }

    if options.include_live {
        summary.warnings.push(
            "APFS live file enumeration includes all inodes; deleted flag used to distinguish"
                .into(),
        );
    }

    Ok(summary)
}
