//! HFS+ (Mac OS Extended) metadata recovery.
//!
//! HFS+ uses B-trees for its catalog and extents overflow. Deleted files leave
//! entries in the catalog B-tree with a "deleted" flag, and their extents may
//! remain in the extents overflow B-tree until reused.

use std::collections::HashMap;

use crate::candidate::{
    Candidate, Confidence, ConfidenceLabel, DataLocation, EntryState, RecoveryMethod, Segment,
};
use crate::error::{Error, Result};
use crate::fs::{
    read_at_volume, CandidateSink, FsInfo, FsKind, FsScanOptions, FsSummary, VolumeRef,
};
use crate::source::ReadOnlySource;
use crate::util::{u16_be, u32_be, u64_be, u8_at};

const HFS_PLUS_MAGIC: u16 = 0x482B; // 'H+'
const HFSX_MAGIC: u16 = 0x4858; // 'HX'
const HFS_MIN_SIZE: u64 = 512 * 1024; // 512 KB minimum

#[derive(Debug, Clone)]
struct HfsPlusVolumeHeader {
    signature: u16,
    version: u16,
    attributes: u32,
    last_mounted_version: u32,
    journal_info_block: u32,
    create_date: u32,
    modify_date: u32,
    backup_date: u32,
    checked_date: u32,
    file_count: u32,
    folder_count: u32,
    block_size: u32,
    total_blocks: u32,
    free_blocks: u32,
    next_allocation: u32,
    rsrc_clump_sz: u32,
    data_clump_sz: u32,
    next_cnid: u32,
    write_count: u32,
    encodings_bitmap: u64,
    finder_info: [u32; 8],
    allocation_file: BTreeRef,
    extents_file: BTreeRef,
    catalog_file: BTreeRef,
    attributes_file: BTreeRef,
    startup_file: BTreeRef,
}

#[derive(Debug, Clone, Default)]
struct BTreeRef {
    tree_depth: u16,
    root_node: u32,
    leaf_records: u32,
    first_leaf_node: u32,
    last_leaf_node: u32,
    node_size: u16,
    max_key_length: u16,
    total_nodes: u32,
    free_nodes: u32,
    reserved1: u16,
    reserved2: u32,
    clump_size: u32,
    btree_type: u8,
    key_compare_type: u8,
    attributes: u32,
}

impl HfsPlusVolumeHeader {
    fn parse(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Result<Self> {
        if volume.length < 1024 {
            return Err(Error::invalid("volume too small for HFS+ header"));
        }
        let buf = read_at_volume(src, volume, 1024, 512)?;
        if buf.len() < 512 {
            return Err(Error::invalid("truncated HFS+ volume header"));
        }
        let signature = u16_be(&buf, 0).unwrap_or(0);
        if signature != HFS_PLUS_MAGIC && signature != HFSX_MAGIC {
            return Err(Error::invalid("missing HFS+ signature"));
        }

        Ok(Self {
            signature,
            version: u16_be(&buf, 2).unwrap_or(0),
            attributes: u32_be(&buf, 4).unwrap_or(0),
            last_mounted_version: u32_be(&buf, 8).unwrap_or(0),
            journal_info_block: u32_be(&buf, 12).unwrap_or(0),
            create_date: u32_be(&buf, 16).unwrap_or(0),
            modify_date: u32_be(&buf, 20).unwrap_or(0),
            backup_date: u32_be(&buf, 24).unwrap_or(0),
            checked_date: u32_be(&buf, 28).unwrap_or(0),
            file_count: u32_be(&buf, 32).unwrap_or(0),
            folder_count: u32_be(&buf, 36).unwrap_or(0),
            block_size: u32_be(&buf, 40).unwrap_or(4096),
            total_blocks: u32_be(&buf, 44).unwrap_or(0),
            free_blocks: u32_be(&buf, 48).unwrap_or(0),
            next_allocation: u32_be(&buf, 52).unwrap_or(0),
            rsrc_clump_sz: u32_be(&buf, 56).unwrap_or(0),
            data_clump_sz: u32_be(&buf, 60).unwrap_or(0),
            next_cnid: u32_be(&buf, 64).unwrap_or(0),
            write_count: u32_be(&buf, 68).unwrap_or(0),
            encodings_bitmap: u64_be(&buf, 72).unwrap_or(0),
            finder_info: [
                u32_be(&buf, 80).unwrap_or(0),
                u32_be(&buf, 84).unwrap_or(0),
                u32_be(&buf, 88).unwrap_or(0),
                u32_be(&buf, 92).unwrap_or(0),
                u32_be(&buf, 96).unwrap_or(0),
                u32_be(&buf, 100).unwrap_or(0),
                u32_be(&buf, 104).unwrap_or(0),
                u32_be(&buf, 108).unwrap_or(0),
            ],
            allocation_file: BTreeRef::parse(&buf, 112)
                .ok_or_else(|| Error::Corrupt("invalid allocation file btree".into()))?,
            extents_file: BTreeRef::parse(&buf, 160)
                .ok_or_else(|| Error::Corrupt("invalid extents file btree".into()))?,
            catalog_file: BTreeRef::parse(&buf, 208)
                .ok_or_else(|| Error::Corrupt("invalid catalog file btree".into()))?,
            attributes_file: BTreeRef::parse(&buf, 256)
                .ok_or_else(|| Error::Corrupt("invalid attributes file btree".into()))?,
            startup_file: BTreeRef::parse(&buf, 304)
                .ok_or_else(|| Error::Corrupt("invalid startup file btree".into()))?,
        })
    }
}

impl BTreeRef {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 48 > buf.len() {
            return None;
        }
        Some(Self {
            tree_depth: u16_be(buf, offset)?,
            root_node: u32_be(buf, offset + 2)?,
            leaf_records: u32_be(buf, offset + 4)?,
            first_leaf_node: u32_be(buf, offset + 8)?,
            last_leaf_node: u32_be(buf, offset + 12)?,
            node_size: u16_be(buf, offset + 14)?,
            max_key_length: u16_be(buf, offset + 16)?,
            total_nodes: u32_be(buf, offset + 18)?,
            free_nodes: u32_be(buf, offset + 22)?,
            reserved1: u16_be(buf, offset + 26)?,
            reserved2: u32_be(buf, offset + 28)?,
            clump_size: u32_be(buf, offset + 32)?,
            btree_type: u8_at(buf, offset + 36)?,
            key_compare_type: u8_at(buf, offset + 37)?,
            attributes: u32_be(buf, offset + 38)?,
        })
    }
}

#[derive(Debug, Clone)]
struct CatalogKey {
    parent_cnid: u32,
    node_name: String,
}

impl CatalogKey {
    fn parse(buf: &[u8], offset: usize, key_len: usize) -> Option<Self> {
        if offset + 6 + key_len > buf.len() {
            return None;
        }
        let parent_cnid = u32_be(buf, offset)?;
        let _reserved = u16_be(buf, offset + 4)?;
        let name_len = u16_be(buf, offset + 6)? as usize;
        if offset + 8 + name_len > buf.len() {
            return None;
        }
        let name_bytes = &buf[offset + 8..offset + 8 + name_len];
        let (chunks, _) = name_bytes.as_chunks::<2>();
        let name = String::from_utf16(
            &chunks
                .iter()
                .map(|c| u16_be(&c[..], 0).unwrap_or(0))
                .collect::<Vec<_>>(),
        )
        .ok()?;
        Some(Self {
            parent_cnid,
            node_name: name,
        })
    }
}

#[derive(Debug, Clone)]
enum CatalogRecord {
    Folder(ThreadRecord, FolderRecord),
    File(ThreadRecord, FileRecord),
    FolderThread(ThreadRecord),
    FileThread(ThreadRecord),
}

#[derive(Debug, Clone)]
struct ThreadRecord {
    reserved: u16,
    parent_cnid: u32,
    node_name: String,
}

impl ThreadRecord {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 10 > buf.len() {
            return None;
        }
        let reserved = u16_be(buf, offset)?;
        let parent_cnid = u32_be(buf, offset + 2)?;
        let name_len = u16_be(buf, offset + 6)? as usize;
        if offset + 8 + name_len > buf.len() {
            return None;
        }
        let name_bytes = &buf[offset + 8..offset + 8 + name_len];
        let (chunks, _) = name_bytes.as_chunks::<2>();
        let name = String::from_utf16(
            &chunks
                .iter()
                .map(|c| u16_be(&c[..], 0).unwrap_or(0))
                .collect::<Vec<_>>(),
        )
        .ok()?;
        Some(Self {
            reserved,
            parent_cnid,
            node_name: name,
        })
    }
}

#[derive(Debug, Clone)]
struct FolderRecord {
    flags: u16,
    valence: u32,
    cnid: u32,
    create_date: u32,
    content_mod_date: u32,
    attribute_mod_date: u32,
    access_date: u32,
    backup_date: u32,
    permissions: Permissions,
    user_info: FolderUserInfo,
    finder_info: [u32; 16],
    text_encoding: u32,
    reserved: u32,
}

#[derive(Debug, Clone)]
struct Permissions {
    owner_id: u32,
    group_id: u32,
    admin_flags: u32,
    owner_flags: u32,
    mode: u16,
    reserved: u16,
}

#[derive(Debug, Clone)]
struct FolderUserInfo {
    window: [i16; 4],
    reserved: [i32; 4],
}

#[derive(Debug, Clone)]
struct FileRecord {
    flags: u16,
    file_type: u32,
    file_creator: u32,
    cnid: u32,
    create_date: u32,
    content_mod_date: u32,
    attribute_mod_date: u32,
    access_date: u32,
    backup_date: u32,
    permissions: Permissions,
    user_info: FileUserInfo,
    finder_info: [u32; 16],
    text_encoding: u32,
    reserved: u32,
    data_fork: ForkData,
    resource_fork: ForkData,
}

#[derive(Debug, Clone)]
struct FileUserInfo {
    window: [i16; 4],
    reserved: [i32; 4],
}

#[derive(Debug, Clone)]
struct ForkData {
    logical_size: u64,
    clump_size: u32,
    total_blocks: u32,
    extents: [ExtentDescriptor; 8],
}

#[derive(Debug, Clone, Copy)]
struct ExtentDescriptor {
    start_block: u32,
    block_count: u32,
}

impl ExtentDescriptor {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 8 > buf.len() {
            return None;
        }
        Some(Self {
            start_block: u32_be(buf, offset)?,
            block_count: u32_be(buf, offset + 4)?,
        })
    }
}

impl FileRecord {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 200 > buf.len() {
            return None;
        }
        let flags = u16_be(buf, offset)?;
        let file_type = u32_be(buf, offset + 2)?;
        let file_creator = u32_be(buf, offset + 6)?;
        let cnid = u32_be(buf, offset + 10)?;
        let create_date = u32_be(buf, offset + 14)?;
        let content_mod_date = u32_be(buf, offset + 18)?;
        let attribute_mod_date = u32_be(buf, offset + 22)?;
        let access_date = u32_be(buf, offset + 26)?;
        let backup_date = u32_be(buf, offset + 30)?;

        let permissions = Permissions {
            owner_id: u32_be(buf, offset + 34)?,
            group_id: u32_be(buf, offset + 38)?,
            admin_flags: u32_be(buf, offset + 42)?,
            owner_flags: u32_be(buf, offset + 46)?,
            mode: u16_be(buf, offset + 50)?,
            reserved: u16_be(buf, offset + 52)?,
        };

        let user_info = FileUserInfo {
            window: [
                i16::from_be_bytes(buf.get(offset + 54..offset + 56)?.try_into().ok()?),
                i16::from_be_bytes(buf.get(offset + 56..offset + 58)?.try_into().ok()?),
                i16::from_be_bytes(buf.get(offset + 58..offset + 60)?.try_into().ok()?),
                i16::from_be_bytes(buf.get(offset + 60..offset + 62)?.try_into().ok()?),
            ],
            reserved: [
                i32::from_be_bytes(buf.get(offset + 62..offset + 66)?.try_into().ok()?),
                i32::from_be_bytes(buf.get(offset + 66..offset + 70)?.try_into().ok()?),
                i32::from_be_bytes(buf.get(offset + 70..offset + 74)?.try_into().ok()?),
                i32::from_be_bytes(buf.get(offset + 74..offset + 78)?.try_into().ok()?),
            ],
        };

        let mut finder_info = [0u32; 16];
        for (i, slot) in finder_info.iter_mut().enumerate() {
            *slot = u32_be(buf, offset + 78 + i * 4)?;
        }

        let text_encoding = u32_be(buf, offset + 142)?;
        let reserved = u32_be(buf, offset + 146)?;

        let data_fork = ForkData::parse(buf, offset + 150)?;
        let resource_fork = ForkData::parse(buf, offset + 230)?;

        Some(Self {
            flags,
            file_type,
            file_creator,
            cnid,
            create_date,
            content_mod_date,
            attribute_mod_date,
            access_date,
            backup_date,
            permissions,
            user_info,
            finder_info,
            text_encoding,
            reserved,
            data_fork,
            resource_fork,
        })
    }

    fn is_deleted(&self) -> bool {
        self.flags & 0x0001 != 0
    }
}

impl ForkData {
    fn parse(buf: &[u8], offset: usize) -> Option<Self> {
        if offset + 80 > buf.len() {
            return None;
        }
        let logical_size = u64_be(buf, offset)?;
        let clump_size = u32_be(buf, offset + 8)?;
        let total_blocks = u32_be(buf, offset + 12)?;
        let mut extents = [ExtentDescriptor {
            start_block: 0,
            block_count: 0,
        }; 8];
        for (i, slot) in extents.iter_mut().enumerate() {
            if let Some(e) = ExtentDescriptor::parse(buf, offset + 16 + i * 8) {
                *slot = e;
            }
        }
        Some(Self {
            logical_size,
            clump_size,
            total_blocks,
            extents,
        })
    }
}

#[derive(Debug, Clone)]
struct BTreeNode {
    node_type: u8,
    height: u8,
    num_records: u16,
    data: Vec<u8>,
}

impl BTreeNode {
    fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < 14 {
            return None;
        }
        let _f_link = u32_be(buf, 0)?;
        let _b_link = u32_be(buf, 4)?;
        let node_type = u8_at(buf, 8)?;
        let height = u8_at(buf, 9)?;
        let num_records = u16_be(buf, 10)?;
        let _reserved = u16_be(buf, 12)?;
        Some(Self {
            node_type,
            height,
            num_records,
            data: buf[14..].to_vec(),
        })
    }
}

pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    if volume.length < 2048 {
        return None;
    }
    let buf = read_at_volume(src, volume, 1024, 512).ok()?;
    if buf.len() < 512 {
        return None;
    }
    let signature = u16_be(&buf, 0)?;
    if signature != HFS_PLUS_MAGIC && signature != HFSX_MAGIC {
        return None;
    }

    let header = HfsPlusVolumeHeader::parse(src, volume).ok()?;
    let mut details = std::collections::BTreeMap::new();
    details.insert("version".into(), header.version.to_string());
    details.insert("block_size".into(), header.block_size.to_string());
    details.insert("total_blocks".into(), header.total_blocks.to_string());
    details.insert("file_count".into(), header.file_count.to_string());
    details.insert("folder_count".into(), header.folder_count.to_string());
    details.insert(
        "catalog_node_size".into(),
        header.catalog_file.node_size.to_string(),
    );
    details.insert(
        "extents_node_size".into(),
        header.extents_file.node_size.to_string(),
    );

    let mut warnings = Vec::new();
    if header.attributes & 0x80000000 != 0 {
        warnings.push("HFS+ volume is case-sensitive (HFSX)".into());
    }
    if header.journal_info_block != 0 {
        warnings.push("HFS+ journal present; not yet replayed".into());
    }
    warnings.push(
        "HFS+ metadata recovery is experimental; extents overflow B-tree not fully parsed".into(),
    );

    Some(FsInfo {
        kind: FsKind::HfsPlus,
        label: None,
        cluster_size: Some(header.block_size as u64),
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
    let header = match HfsPlusVolumeHeader::parse(src, volume) {
        Ok(h) => h,
        Err(_) => {
            return Ok(FsSummary {
                warnings: vec!["failed to parse HFS+ volume header".into()],
                ..Default::default()
            });
        }
    };

    let mut summary = FsSummary::new(FsKind::HfsPlus, None);
    let mut path_map: HashMap<u32, String> = HashMap::new();
    path_map.insert(1, "/".to_string()); // Root folder CNID is typically 2, but 1 is parent of root

    let catalog_ref = header.catalog_file;
    let node_size = catalog_ref.node_size as usize;
    if !(512..=32768).contains(&node_size) || !node_size.is_power_of_two() {
        summary.warnings.push("invalid catalog node size".into());
        return Ok(summary);
    }

    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![catalog_ref.root_node];

    let volume_offset = volume.offset;
    let block_size = header.block_size;

    while let Some(node_num) = stack.pop() {
        if options.is_cancelled() {
            summary.truncated = true;
            break;
        }
        if !visited.insert(node_num) {
            continue;
        }
        if summary.emitted as usize >= options.max_candidates {
            summary.truncated = true;
            break;
        }

        let node_offset = volume_offset + node_num as u64 * block_size as u64;
        if node_offset + node_size as u64 > volume_offset + volume.length {
            continue;
        }
        let buf = read_at_volume(
            src,
            volume,
            node_num as u64 * block_size as u64,
            node_size as u64,
        )?;
        let Some(node) = BTreeNode::parse(&buf) else {
            continue;
        };

        if node.node_type == 0xFF || node.node_type == 0x00 {
            summary
                .warnings
                .push("encountered empty/free catalog node".into());
            continue;
        }

        let _offset = 0;
        let record_offsets: Vec<u16> = (0..node.num_records)
            .filter_map(|i| {
                let off = node.data.len().checked_sub((i + 1) as usize * 2)?;
                u16_be(&node.data, off)
            })
            .collect();

        for &rec_off in &record_offsets {
            if options.is_cancelled() {
                summary.truncated = true;
                break;
            }
            if summary.emitted as usize >= options.max_candidates {
                summary.truncated = true;
                break;
            }
            let rec_off = rec_off as usize;
            if rec_off >= node.data.len() {
                continue;
            }
            let rec_len = u16_be(&node.data, rec_off).unwrap_or(0) as usize;
            if rec_off + 2 + rec_len > node.data.len() {
                continue;
            }
            let record_data = &node.data[rec_off + 2..rec_off + 2 + rec_len];

            if record_data.len() < 6 {
                continue;
            }
            let record_type = u16_be(record_data, 0).unwrap_or(0);

            match record_type {
                0x0001 => {
                    if let Some(folder_thread) = ThreadRecord::parse(record_data, 2) {
                        path_map.insert(
                            folder_thread.parent_cnid,
                            format!("/{}", folder_thread.node_name),
                        );
                    }
                }
                0x0002 => {
                    if let Some(_file_thread) = ThreadRecord::parse(record_data, 2) {
                        // File thread record
                    }
                }
                0x0003 => {
                    if record_data.len() < 2 + 2 + 100 {
                        continue;
                    }
                    let _reserved = u16_be(record_data, 2);
                    let folder_record = FolderRecord {
                        flags: u16_be(record_data, 4).unwrap_or(0),
                        valence: u32_be(record_data, 6).unwrap_or(0),
                        cnid: u32_be(record_data, 10).unwrap_or(0),
                        create_date: u32_be(record_data, 14).unwrap_or(0),
                        content_mod_date: u32_be(record_data, 18).unwrap_or(0),
                        attribute_mod_date: u32_be(record_data, 22).unwrap_or(0),
                        access_date: u32_be(record_data, 26).unwrap_or(0),
                        backup_date: u32_be(record_data, 30).unwrap_or(0),
                        permissions: Permissions {
                            owner_id: u32_be(record_data, 34).unwrap_or(0),
                            group_id: u32_be(record_data, 38).unwrap_or(0),
                            admin_flags: u32_be(record_data, 42).unwrap_or(0),
                            owner_flags: u32_be(record_data, 46).unwrap_or(0),
                            mode: u16_be(record_data, 50).unwrap_or(0),
                            reserved: u16_be(record_data, 52).unwrap_or(0),
                        },
                        user_info: FolderUserInfo {
                            window: [0, 0, 0, 0],
                            reserved: [0, 0, 0, 0],
                        },
                        finder_info: [0; 16],
                        text_encoding: 0,
                        reserved: 0,
                    };
                    path_map.insert(folder_record.cnid, format!("/{}", folder_record.cnid));
                }
                0x0004 => {
                    if record_data.len() < 2 + 200 {
                        continue;
                    }
                    if let Some(file_record) = FileRecord::parse(record_data, 2) {
                        if file_record.is_deleted() || options.include_live {
                            summary.deleted_entries += if file_record.is_deleted() { 1 } else { 0 };
                            summary.live_entries += if !file_record.is_deleted() { 1 } else { 0 };

                            let parent_path = path_map
                                .get(&file_record.cnid)
                                .cloned()
                                .unwrap_or_else(|| "/".to_string());
                            let name = format!("file_{}", file_record.cnid);
                            let path = format!("{}/{}", parent_path, name);

                            emit_hfs_file_candidate(
                                sink,
                                &mut summary,
                                &name,
                                &path,
                                &file_record,
                                volume_offset,
                                block_size,
                                file_record.is_deleted(),
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }

        if node.node_type == 0x01 {
            // Internal node
            for i in 0..node.num_records {
                let child_off = 2 + i as usize * 2;
                if child_off + 2 <= node.data.len() {
                    let child_node = u32_be(&node.data, child_off).unwrap_or(0);
                    if child_node != 0 {
                        stack.push(child_node);
                    }
                }
            }
        }
    }

    Ok(summary)
}

#[allow(clippy::too_many_arguments)]
fn emit_hfs_file_candidate(
    sink: &mut dyn CandidateSink,
    summary: &mut FsSummary,
    name: &str,
    path: &str,
    file: &FileRecord,
    volume_offset: u64,
    block_size: u32,
    is_deleted: bool,
) -> Result<()> {
    let mut extents = Vec::new();
    let mut total_size = 0u64;

    for extent in &file.data_fork.extents {
        if extent.block_count == 0 {
            continue;
        }
        let start = volume_offset + extent.start_block as u64 * block_size as u64;
        let len = extent.block_count as u64 * block_size as u64;
        extents.push(Segment::Data {
            offset: start,
            length: len,
        });
        total_size += len;
    }

    // Also handle resource fork if present
    for extent in &file.resource_fork.extents {
        if extent.block_count == 0 {
            continue;
        }
        let start = volume_offset + extent.start_block as u64 * block_size as u64;
        let len = extent.block_count as u64 * block_size as u64;
        extents.push(Segment::Data {
            offset: start,
            length: len,
        });
        total_size += len;
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
            80,
            vec![
                "HFS+ catalog record marked deleted".into(),
                "Extent descriptors preserved".into(),
            ],
        )
    } else {
        Confidence::new(
            ConfidenceLabel::Verified,
            95,
            vec![
                "HFS+ live catalog record".into(),
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
        "hfs+",
    );
    candidate.path = path.to_string();
    candidate.state = if is_deleted {
        EntryState::Deleted
    } else {
        EntryState::Live
    };
    candidate.modified = Some(hfs_time_to_iso(file.content_mod_date));
    candidate.accessed = Some(hfs_time_to_iso(file.access_date));
    candidate.created = Some(hfs_time_to_iso(file.create_date));
    candidate.mode = Some(file.permissions.mode);
    candidate.record_id = Some(format!("hfs_cnid_{}", file.cnid));

    sink.emit(candidate)?;
    summary.emitted += 1;
    Ok(())
}

fn hfs_time_to_iso(mac_time: u32) -> String {
    if mac_time == 0 {
        return "1904-01-01T00:00:00Z".to_string();
    }
    // HFS+ uses seconds since Jan 1, 1904
    let unix_time = mac_time as i64 - 2082844800; // 1904 to 1970 offset
    if unix_time <= 0 {
        return "1970-01-01T00:00:00Z".to_string();
    }
    crate::util::unix_to_iso(unix_time).unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}
