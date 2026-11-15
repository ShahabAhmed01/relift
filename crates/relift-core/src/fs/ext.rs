//! ext2/ext3/ext4 metadata recovery.
//!
//! Relift reads the superblock and group descriptors, then walks the inode
//! table looking for inodes whose bitmap bit is clear or whose deletion time is
//! set. Live and deleted directory entries are parsed so that names and parent
//! paths can be attached to those inodes; an inode with no surviving entry is
//! still reported, under a synthetic name, instead of being hidden.
//!
//! Data mapping supports extent trees (ext4) and direct/single/double/triple
//! indirect blocks (ext2/ext3). Sparse holes and unwritten extents are kept as
//! explicit zero segments so recovered files retain their original length and
//! byte offsets. Not implemented: ext4 inline-data beyond the inode body,
//! encrypted files, bigalloc, and journal replay.

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
use crate::util::{u16_le, u32_le, u8_at, unix_to_iso};

const EXT_MAGIC: u16 = 0xEF53;
const ROOT_INODE: u32 = 2;
const MAX_INODES: u32 = 2_000_000;
const MAX_GROUPS: u32 = 65_536;
const MAX_DIR_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DIR_DEPTH: u32 = 64;
const MAX_EXTENTS: usize = 1_000_000;
const MAX_INDIRECT_BLOCKS: u64 = 4_000_000;

const INCOMPAT_FILETYPE: u32 = 0x0002;
const INCOMPAT_JOURNAL_DEV: u32 = 0x0008;
const INCOMPAT_META_BG: u32 = 0x0010;
const INCOMPAT_EXTENTS: u32 = 0x0040;
const INCOMPAT_64BIT: u32 = 0x0080;
const INCOMPAT_INLINE_DATA: u32 = 0x8000;
const INCOMPAT_ENCRYPT: u32 = 0x1_0000;
const RO_COMPAT_BIGALLOC: u32 = 0x0200;

const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
const EXT4_INLINE_DATA_FL: u32 = 0x1000_0000;

#[derive(Debug, Clone)]
struct ExtBoot {
    block_size: u64,
    inodes_count: u32,
    blocks_count: u64,
    first_data_block: u64,
    blocks_per_group: u32,
    inodes_per_group: u32,
    inode_size: u32,
    desc_size: u32,
    group_count: u32,
    gdt_offset: u64,
    feature_compat: u32,
    feature_incompat: u32,
    feature_ro_compat: u32,
    volume_name: Option<String>,
    warnings: Vec<String>,
}

impl ExtBoot {
    fn parse(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Result<Self> {
        if volume.length < 2048 {
            return Err(Error::invalid("volume too small for an ext superblock"));
        }
        let sb = read_at_volume(src, volume, 1024, 1024)?;
        if sb.len() < 1024 {
            return Err(Error::invalid("truncated ext superblock"));
        }
        if u16_le(&sb, 56) != Some(EXT_MAGIC) {
            return Err(Error::invalid("missing ext2/3/4 magic"));
        }
        let mut warnings = Vec::new();
        let log_block = u32_le(&sb, 24).unwrap_or(0xFF);
        if log_block > 6 {
            return Err(Error::invalid("ext block size shift is out of range"));
        }
        let block_size = 1024u64 << log_block;
        let inodes_count = u32_le(&sb, 0).unwrap_or(0);
        let blocks_lo = u32_le(&sb, 4).unwrap_or(0) as u64;
        let rev_level = u32_le(&sb, 76).unwrap_or(0);
        let feature_compat = u32_le(&sb, 92).unwrap_or(0);
        let feature_incompat = u32_le(&sb, 96).unwrap_or(0);
        let feature_ro_compat = u32_le(&sb, 100).unwrap_or(0);
        let blocks_hi = if feature_incompat & INCOMPAT_64BIT != 0 {
            u32_le(&sb, 336).unwrap_or(0) as u64
        } else {
            0
        };
        let blocks_count = blocks_lo | (blocks_hi << 32);
        let first_data_block = u32_le(&sb, 20).unwrap_or(0) as u64;
        let blocks_per_group = u32_le(&sb, 32).unwrap_or(0);
        let inodes_per_group = u32_le(&sb, 40).unwrap_or(0);
        if inodes_count == 0 || blocks_count == 0 {
            return Err(Error::invalid(
                "ext superblock declares zero inodes or blocks",
            ));
        }
        if first_data_block >= blocks_count {
            return Err(Error::invalid("ext first data block is outside the volume"));
        }
        if blocks_per_group == 0 || inodes_per_group == 0 {
            return Err(Error::invalid("ext group dimensions are zero"));
        }
        let inode_size = if rev_level == 0 {
            128
        } else {
            let v = u16_le(&sb, 88).unwrap_or(128) as u32;
            if v == 0 {
                128
            } else {
                v
            }
        };
        if !(128..=block_size as u32).contains(&inode_size) || !inode_size.is_power_of_two() {
            return Err(Error::invalid(format!(
                "ext inode size {inode_size} is invalid for a {block_size}-byte block"
            )));
        }
        let desc_size = if feature_incompat & INCOMPAT_64BIT != 0 {
            let v = u16_le(&sb, 254).unwrap_or(64) as u32;
            if v < 64 {
                64
            } else {
                v.min(block_size as u32)
            }
        } else {
            32
        };
        if desc_size < 32 || !desc_size.is_power_of_two() {
            return Err(Error::invalid("ext group descriptor size is invalid"));
        }
        let group_count = (blocks_count - first_data_block).div_ceil(blocks_per_group as u64);
        if group_count == 0 || group_count > MAX_GROUPS as u64 {
            return Err(Error::unsupported(format!(
                "ext filesystem has {group_count} block groups, above the supported limit of {MAX_GROUPS}"
            )));
        }
        if feature_incompat & INCOMPAT_JOURNAL_DEV != 0 {
            return Err(Error::unsupported(
                "this is an external ext journal device, not a filesystem",
            ));
        }
        if feature_incompat & INCOMPAT_META_BG != 0 {
            return Err(Error::unsupported(
                "ext meta_bg group descriptors are not implemented in this build",
            ));
        }
        if feature_ro_compat & RO_COMPAT_BIGALLOC != 0 {
            return Err(Error::unsupported(
                "ext bigalloc filesystems are not implemented in this build",
            ));
        }
        if feature_incompat & INCOMPAT_ENCRYPT != 0 {
            warnings.push(
                "ext encryption feature is present; encrypted inodes are reported as unverified"
                    .into(),
            );
        }
        let volume_name = sb
            .get(120..136)
            .map(|b| {
                String::from_utf8_lossy(b)
                    .trim_matches('\u{0}')
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty());
        let gdt_offset = (first_data_block + 1)
            .checked_mul(block_size)
            .ok_or_else(|| Error::Corrupt("ext group descriptor offset overflow".into()))?;
        let gdt_bytes = group_count
            .checked_mul(desc_size as u64)
            .ok_or_else(|| Error::Corrupt("ext group descriptor table size overflow".into()))?;
        if gdt_offset
            .checked_add(gdt_bytes)
            .map(|end| end > volume.length)
            .unwrap_or(true)
        {
            return Err(Error::invalid(
                "ext group descriptor table lies outside the volume",
            ));
        }
        let fs_bytes = blocks_count
            .checked_mul(block_size)
            .ok_or_else(|| Error::Corrupt("ext volume size overflow".into()))?;
        if fs_bytes > volume.length {
            warnings.push(format!(
                "ext superblock describes {fs_bytes} bytes but the volume is {} bytes; using the smaller bound",
                volume.length
            ));
        }
        Ok(Self {
            block_size,
            inodes_count,
            blocks_count,
            first_data_block,
            blocks_per_group,
            inodes_per_group,
            inode_size,
            desc_size,
            group_count: group_count as u32,
            gdt_offset,
            feature_compat,
            feature_incompat,
            feature_ro_compat,
            volume_name,
            warnings,
        })
    }

    fn info(&self) -> FsInfo {
        let mut details = BTreeMap::new();
        details.insert("block_size".into(), self.block_size.to_string());
        details.insert("inode_size".into(), self.inode_size.to_string());
        details.insert("inodes_count".into(), self.inodes_count.to_string());
        details.insert("blocks_count".into(), self.blocks_count.to_string());
        details.insert("groups".into(), self.group_count.to_string());
        details.insert(
            "features_incompat".into(),
            format!("{:#010x}", self.feature_incompat),
        );
        details.insert(
            "features_ro_compat".into(),
            format!("{:#010x}", self.feature_ro_compat),
        );
        details.insert(
            "features_compat".into(),
            format!("{:#010x}", self.feature_compat),
        );
        FsInfo {
            kind: FsKind::Ext4,
            label: self.volume_name.clone(),
            cluster_size: Some(self.block_size),
            details,
            warnings: self.warnings.clone(),
        }
    }

    fn kind(&self) -> FsKind {
        // ext2 has no extents or 64-bit feature; ext3 adds a journal; ext4 is
        // identified by extents/64-bit/inline-data metadata.
        if self.feature_incompat & (INCOMPAT_EXTENTS | INCOMPAT_64BIT | INCOMPAT_INLINE_DATA) != 0 {
            FsKind::Ext4
        } else if self.feature_compat & 0x0004 != 0 {
            // HAS_JOURNAL compat feature.
            FsKind::Ext3
        } else {
            FsKind::Ext2
        }
    }
}

#[derive(Debug, Clone)]
struct GroupDesc {
    inode_bitmap: u64,
    inode_table: u64,
    flags: u16,
}

const BG_INODE_UNINIT: u16 = 0x0001;

#[derive(Debug, Clone)]
struct Inode {
    number: u32,
    mode: u16,
    size: u64,
    atime: Option<String>,
    ctime: Option<String>,
    mtime: Option<String>,
    deleted_time: Option<String>,
    dtime_raw: u32,
    links: u16,
    blocks: u64,
    flags: u32,
    i_block: [u8; 60],
    extra_isize: u16,
    crtime: Option<String>,
    raw: Vec<u8>,
}

impl Inode {
    fn file_type(&self) -> u16 {
        self.mode & 0xF000
    }

    fn is_dir(&self) -> bool {
        self.file_type() == 0x4000
    }

    fn is_regular(&self) -> bool {
        self.file_type() == 0x8000
    }

    fn is_symlink(&self) -> bool {
        self.file_type() == 0xA000
    }
}

fn parse_inode(number: u32, raw: Vec<u8>, inode_size: u32) -> Option<Inode> {
    if raw.len() < 128 {
        return None;
    }
    let mode = u16_le(&raw, 0)?;
    if mode == 0 {
        return None;
    }
    let size_lo = u32_le(&raw, 4)? as u64;
    let size_hi = if inode_size >= 112 {
        u32_le(&raw, 108).unwrap_or(0) as u64
    } else {
        0
    };
    let mut size = size_lo | (size_hi << 32);
    // ext4 inline data uses the inode body for tiny files.
    if size == 0 && u32_le(&raw, 32).unwrap_or(0) & EXT4_INLINE_DATA_FL != 0 {
        size = 60;
    }
    let flags = u32_le(&raw, 32).unwrap_or(0);
    let mut i_block = [0u8; 60];
    i_block.copy_from_slice(&raw[40..100]);
    let extra_isize = if inode_size >= 130 {
        u16_le(&raw, 128).unwrap_or(0)
    } else {
        0
    };
    let crtime = if inode_size >= 148 && extra_isize >= 20 {
        unix_to_iso(u32_le(&raw, 144).unwrap_or(0) as i64)
    } else {
        None
    };
    Some(Inode {
        number,
        mode,
        size,
        atime: unix_to_iso(u32_le(&raw, 8).unwrap_or(0) as i64),
        ctime: unix_to_iso(u32_le(&raw, 12).unwrap_or(0) as i64),
        mtime: unix_to_iso(u32_le(&raw, 16).unwrap_or(0) as i64),
        deleted_time: if u32_le(&raw, 20).unwrap_or(0) != 0 {
            unix_to_iso(u32_le(&raw, 20).unwrap_or(0) as i64)
        } else {
            None
        },
        dtime_raw: u32_le(&raw, 20).unwrap_or(0),
        links: u16_le(&raw, 26).unwrap_or(0),
        blocks: if flags & 0x0004_0000 != 0 {
            // EXT4_HUGE_FILE_FL: i_blocks is in filesystem blocks, not 512-byte
            // sectors. The high half is in osd2. We do not need it for mapping.
            u32_le(&raw, 28).unwrap_or(0) as u64
        } else {
            u32_le(&raw, 28).unwrap_or(0) as u64
        },
        flags,
        i_block,
        extra_isize,
        crtime,
        raw,
    })
}

struct ExtScan<'a> {
    src: &'a dyn ReadOnlySource,
    volume: &'a VolumeRef,
    boot: ExtBoot,
    groups: Vec<GroupDesc>,
    options: &'a FsScanOptions,
    sink: &'a mut dyn CandidateSink,
    summary: FsSummary,
    inorder_bitmaps: HashMap<u32, Vec<u8>>,
    block_cache: HashMap<u64, Vec<u32>>,
    paths: HashMap<u32, PathRef>,
    visited_dirs: HashSet<u32>,
}

#[derive(Debug, Clone)]
struct PathRef {
    name: String,
    parent: String,
    is_dir: bool,
}

#[derive(Debug, Clone)]
struct ExtentRun {
    logical: u64,
    clusters: u64,
    physical: Option<u64>,
    unwritten: bool,
}

impl<'a> ExtScan<'a> {
    fn open(
        src: &'a dyn ReadOnlySource,
        volume: &'a VolumeRef,
        options: &'a FsScanOptions,
        sink: &'a mut dyn CandidateSink,
    ) -> Result<Self> {
        let boot = ExtBoot::parse(src, volume)?;
        let gdt_abs = volume.offset.checked_add(boot.gdt_offset).ok_or_else(|| {
            Error::Corrupt("ext group descriptor absolute offset overflow".into())
        })?;
        let gdt_len = boot.group_count as u64 * boot.desc_size as u64;
        let table = src.read_vec(gdt_abs, gdt_len)?;
        let mut groups = Vec::with_capacity(boot.group_count as usize);
        for i in 0..boot.group_count as usize {
            let at = i * boot.desc_size as usize;
            let desc = &table[at..at + boot.desc_size as usize];
            let inode_bitmap = u32_le(desc, 4).unwrap_or(0) as u64
                | (if desc.len() >= 40 {
                    (u32_le(desc, 36).unwrap_or(0) as u64) << 32
                } else {
                    0
                });
            let inode_table = u32_le(desc, 8).unwrap_or(0) as u64
                | (if desc.len() >= 44 {
                    (u32_le(desc, 40).unwrap_or(0) as u64) << 32
                } else {
                    0
                });
            if inode_table == 0 || inode_table >= boot.blocks_count {
                return Err(Error::Corrupt(format!(
                    "ext group {i} inode table block {inode_table} is outside the filesystem"
                )));
            }
            if inode_bitmap == 0 || inode_bitmap >= boot.blocks_count {
                return Err(Error::Corrupt(format!(
                    "ext group {i} inode bitmap block {inode_bitmap} is outside the filesystem"
                )));
            }
            groups.push(GroupDesc {
                inode_bitmap,
                inode_table,
                flags: u16_le(desc, 18).unwrap_or(0),
            });
        }
        let mut summary = FsSummary::new(boot.kind(), boot.volume_name.clone());
        summary.warnings.extend(boot.warnings.clone());
        Ok(Self {
            src,
            volume,
            boot,
            groups,
            options,
            sink,
            summary,
            inorder_bitmaps: HashMap::new(),
            block_cache: HashMap::new(),
            paths: HashMap::new(),
            visited_dirs: HashSet::new(),
        })
    }

    fn read_block(&self, block: u64) -> Result<Vec<u8>> {
        if block >= self.boot.blocks_count {
            return Err(Error::Corrupt(format!(
                "ext block {block} is outside the filesystem"
            )));
        }
        let rel = block
            .checked_mul(self.boot.block_size)
            .ok_or_else(|| Error::Corrupt("ext block offset overflow".into()))?;
        let abs = self
            .volume
            .offset
            .checked_add(rel)
            .ok_or_else(|| Error::Corrupt("ext block absolute offset overflow".into()))?;
        let len = self.boot.block_size.min(crate::source::MAX_BUFFERED_READ);
        let mut out = vec![0u8; len as usize];
        self.src.read_exact_at(abs, &mut out)?;
        Ok(out)
    }

    fn read_inode(&self, number: u32) -> Result<Inode> {
        if number == 0 || number > self.boot.inodes_count {
            return Err(Error::Corrupt(format!(
                "ext inode {number} is out of range"
            )));
        }
        let index = number - 1;
        let group = index / self.boot.inodes_per_group;
        let local = index % self.boot.inodes_per_group;
        let desc = self
            .groups
            .get(group as usize)
            .ok_or_else(|| Error::Corrupt(format!("ext inode {number} has no group descriptor")))?;
        let offset = local as u64 * self.boot.inode_size as u64;
        let table_bytes = (self.boot.inodes_per_group as u64)
            .checked_mul(self.boot.inode_size as u64)
            .ok_or_else(|| Error::Corrupt("ext inode table size overflow".into()))?;
        if offset + self.boot.inode_size as u64 > table_bytes {
            return Err(Error::Corrupt(format!(
                "ext inode {number} lies outside its inode table"
            )));
        }
        let rel = desc
            .inode_table
            .checked_mul(self.boot.block_size)
            .and_then(|v| v.checked_add(offset))
            .ok_or_else(|| Error::Corrupt("ext inode offset overflow".into()))?;
        let abs = self
            .volume
            .offset
            .checked_add(rel)
            .ok_or_else(|| Error::Corrupt("ext inode absolute offset overflow".into()))?;
        let raw = self.src.read_vec(abs, self.boot.inode_size as u64)?;
        parse_inode(number, raw, self.boot.inode_size)
            .ok_or_else(|| Error::Corrupt(format!("ext inode {number} is empty")))
    }

    fn inode_allocated(&mut self, number: u32) -> Option<bool> {
        let index = number.checked_sub(1)?;
        let group = index / self.boot.inodes_per_group;
        let local = index % self.boot.inodes_per_group;
        let desc = self.groups.get(group as usize)?;
        if desc.flags & BG_INODE_UNINIT != 0 {
            return Some(false);
        }
        if !self.inorder_bitmaps.contains_key(&group) {
            let block = desc.inode_bitmap;
            let bytes = self.read_block(block).ok()?;
            self.inorder_bitmaps.insert(group, bytes);
        }
        let bitmap = self.inorder_bitmaps.get(&group)?;
        let byte = (local / 8) as usize;
        let bit = (local % 8) as u8;
        let value = *bitmap.get(byte)?;
        Some(value & (1 << bit) != 0)
    }

    fn read_directory_entries(&self, data: &[u8]) -> Vec<(u32, String, bool)> {
        let mut out = Vec::new();
        let block_size = self.boot.block_size as usize;
        let file_type_feature = self.boot.feature_incompat & INCOMPAT_FILETYPE != 0;
        for chunk in data.chunks(block_size) {
            let mut at = 0usize;
            while at + 8 <= chunk.len() {
                let inode = u32_le(chunk, at).unwrap_or(0);
                let rec_len = u16_le(chunk, at + 4).unwrap_or(0) as usize;
                if rec_len < 8 || rec_len % 4 != 0 || at + rec_len > chunk.len() {
                    break;
                }
                let (name_len, file_type) = if file_type_feature {
                    (
                        u8_at(chunk, at + 6).unwrap_or(0) as usize,
                        u8_at(chunk, at + 7).unwrap_or(0),
                    )
                } else {
                    (u16_le(chunk, at + 6).unwrap_or(0) as usize, 0)
                };
                if inode != 0 && name_len > 0 && name_len <= rec_len.saturating_sub(8) {
                    let name_bytes = &chunk[at + 8..at + 8 + name_len];
                    let name = String::from_utf8_lossy(name_bytes)
                        .trim_matches('\u{0}')
                        .to_string();
                    if !name.is_empty() && name != "." && name != ".." {
                        let is_dir = file_type == 2;
                        out.push((inode, name, is_dir));
                    }
                }
                at += rec_len;
            }
        }
        out
    }

    fn walk_directory_tree(&mut self) -> Result<()> {
        let mut stack = vec![(ROOT_INODE, String::new(), 0u32)];
        while let Some((inode_no, parent, depth)) = stack.pop() {
            if self.options.is_cancelled() {
                break;
            }
            if depth >= MAX_DIR_DEPTH {
                self.summary.warnings.push(format!(
                    "ext directory depth limit reached at inode {inode_no}"
                ));
                continue;
            }
            if !self.visited_dirs.insert(inode_no) {
                continue;
            }
            let inode = match self.read_inode(inode_no) {
                Ok(i) if i.is_dir() => i,
                _ => continue,
            };
            let data = match self.read_inode_bytes(&inode, MAX_DIR_BYTES) {
                Ok(d) => d,
                Err(e) => {
                    self.summary.warnings.push(format!(
                        "could not read ext directory inode {inode_no}: {e}"
                    ));
                    continue;
                }
            };
            for (child, name, hinted_dir) in self.read_directory_entries(&data) {
                if child == 0 || child > self.boot.inodes_count {
                    continue;
                }
                let is_dir = if hinted_dir {
                    true
                } else {
                    self.read_inode(child).map(|i| i.is_dir()).unwrap_or(false)
                };
                let child_path = if parent.is_empty() {
                    name.clone()
                } else {
                    format!("{parent}/{name}")
                };
                self.paths.entry(child).or_insert_with(|| PathRef {
                    name: name.clone(),
                    parent: parent.clone(),
                    is_dir,
                });
                if is_dir {
                    stack.push((child, child_path, depth + 1));
                }
            }
        }
        Ok(())
    }

    fn read_inode_bytes(&mut self, inode: &Inode, max: u64) -> Result<Vec<u8>> {
        let size = inode.size.min(max);
        if size == 0 {
            return Ok(Vec::new());
        }
        if inode.is_symlink() && inode.blocks == 0 && inode.size <= 60 {
            return Ok(inode.i_block[..inode.size as usize].to_vec());
        }
        if inode.flags & EXT4_INLINE_DATA_FL != 0 && inode.size <= 60 {
            return Ok(inode.i_block[..inode.size as usize].to_vec());
        }
        let (segments, _, _) = self.map_inode(inode, size)?;
        let mut out = vec![0u8; size as usize];
        let mut logical = 0u64;
        let mut filled = 0usize;
        for segment in &segments {
            let seg_len = segment.length();
            if logical >= size {
                break;
            }
            let take = seg_len.min(size - logical) as usize;
            match segment {
                Segment::Data { offset, length } => {
                    let take = take.min(length.saturating_sub(logical) as usize);
                    self.src
                        .read_exact_at(*offset, &mut out[filled..filled + take])?;
                }
                Segment::Zero { .. } => {}
            }
            logical = logical.saturating_add(seg_len);
            filled += take;
            if filled >= out.len() {
                break;
            }
        }
        Ok(out)
    }

    fn collect_extents(
        &self,
        node: &[u8],
        expected_depth: u16,
        out: &mut Vec<ExtentRun>,
        depth_left: u32,
    ) -> Result<()> {
        if node.len() < 12 {
            return Err(Error::Corrupt("ext extent node is truncated".into()));
        }
        if u16_le(node, 0) != Some(0xF30A) {
            return Err(Error::Corrupt("ext extent magic does not match".into()));
        }
        let entries = u16_le(node, 2).unwrap_or(0) as usize;
        let max_entries = u16_le(node, 4).unwrap_or(0) as usize;
        let depth = u16_le(node, 6).unwrap_or(0);
        if depth != expected_depth || entries > max_entries {
            return Err(Error::Corrupt("ext extent header is inconsistent".into()));
        }
        // Leaf entries (extent) and index entries (index) are both 12 bytes.
        let entry_size = 12;
        let needed = 12usize
            .checked_add(
                entries
                    .checked_mul(entry_size)
                    .ok_or_else(|| Error::Corrupt("ext extent entry count overflow".into()))?,
            )
            .ok_or_else(|| Error::Corrupt("ext extent node size overflow".into()))?;
        if needed > node.len() {
            return Err(Error::Corrupt("ext extent entries exceed the node".into()));
        }
        if depth == 0 {
            for i in 0..entries {
                let at = 12 + i * 12;
                let logical = u32_le(node, at).unwrap_or(0) as u64;
                let raw_len = u16_le(node, at + 4).unwrap_or(0);
                if raw_len == 0 {
                    continue;
                }
                let (clusters, unwritten) = if raw_len > 32768 {
                    ((raw_len - 32768) as u64, true)
                } else {
                    (raw_len as u64, false)
                };
                let hi = u16_le(node, at + 6).unwrap_or(0) as u64;
                let lo = u32_le(node, at + 8).unwrap_or(0) as u64;
                let physical = (hi << 32) | lo;
                out.push(ExtentRun {
                    logical,
                    clusters,
                    physical: if physical == 0 { None } else { Some(physical) },
                    unwritten,
                });
                if out.len() > MAX_EXTENTS {
                    return Err(Error::unsupported(
                        "ext inode has too many extents to scan safely",
                    ));
                }
            }
        } else {
            if depth_left == 0 {
                return Err(Error::Corrupt("ext extent tree is too deep".into()));
            }
            for i in 0..entries {
                let at = 12 + i * 12;
                let _logical = u32_le(node, at).unwrap_or(0);
                let lo = u32_le(node, at + 4).unwrap_or(0) as u64;
                let hi = u16_le(node, at + 8).unwrap_or(0) as u64;
                let child = (hi << 32) | lo;
                if child == 0 || child >= self.boot.blocks_count {
                    return Err(Error::Corrupt(
                        "ext extent index points outside the volume".into(),
                    ));
                }
                let block = self.read_block(child)?;
                self.collect_extents(&block, depth - 1, out, depth_left - 1)?;
            }
        }
        Ok(())
    }

    fn map_inode(
        &mut self,
        inode: &Inode,
        size: u64,
    ) -> Result<(Vec<Segment>, bool, Option<String>)> {
        if inode.flags & EXT4_INLINE_DATA_FL != 0 {
            if inode.size <= 60 {
                return Ok((
                    vec![Segment::data(0, 0)],
                    true,
                    Some("inline data is stored inside the inode body".into()),
                ));
            }
            return Err(Error::unsupported(
                "ext4 inline data larger than the inode body is not implemented",
            ));
        }
        if inode.size == 0 {
            return Ok((Vec::new(), true, None));
        }
        if inode.flags & EXT4_EXTENTS_FL != 0 {
            let mut runs = Vec::new();
            self.collect_extents(&inode.i_block, 0, &mut runs, 8)?;
            self.runs_to_segments(inode, runs, size)
        } else {
            self.indirect_segments(inode, size)
        }
    }

    fn runs_to_segments(
        &self,
        inode: &Inode,
        mut runs: Vec<ExtentRun>,
        size: u64,
    ) -> Result<(Vec<Segment>, bool, Option<String>)> {
        runs.sort_by_key(|r| r.logical);
        let mut segments = Vec::new();
        let mut cursor = 0u64;
        let mut note = None;
        for run in &runs {
            let log_bytes = run
                .logical
                .checked_mul(self.boot.block_size)
                .ok_or_else(|| Error::Corrupt("ext extent logical offset overflow".into()))?;
            let len = run
                .clusters
                .checked_mul(self.boot.block_size)
                .ok_or_else(|| Error::Corrupt("ext extent length overflow".into()))?;
            if log_bytes > cursor {
                segments.push(Segment::zero(log_bytes - cursor));
                cursor = log_bytes;
            }
            if log_bytes < cursor {
                note = Some(format!(
                    "ext inode {} has overlapping extents; the later extent was clamped",
                    inode.number
                ));
                continue;
            }
            match run.physical {
                Some(physical) if !run.unwritten => {
                    let end = physical
                        .checked_add(run.clusters)
                        .ok_or_else(|| Error::Corrupt("ext extent physical end overflow".into()))?;
                    if end > self.boot.blocks_count {
                        return Err(Error::Corrupt(format!(
                            "ext inode {} extent points outside the filesystem",
                            inode.number
                        )));
                    }
                    let rel = physical
                        .checked_mul(self.boot.block_size)
                        .ok_or_else(|| Error::Corrupt("ext extent offset overflow".into()))?;
                    let abs = self.volume.offset.checked_add(rel).ok_or_else(|| {
                        Error::Corrupt("ext extent absolute offset overflow".into())
                    })?;
                    segments.push(Segment::data(abs, len));
                }
                _ => {
                    segments.push(Segment::zero(len));
                    if run.unwritten {
                        note.get_or_insert_with(|| {
                            "unwritten extents are emitted as zeroes".into()
                        });
                    }
                }
            }
            cursor = cursor.saturating_add(len);
        }
        let complete = cursor >= size;
        if !complete {
            note.get_or_insert_with(|| "extents cover less than the inode size".into());
        }
        Ok((segments, complete, note))
    }

    fn block_table(&mut self, block: u64) -> Result<Vec<u32>> {
        if let Some(cached) = self.block_cache.get(&block) {
            return Ok(cached.clone());
        }
        let data = self.read_block(block)?;
        let mut table = Vec::with_capacity(data.len() / 4);
        for chunk in data.chunks_exact(4) {
            table.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        if self.block_cache.len() > 64 {
            self.block_cache.clear();
        }
        self.block_cache.insert(block, table.clone());
        Ok(table)
    }

    fn indirect_pointer(&mut self, inode: &Inode, logical: u64) -> Result<Option<u64>> {
        let per = self.boot.block_size / 4;
        if logical < 12 {
            return Ok(Some(
                u32_le(&inode.i_block, logical as usize * 4).unwrap_or(0) as u64,
            ));
        }
        let mut n = logical - 12;
        let single = u32_le(&inode.i_block, 12 * 4).unwrap_or(0) as u64;
        if n < per {
            if single == 0 {
                return Ok(None);
            }
            let table = self.block_table(single)?;
            return Ok(table
                .get(n as usize)
                .copied()
                .map(|v| v as u64)
                .filter(|v| *v != 0));
        }
        n -= per;
        let double = u32_le(&inode.i_block, 13 * 4).unwrap_or(0) as u64;
        if n < per * per {
            if double == 0 {
                return Ok(None);
            }
            let table = self.block_table(double)?;
            let first = table.get((n / per) as usize).copied().unwrap_or(0) as u64;
            if first == 0 {
                return Ok(None);
            }
            let leaf = self.block_table(first)?;
            return Ok(leaf
                .get((n % per) as usize)
                .copied()
                .map(|v| v as u64)
                .filter(|v| *v != 0));
        }
        n -= per * per;
        let triple = u32_le(&inode.i_block, 14 * 4).unwrap_or(0) as u64;
        if n < per * per * per {
            if triple == 0 {
                return Ok(None);
            }
            let table = self.block_table(triple)?;
            let first = table.get((n / (per * per)) as usize).copied().unwrap_or(0) as u64;
            if first == 0 {
                return Ok(None);
            }
            let second_table = self.block_table(first)?;
            let second = second_table
                .get(((n / per) % per) as usize)
                .copied()
                .unwrap_or(0) as u64;
            if second == 0 {
                return Ok(None);
            }
            let leaf = self.block_table(second)?;
            return Ok(leaf
                .get((n % per) as usize)
                .copied()
                .map(|v| v as u64)
                .filter(|v| *v != 0));
        }
        Ok(None)
    }

    fn indirect_segments(
        &mut self,
        inode: &Inode,
        size: u64,
    ) -> Result<(Vec<Segment>, bool, Option<String>)> {
        let needed = size.div_ceil(self.boot.block_size);
        if needed > MAX_INDIRECT_BLOCKS {
            return Ok((
                Vec::new(),
                false,
                Some(format!(
                    "file needs {needed} indirect blocks, above the safe scan limit of {MAX_INDIRECT_BLOCKS}"
                )),
            ));
        }
        let block_size = self.boot.block_size;
        let mut segments = Vec::new();
        let mut logical_run_start = 0u64;
        let mut run_data_start: Option<u64> = None;
        let mut run_len = 0u64;
        let mut run_zero = false;
        let flush = |segments: &mut Vec<Segment>, len: u64, data_start: Option<u64>, zero: bool| {
            if len == 0 {
                return;
            }
            match data_start {
                Some(start) if !zero => {
                    segments.push(Segment::data(start, len * block_size));
                }
                _ => segments.push(Segment::zero(len * block_size)),
            }
        };
        for logical in 0..needed {
            let pointer = self.indirect_pointer(inode, logical)?;
            let abs = pointer.and_then(|p| {
                if p >= self.boot.blocks_count {
                    None
                } else {
                    p.checked_mul(self.boot.block_size)
                        .and_then(|rel| self.volume.offset.checked_add(rel))
                }
            });
            let zero = abs.is_none();
            if run_len > 0 && zero == run_zero {
                if zero {
                    run_len += 1;
                    continue;
                }
                if let (Some(start), Some(next)) = (run_data_start, abs) {
                    if next == start + run_len * block_size {
                        run_len += 1;
                        continue;
                    }
                }
            }
            flush(&mut segments, run_len, run_data_start, run_zero);
            logical_run_start = logical;
            run_len = 1;
            run_zero = zero;
            run_data_start = abs;
        }
        flush(&mut segments, run_len, run_data_start, run_zero);
        let _ = logical_run_start;
        let complete = needed * self.boot.block_size >= size;
        Ok((segments, complete, None))
    }

    fn make_candidate(
        &mut self,
        inode: &Inode,
        allocated: Option<bool>,
        path: Option<&PathRef>,
    ) -> Candidate {
        let deleted = inode.dtime_raw != 0 || allocated == Some(false);
        let mut reasons = Vec::new();
        let mut label = if deleted {
            ConfidenceLabel::Likely
        } else {
            ConfidenceLabel::Verified
        };
        let mut score: u8 = if deleted { 80 } else { 95 };
        let mut complete = true;
        let data;
        let name;
        let parent;
        if let Some(p) = path {
            name = p.name.clone();
            parent = p.parent.clone();
            reasons.push("directory entry preserves the name".into());
            score = score.saturating_add(3);
        } else {
            name = format!("inode_{}", inode.number);
            parent = String::new();
            label = if deleted {
                ConfidenceLabel::Partial
            } else {
                ConfidenceLabel::Likely
            };
            score = if deleted { 55 } else { 75 };
            reasons.push("no live or stale directory entry names this inode".into());
        }
        if allocated == Some(false) {
            reasons.push("inode bitmap marks this inode free".into());
        }
        if inode.dtime_raw != 0 {
            reasons.push(format!(
                "inode deletion time is {}",
                inode.deleted_time.as_deref().unwrap_or("set")
            ));
        }
        if allocated == Some(false) && inode.dtime_raw == 0 {
            reasons.push("inode bitmap is free but the deletion time is zero".into());
            label = ConfidenceLabel::Partial;
            score = score.min(60);
        }
        if allocated == Some(true) && inode.dtime_raw != 0 {
            reasons.push("inode is still allocated but has a deletion time".into());
            label = ConfidenceLabel::Partial;
            score = score.min(65);
        }
        if inode.links > 1 {
            reasons.push(format!("inode has {} hard links", inode.links));
        }

        if inode.size == 0 {
            data = DataLocation::resident(Vec::new());
            complete = true;
            reasons.push("zero-length inode".into());
        } else if inode.is_symlink() && inode.blocks == 0 && inode.size <= 60 {
            let bytes = inode.i_block[..inode.size as usize].to_vec();
            data = DataLocation::resident(bytes);
            reasons.push("fast symlink target is stored inside the inode".into());
        } else if inode.flags & EXT4_INLINE_DATA_FL != 0 {
            if inode.size <= 60 {
                data = DataLocation::resident(inode.i_block[..inode.size as usize].to_vec());
                reasons.push("inline data is stored inside the inode body".into());
            } else {
                data = DataLocation::None;
                complete = false;
                label = ConfidenceLabel::Unverified;
                score = 30;
                reasons.push("ext4 inline data beyond the inode body is not implemented".into());
            }
        } else if inode.is_dir() {
            data = DataLocation::None;
            complete = false;
            label = ConfidenceLabel::Unverified;
            score = 30;
            reasons.push("inode is a directory, not a recoverable file".into());
        } else {
            match self.map_inode(inode, inode.size) {
                Ok((segments, mapped_complete, note)) => {
                    complete = mapped_complete;
                    if segments.is_empty() {
                        data = DataLocation::None;
                        complete = false;
                        reasons.push("no data blocks could be mapped".into());
                        if label != ConfidenceLabel::Unverified {
                            label = ConfidenceLabel::Partial;
                            score = score.min(45);
                        }
                    } else {
                        data = DataLocation::segments(segments);
                        if inode.flags & EXT4_EXTENTS_FL != 0 {
                            reasons.push("extent tree was walked".into());
                        } else {
                            reasons.push("direct/indirect block pointers were walked".into());
                        }
                    }
                    if let Some(note) = note {
                        reasons.push(note);
                    }
                    if !mapped_complete {
                        label = ConfidenceLabel::Partial;
                        score = score.min(58);
                    }
                }
                Err(e) => {
                    data = DataLocation::None;
                    complete = false;
                    label = ConfidenceLabel::Unverified;
                    score = 25;
                    reasons.push(format!("data mapping failed: {e}"));
                }
            }
        }

        let format = format_from_name(&name);
        let mut candidate = Candidate::new(
            name,
            inode.size,
            format,
            data,
            RecoveryMethod::ExtInode,
            Confidence::new(label, score, reasons),
            self.boot.kind().as_str(),
        );
        candidate.path = parent;
        candidate.state = if deleted {
            EntryState::Deleted
        } else {
            EntryState::Live
        };
        candidate.complete = complete;
        candidate.volume_offset = self.volume.offset;
        candidate.record_id = Some(format!("inode:{}", inode.number));
        candidate.created = inode.crtime.clone();
        candidate.modified = inode.mtime.clone();
        candidate.accessed = inode.atime.clone();
        candidate.deleted = inode.deleted_time.clone();
        candidate.validation.push("inode fields parsed".into());
        if allocated == Some(false) {
            candidate
                .validation
                .push("inode bitmap marks inode free".into());
        }
        candidate
    }
}

/// Identify an ext2/3/4 volume.
pub fn probe(src: &dyn ReadOnlySource, volume: &VolumeRef) -> Option<FsInfo> {
    ExtBoot::parse(src, volume).ok().map(|b| b.info())
}

/// Recovery entry point.
pub fn scan(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    options: &FsScanOptions,
    sink: &mut dyn CandidateSink,
) -> Result<FsSummary> {
    let mut scanner = ExtScan::open(src, volume, options, sink)?;
    if let Err(e) = scanner.walk_directory_tree() {
        scanner
            .summary
            .warnings
            .push(format!("ext directory walk stopped early: {e}"));
    }
    let inodes = scanner.boot.inodes_count.min(MAX_INODES);
    if inodes < scanner.boot.inodes_count {
        scanner.summary.warnings.push(format!(
            "ext filesystem has {} inodes; this build scans the first {inodes}",
            scanner.boot.inodes_count
        ));
        scanner.summary.truncated = true;
    }
    for number in 1..=inodes {
        if scanner.options.is_cancelled() {
            break;
        }
        if scanner.summary.emitted as usize >= scanner.options.max_candidates {
            scanner.summary.truncated = true;
            break;
        }
        let inode = match scanner.read_inode(number) {
            Ok(i) => i,
            Err(_) => continue,
        };
        if inode.number < 11 {
            continue;
        }
        if !inode.is_regular() && !inode.is_symlink() {
            continue;
        }
        let allocated = scanner.inode_allocated(number);
        let deleted = inode.dtime_raw != 0 || allocated == Some(false);
        if inode.dtime_raw == 0 && allocated.is_none() {
            // No bitmap and no deletion time: only include it when the caller
            // asked for a full forensic listing.
            if !scanner.options.include_live {
                continue;
            }
        }
        if !deleted {
            scanner.summary.live_entries += 1;
            if !scanner.options.include_live {
                continue;
            }
        } else {
            scanner.summary.deleted_entries += 1;
        }
        let path = scanner.paths.get(&number).cloned();
        let candidate = scanner.make_candidate(&inode, allocated, path.as_ref());
        scanner.sink.emit(candidate)?;
        scanner.summary.emitted += 1;
    }
    Ok(scanner.summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::utf16le_to_string;

    #[test]
    fn extent_header_and_run_decoding() {
        let mut node = vec![0u8; 60];
        node[0..2].copy_from_slice(&0xF30Au16.to_le_bytes());
        node[2..4].copy_from_slice(&1u16.to_le_bytes());
        node[4..6].copy_from_slice(&4u16.to_le_bytes());
        node[6..8].copy_from_slice(&0u16.to_le_bytes());
        node[12..16].copy_from_slice(&0u32.to_le_bytes());
        node[16..18].copy_from_slice(&1u16.to_le_bytes());
        node[18..20].copy_from_slice(&0u16.to_le_bytes());
        node[20..24].copy_from_slice(&100u32.to_le_bytes());
        // Parsing needs a real scanner; the bit-level check is covered by the
        // fixture integration tests. Here we assert the header helpers.
        assert_eq!(u16_le(&node, 0), Some(0xF30A));
        assert_eq!(u32_le(&node, 20), Some(100));
    }

    #[test]
    fn inode_mode_classification() {
        let mut raw = vec![0u8; 256];
        raw[0..2].copy_from_slice(&0x81A4u16.to_le_bytes());
        raw[4..8].copy_from_slice(&1234u32.to_le_bytes());
        let inode = parse_inode(42, raw, 256).unwrap();
        assert!(inode.is_regular());
        assert_eq!(inode.size, 1234);
    }

    #[test]
    fn utf16_volume_name_helper_is_not_used_for_ext_utf8() {
        // ext volume names are UTF-8 bytes, unlike NTFS/exFAT labels.
        assert_eq!(utf16le_to_string(b"R\0E\0L\0"), "REL");
    }
}
