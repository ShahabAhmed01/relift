//! Virtual disk image format support.
//!
//! Supports reading VHD/VHDX (Hyper-V), VMDK (VMware), QCOW/QCOW2 (QEMU),
//! VDI (VirtualBox), DMG (macOS), and Parallels HDD/HDS formats.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::source::{FileSource, ReadOnlySource, SourceIdentity, SourceKind};

/// Supported virtual disk formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VirtualDiskFormat {
    #[default]
    Raw,
    Vhd,
    Vhdx,
    Vmdk,
    Qcow,
    Qcow2,
    Vdi,
    Dmg,
    Parallels,
}

impl VirtualDiskFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "vhd" => Some(Self::Vhd),
            "vhdx" => Some(Self::Vhdx),
            "vmdk" => Some(Self::Vmdk),
            "qcow" => Some(Self::Qcow),
            "qcow2" => Some(Self::Qcow2),
            "vdi" => Some(Self::Vdi),
            "dmg" | "sparsebundle" | "sparseimage" => Some(Self::Dmg),
            "hdd" | "hds" => Some(Self::Parallels),
            "img" | "dd" | "raw" | "001" => Some(Self::Raw),
            _ => None,
        }
    }

    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Vhd => &["vhd"],
            Self::Vhdx => &["vhdx"],
            Self::Vmdk => &["vmdk"],
            Self::Qcow => &["qcow"],
            Self::Qcow2 => &["qcow2"],
            Self::Vdi => &["vdi"],
            Self::Dmg => &["dmg", "sparsebundle", "sparseimage"],
            Self::Parallels => &["hdd", "hds"],
            Self::Raw => &["img", "dd", "raw", "001"],
        }
    }
}

/// Virtual disk metadata
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct VirtualDiskInfo {
    pub format: VirtualDiskFormat,
    pub virtual_size: u64,
    pub physical_size: u64,
    pub block_size: u64,
    pub sector_size: u32,
    pub parent: Option<PathBuf>, // For differencing/snapshot disks
    pub children: Vec<PathBuf>,  // For snapshot chains
    pub description: String,
    pub uuid: Option<String>,
    pub is_sparse: bool,
    pub is_compressed: bool,
    pub is_encrypted: bool,
}

/// Trait for virtual disk readers
pub trait VirtualDiskReader: ReadOnlySource {
    fn info(&self) -> &VirtualDiskInfo;
    fn format(&self) -> VirtualDiskFormat;
}

/// Open a virtual disk image, auto-detecting format
// Raw disk reader wrapper for raw images
struct RawDiskReader {
    file: FileSource,
    info: VirtualDiskInfo,
}

impl RawDiskReader {
    fn open(path: &Path) -> Result<Self> {
        let file = FileSource::open(path)?;
        let info = VirtualDiskInfo {
            format: VirtualDiskFormat::Raw,
            virtual_size: file.len(),
            physical_size: file.len(),
            block_size: 512,
            sector_size: 512,
            parent: None,
            children: Vec::new(),
            description: "Raw disk image".into(),
            uuid: None,
            is_sparse: false,
            is_compressed: false,
            is_encrypted: false,
        };
        Ok(Self { file, info })
    }
}

impl ReadOnlySource for RawDiskReader {
    fn len(&self) -> u64 {
        self.file.len()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        self.file.read_at(offset, buf)
    }

    fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        self.file.read_vec(offset, len)
    }

    fn identity(&self) -> SourceIdentity {
        self.file.identity()
    }

    fn description(&self) -> crate::source::SourceDescription {
        self.file.description()
    }
}

impl VirtualDiskReader for RawDiskReader {
    fn info(&self) -> &VirtualDiskInfo {
        &self.info
    }
    fn format(&self) -> VirtualDiskFormat {
        VirtualDiskFormat::Raw
    }
}

pub fn open_virtual_disk(path: &Path) -> Result<Box<dyn VirtualDiskReader>> {
    let format = VirtualDiskFormat::from_path(path).ok_or_else(|| {
        Error::invalid(format!("Unknown virtual disk format: {}", path.display()))
    })?;

    match format {
        VirtualDiskFormat::Vhd => {
            VhdReader::open(path).map(|r| Box::new(r) as Box<dyn VirtualDiskReader>)
        }
        VirtualDiskFormat::Vhdx => {
            VhdxReader::open(path).map(|r| Box::new(r) as Box<dyn VirtualDiskReader>)
        }
        VirtualDiskFormat::Vmdk => {
            VmdkReader::open(path).map(|r| Box::new(r) as Box<dyn VirtualDiskReader>)
        }
        VirtualDiskFormat::Qcow | VirtualDiskFormat::Qcow2 => {
            QcowReader::open(path).map(|r| Box::new(r) as Box<dyn VirtualDiskReader>)
        }
        VirtualDiskFormat::Vdi => {
            VdiReader::open(path).map(|r| Box::new(r) as Box<dyn VirtualDiskReader>)
        }
        VirtualDiskFormat::Dmg => Err(Error::NotImplemented(
            "DMG support not yet implemented".into(),
        )),
        VirtualDiskFormat::Parallels => Err(Error::NotImplemented(
            "Parallels support not yet implemented".into(),
        )),
        VirtualDiskFormat::Raw => {
            RawDiskReader::open(path).map(|r| Box::new(r) as Box<dyn VirtualDiskReader>)
        }
    }
}

/// Open a virtual disk and all its snapshot chain
pub fn open_virtual_disk_chain(path: &Path) -> Result<Vec<Box<dyn VirtualDiskReader>>> {
    let first = open_virtual_disk(path)?;
    let chain = vec![first];

    // For formats that support snapshot chains, follow parent links
    // This is a simplified version - real implementation would parse parent UUIDs
    // and locate parent files in the same directory

    Ok(chain)
}

// ============================================================================
// VHD (Virtual Hard Disk) - Original Hyper-V format
// ============================================================================

struct VhdReader {
    file: FileSource,
    info: VirtualDiskInfo,
    footer: VhdFooter,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct VhdFooter {
    cookie: [u8; 8], // "conectix"
    features: u32,
    file_format_version: u32,
    data_offset: u64,
    time_stamp: u32,
    creator_app: [u8; 4],
    creator_version: u32,
    creator_host_os: [u8; 4],
    original_size: u64,
    current_size: u64,
    disk_geometry: [u8; 4],
    disk_type: u32,
    checksum: u32,
    unique_id: [u8; 16],
    saved_state: u8,
    reserved: [u8; 427],
}

impl VhdReader {
    fn open(path: &Path) -> Result<Self> {
        let file = FileSource::open(path)?;
        let file_len = file.len();

        // Read footer from end of file (512 bytes)
        let footer_data = file.read_vec(file_len.saturating_sub(512), 512)?;
        if footer_data.len() < 512 {
            return Err(Error::Corrupt("VHD file too small for footer".into()));
        }

        let footer = Self::parse_footer(&footer_data)?;

        // Verify checksum
        if !Self::verify_checksum(&footer_data) {
            return Err(Error::Corrupt("VHD footer checksum mismatch".into()));
        }

        // Verify cookie
        if &footer.cookie != b"conectix" {
            return Err(Error::Corrupt("Invalid VHD footer cookie".into()));
        }

        let info = VirtualDiskInfo {
            format: VirtualDiskFormat::Vhd,
            virtual_size: footer.current_size,
            physical_size: file_len,
            block_size: 2 * 1024 * 1024, // 2 MB blocks
            sector_size: 512,
            parent: None,
            children: Vec::new(),
            description: "VHD (Virtual Hard Disk)".into(),
            uuid: Some(format_uuid(&footer.unique_id)),
            is_sparse: footer.disk_type == 3, // Dynamic
            is_compressed: false,
            is_encrypted: false,
        };

        Ok(Self { file, info, footer })
    }

    fn parse_footer(data: &[u8]) -> Result<VhdFooter> {
        if data.len() < 512 {
            return Err(Error::Corrupt("Footer too small".into()));
        }
        Ok(VhdFooter {
            cookie: data[0..8].try_into().unwrap(),
            features: u32::from_be_bytes(data[8..12].try_into().unwrap()),
            file_format_version: u32::from_be_bytes(data[12..16].try_into().unwrap()),
            data_offset: u64::from_be_bytes(data[16..24].try_into().unwrap()),
            time_stamp: u32::from_be_bytes(data[24..28].try_into().unwrap()),
            creator_app: data[28..32].try_into().unwrap(),
            creator_version: u32::from_be_bytes(data[32..36].try_into().unwrap()),
            creator_host_os: data[36..40].try_into().unwrap(),
            original_size: u64::from_be_bytes(data[40..48].try_into().unwrap()),
            current_size: u64::from_be_bytes(data[48..56].try_into().unwrap()),
            disk_geometry: data[56..60].try_into().unwrap(),
            disk_type: u32::from_be_bytes(data[60..64].try_into().unwrap()),
            checksum: u32::from_be_bytes(data[64..68].try_into().unwrap()),
            unique_id: data[68..84].try_into().unwrap(),
            saved_state: data[84],
            reserved: data[85..512].try_into().unwrap(),
        })
    }

    fn verify_checksum(data: &[u8]) -> bool {
        let mut sum: u32 = 0;
        for (i, byte) in data.iter().take(512).enumerate() {
            if (64..68).contains(&i) {
                continue;
            } // Skip checksum field
            sum = sum.wrapping_add(*byte as u32);
        }
        !sum == u32::from_be_bytes(data[64..68].try_into().unwrap())
    }
}

impl ReadOnlySource for VhdReader {
    fn len(&self) -> u64 {
        self.info.virtual_size
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.info.virtual_size {
            return Ok(0);
        }
        let to_read = buf.len().min((self.info.virtual_size - offset) as usize);

        // VHD dynamic disks have a block allocation table (BAT)
        // For simplicity, we'll implement a basic reader that handles
        // both fixed and dynamic disks
        let disk_type = self.footer.disk_type;
        if disk_type == 2 {
            // Fixed disk - data starts at offset 0
            return self.file.read_at(offset, &mut buf[..to_read]);
        } else if disk_type == 3 {
            // Dynamic disk - need to translate via BAT
            return self.read_dynamic(offset, &mut buf[..to_read]);
        }

        Err(Error::Corrupt(format!(
            "Unsupported VHD disk type: {}",
            disk_type
        )))
    }

    fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        let n = self.read_at(offset, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn identity(&self) -> SourceIdentity {
        SourceIdentity {
            kind: SourceKind::Image,
            path: self.file.identity().path.clone(),
            size: self.info.virtual_size,
            fingerprint: format!("vhd:{}", self.info.uuid.as_deref().unwrap_or("unknown")),
            device_id: None,
        }
    }

    fn description(&self) -> crate::source::SourceDescription {
        crate::source::SourceDescription {
            identity: self.identity(),
            sector_size: 512,
            read_only: true,
            warnings: vec![format!(
                "VHD virtual disk: {} bytes virtual, {} bytes physical",
                self.info.virtual_size, self.info.physical_size
            )],
        }
    }
}

impl VhdReader {
    fn read_dynamic(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        // Dynamic VHD:
        // - Footer at end (512 bytes)
        // - BAT (Block Allocation Table) at data_offset from footer
        // - Each block = 2 MB + sector bitmap (1 sector per 2KB chunk)

        const BLOCK_SIZE: u64 = 2 * 1024 * 1024;
        let bat_offset = self.footer.data_offset;
        let bat_entries = self.footer.current_size.div_ceil(BLOCK_SIZE);

        // Read BAT (array of u32 block offsets in sectors)
        let bat_size = bat_entries * 4;
        let bat_data = self.file.read_vec(bat_offset, bat_size)?;

        let mut total_read = 0;
        let mut buf_offset = 0;
        let mut remaining = buf.len();
        let mut current_offset = offset;

        while remaining > 0 {
            let block_index = current_offset / BLOCK_SIZE;
            let block_offset = current_offset % BLOCK_SIZE;
            let chunk_in_block = block_offset / 2048; // 2KB chunks
            let _sector_in_block = chunk_in_block; // 1 sector per chunk

            if block_index >= bat_entries {
                // Beyond allocated blocks - zero fill
                let to_fill = remaining.min((BLOCK_SIZE - block_offset) as usize);
                buf[buf_offset..buf_offset + to_fill].fill(0);
                buf_offset += to_fill;
                remaining -= to_fill;
                current_offset += to_fill as u64;
                continue;
            }

            let bat_entry = u32::from_be_bytes([
                bat_data[(block_index * 4) as usize],
                bat_data[(block_index * 4 + 1) as usize],
                bat_data[(block_index * 4 + 2) as usize],
                bat_data[(block_index * 4 + 3) as usize],
            ]);

            if bat_entry == 0xFFFFFFFF {
                // Unallocated block - zero fill
                let to_fill = remaining.min((BLOCK_SIZE - block_offset) as usize);
                buf[buf_offset..buf_offset + to_fill].fill(0);
                buf_offset += to_fill;
                remaining -= to_fill;
                current_offset += to_fill as u64;
                continue;
            }

            // Block data starts at bat_entry * 512 (sector)
            // First sector is bitmap, then data sectors
            let block_data_offset = (bat_entry as u64) * 512 + 512; // Skip bitmap
            let data_offset_in_block = block_offset;
            let absolute_offset = block_data_offset + data_offset_in_block;

            let to_read = remaining.min((BLOCK_SIZE - block_offset) as usize);
            let n = self
                .file
                .read_at(absolute_offset, &mut buf[buf_offset..buf_offset + to_read])?;

            buf_offset += n;
            remaining -= n;
            current_offset += n as u64;
            total_read += n;

            if n < to_read {
                break; // EOF
            }
        }

        Ok(total_read)
    }
}

impl VirtualDiskReader for VhdReader {
    fn info(&self) -> &VirtualDiskInfo {
        &self.info
    }
    fn format(&self) -> VirtualDiskFormat {
        VirtualDiskFormat::Vhd
    }
}

fn format_uuid(bytes: &[u8]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[3], bytes[2], bytes[1], bytes[0],
        bytes[5], bytes[4],
        bytes[7], bytes[6],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

// ============================================================================
// VHDX (Hyper-V Virtual Hard Disk v2)
// ============================================================================

struct VhdxReader {
    file: FileSource,
    info: VirtualDiskInfo,
    header: VhdxHeader,
    bat: Vec<u64>, // Block allocation table (file offsets)
    block_size: u32,
    logical_sector_size: u32,
    physical_sector_size: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct VhdxHeader {
    signature: [u8; 8], // "vhdxfile"
    checksum: u32,
    sequence: u64,
    file_write_guid: [u8; 16],
    data_write_guid: [u8; 16],
    log_guid: [u8; 16],
    log_version: u32,
    version: u32,
    log_length: u64,
    log_offset: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct VhdxRegionEntry {
    guid: [u8; 16],
    file_offset: u64,
    length: u64,
    required: u32,
    reserved: u32,
}

impl VhdxReader {
    fn open(path: &Path) -> Result<Self> {
        let file = FileSource::open(path)?;
        let file_len = file.len();

        if file_len < 4096 {
            return Err(Error::Corrupt("VHDX file too small".into()));
        }

        // VHDX has headers at 0 and 64 MB
        let file_len = file.len();
        let header_data = file.read_vec(0, 4096)?;
        if header_data.len() < 4096 {
            return Err(Error::Corrupt("Cannot read VHDX header".into()));
        }

        if &header_data[0..8] != b"vhdxfile" {
            return Err(Error::Corrupt("Invalid VHDX signature".into()));
        }

        let header = Self::parse_header(&header_data)?;

        // Read region table
        let region_data = file.read_vec(header.log_offset, header.log_length)?;
        let region_table = Self::parse_region_table(&region_data)?;

        // Find BAT and metadata regions
        // GUIDs in binary format (little-endian for first three components)
        const BAT_REGION_GUID: [u8; 16] = [
            0x66, 0x77, 0xc2, 0x2d, 0x23, 0xf6, 0x00, 0x42, 0x9d, 0x64, 0x11, 0x5e, 0x9b, 0xfd,
            0x4a, 0x08,
        ];
        const METADATA_REGION_GUID: [u8; 16] = [
            0x06, 0xa2, 0x7c, 0x8b, 0x90, 0x47, 0x9a, 0x4b, 0xb8, 0xfe, 0x57, 0x5f, 0x05, 0x0f,
            0x88, 0x6e,
        ];

        let mut bat_region = None;
        let mut metadata_region = None;

        for entry in &region_table {
            if entry.guid == BAT_REGION_GUID {
                bat_region = Some(*entry);
            } else if entry.guid == METADATA_REGION_GUID {
                metadata_region = Some(*entry);
            }
        }

        let mut reader = Self {
            file,
            info: VirtualDiskInfo::default(),
            header,
            bat: Vec::new(),
            block_size: 0,
            logical_sector_size: 512,
            physical_sector_size: 4096,
        };

        // Read BAT
        if let Some(bat_reg) = bat_region {
            reader.read_bat(&bat_reg)?;
        }

        // Read metadata for virtual size, block size, etc.
        if let Some(meta_reg) = metadata_region {
            reader.read_metadata(&meta_reg)?;
        }

        reader.info.format = VirtualDiskFormat::Vhdx;
        reader.info.virtual_size = reader.virtual_size();
        reader.info.physical_size = file_len;
        reader.info.block_size = reader.block_size as u64;
        reader.info.sector_size = reader.logical_sector_size;
        reader.info.description = "VHDX (Hyper-V Virtual Hard Disk v2)".into();
        reader.info.is_sparse = true;
        reader.info.uuid = Some(format_uuid(&reader.header.file_write_guid));

        Ok(reader)
    }

    fn parse_header(data: &[u8]) -> Result<VhdxHeader> {
        Ok(VhdxHeader {
            signature: data[0..8].try_into().unwrap(),
            checksum: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            sequence: u64::from_le_bytes(data[12..20].try_into().unwrap()),
            file_write_guid: data[20..36].try_into().unwrap(),
            data_write_guid: data[36..52].try_into().unwrap(),
            log_guid: data[52..68].try_into().unwrap(),
            log_version: u32::from_le_bytes(data[68..72].try_into().unwrap()),
            version: u32::from_le_bytes(data[72..76].try_into().unwrap()),
            log_length: u64::from_le_bytes(data[76..84].try_into().unwrap()),
            log_offset: u64::from_le_bytes(data[84..92].try_into().unwrap()),
        })
    }

    fn parse_region_table(data: &[u8]) -> Result<Vec<VhdxRegionEntry>> {
        let mut entries = Vec::new();
        let mut offset = 0;
        while offset + 48 <= data.len() {
            let entry = VhdxRegionEntry {
                guid: data[offset..offset + 16].try_into().unwrap(),
                file_offset: u64::from_le_bytes(data[offset + 16..offset + 24].try_into().unwrap()),
                length: u64::from_le_bytes(data[offset + 24..offset + 32].try_into().unwrap()),
                required: u32::from_le_bytes(data[offset + 32..offset + 36].try_into().unwrap()),
                reserved: u32::from_le_bytes(data[offset + 36..offset + 40].try_into().unwrap()),
            };
            if entry.length == 0 {
                break;
            }
            entries.push(entry);
            offset += 48;
        }
        Ok(entries)
    }

    fn read_bat(&mut self, region: &VhdxRegionEntry) -> Result<()> {
        let bat_data = self.file.read_vec(region.file_offset, region.length)?;
        // BAT is array of u64 file offsets (in bytes), 0 = unallocated
        let entries = bat_data.len() / 8;
        self.bat = (0..entries)
            .map(|i| u64::from_le_bytes(bat_data[i * 8..i * 8 + 8].try_into().unwrap()))
            .collect();
        Ok(())
    }

    fn read_metadata(&mut self, region: &VhdxRegionEntry) -> Result<()> {
        let meta_data = self.file.read_vec(region.file_offset, region.length)?;
        // Parse metadata items (key-value pairs)
        let mut offset = 0;
        while offset + 8 <= meta_data.len() {
            let item_id = u32::from_le_bytes(meta_data[offset..offset + 4].try_into().unwrap());
            let item_len =
                u32::from_le_bytes(meta_data[offset + 4..offset + 8].try_into().unwrap()) as usize;
            if offset + 8 + item_len > meta_data.len() {
                break;
            }

            match item_id {
                1 => {
                    // Virtual disk size
                    if item_len >= 8 {
                        let size = u64::from_le_bytes(
                            meta_data[offset + 8..offset + 16].try_into().unwrap(),
                        );
                        self.info.virtual_size = size;
                    }
                }
                2 => {
                    // Block size
                    if item_len >= 4 {
                        self.block_size = u32::from_le_bytes(
                            meta_data[offset + 8..offset + 12].try_into().unwrap(),
                        );
                    }
                }
                3 => {
                    // Logical sector size
                    if item_len >= 4 {
                        self.logical_sector_size = u32::from_le_bytes(
                            meta_data[offset + 8..offset + 12].try_into().unwrap(),
                        );
                    }
                }
                4 => {
                    // Physical sector size
                    if item_len >= 4 {
                        self.physical_sector_size = u32::from_le_bytes(
                            meta_data[offset + 8..offset + 12].try_into().unwrap(),
                        );
                    }
                }
                _ => {}
            }
            offset += 8 + item_len;
        }
        Ok(())
    }

    fn virtual_size(&self) -> u64 {
        self.info.virtual_size
    }
}

impl ReadOnlySource for VhdxReader {
    fn len(&self) -> u64 {
        self.virtual_size()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.virtual_size() {
            return Ok(0);
        }
        let to_read = buf.len().min((self.virtual_size() - offset) as usize);

        let block_size = self.block_size as u64;
        let mut total_read = 0;
        let mut buf_offset = 0;
        let mut remaining = to_read;
        let mut current_offset = offset;

        while remaining > 0 {
            let block_index = current_offset / block_size;
            let block_offset = current_offset % block_size;

            if block_index >= self.bat.len() as u64 {
                // Beyond BAT - zero fill
                let to_fill = remaining.min((block_size - block_offset) as usize);
                buf[buf_offset..buf_offset + to_fill].fill(0);
                buf_offset += to_fill;
                remaining -= to_fill;
                current_offset += to_fill as u64;
                continue;
            }

            let bat_entry = self.bat[block_index as usize];
            if bat_entry == 0 {
                // Unallocated - zero fill
                let to_fill = remaining.min((block_size - block_offset) as usize);
                buf[buf_offset..buf_offset + to_fill].fill(0);
                buf_offset += to_fill;
                remaining -= to_fill;
                current_offset += to_fill as u64;
                continue;
            }

            // Data is at bat_entry (file offset)
            let absolute_offset = bat_entry + block_offset;
            let to_read_now = remaining.min((block_size - block_offset) as usize);
            let n = self.file.read_at(
                absolute_offset,
                &mut buf[buf_offset..buf_offset + to_read_now],
            )?;

            buf_offset += n;
            remaining -= n;
            current_offset += n as u64;
            total_read += n;

            if n < to_read_now {
                break;
            }
        }

        Ok(total_read)
    }

    fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        let n = self.read_at(offset, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn identity(&self) -> SourceIdentity {
        SourceIdentity {
            kind: SourceKind::Image,
            path: self.file.identity().path.clone(),
            size: self.virtual_size(),
            fingerprint: format!("vhdx:{}", self.info.uuid.as_deref().unwrap_or("unknown")),
            device_id: None,
        }
    }

    fn description(&self) -> crate::source::SourceDescription {
        crate::source::SourceDescription {
            identity: self.identity(),
            sector_size: self.logical_sector_size,
            read_only: true,
            warnings: vec![format!(
                "VHDX virtual disk: {} bytes virtual",
                self.virtual_size()
            )],
        }
    }
}

impl VirtualDiskReader for VhdxReader {
    fn info(&self) -> &VirtualDiskInfo {
        &self.info
    }
    fn format(&self) -> VirtualDiskFormat {
        VirtualDiskFormat::Vhdx
    }
}

// ============================================================================
// VMDK (VMware Virtual Machine Disk)
// ============================================================================

struct VmdkReader {
    file: FileSource,
    info: VirtualDiskInfo,
    descriptor: VmdkDescriptor,
    extent_files: Vec<FileSource>,
    grain_size: u64,
    grain_table_entries: u64,
    grain_tables: Vec<Vec<u64>>, // Per extent
    is_sparse: bool,
}

#[derive(Debug, Clone)]
struct VmdkDescriptor {
    version: u32,
    create_type: String,
    cid: u32,
    parent_cid: u32,
    parent_filename: String,
    num_sectors: u64,
    sector_size: u32,
    extent_lines: Vec<VmdkExtent>,
    geometry: VmdkGeometry,
}

#[derive(Debug, Clone)]
struct VmdkExtent {
    size_sectors: u64,
    type_: String,
    filename: String,
}

#[derive(Debug, Clone)]
struct VmdkGeometry {
    cylinders: u32,
    heads: u32,
    sectors: u32,
}

impl VmdkReader {
    fn open(path: &Path) -> Result<Self> {
        let file = FileSource::open(path)?;
        let file_len = file.len();
        let data = file.read_vec(0, file_len.min(1024 * 1024))?;

        // Check if it's a monolithic flat VMDK (no descriptor)
        if data.len() >= 4 && &data[0..4] == b"KDMV" {
            // Sparse extent header
            return Self::open_sparse_extent(file, path);
        }

        // Text descriptor format
        let text = String::from_utf8_lossy(&data);
        let descriptor = Self::parse_descriptor(&text)?;

        if descriptor.extent_lines.iter().any(|extent| {
            !extent.type_.eq_ignore_ascii_case("FLAT")
                && !extent.type_.eq_ignore_ascii_case("SPARSE")
        }) {
            return Err(Error::NotImplemented(
                "VMDK extent type is not supported".into(),
            ));
        }

        let mut extent_files = Vec::new();
        let mut total_sectors = 0u64;

        for extent in &descriptor.extent_lines {
            let extent_path = if extent.filename.is_empty() || extent.filename == ".." {
                path.to_path_buf()
            } else {
                path.parent()
                    .unwrap_or(Path::new(""))
                    .join(&extent.filename)
            };
            extent_files.push(FileSource::open(&extent_path)?);
            total_sectors += extent.size_sectors;
        }

        let mut reader = Self {
            file,
            info: VirtualDiskInfo::default(),
            descriptor: descriptor.clone(),
            extent_files,
            grain_size: 0,
            grain_table_entries: 0,
            grain_tables: Vec::new(),
            is_sparse: descriptor.create_type.contains("sparse")
                || descriptor.create_type.contains("streamOptimized"),
        };

        // For sparse VMDKs, read grain tables
        if reader.is_sparse {
            reader.read_grain_tables()?;
        }

        reader.info.format = VirtualDiskFormat::Vmdk;
        reader.info.virtual_size = total_sectors * descriptor.sector_size as u64;
        reader.info.physical_size = file_len;
        reader.info.sector_size = descriptor.sector_size;
        reader.info.block_size = reader.grain_size;
        reader.info.description = format!("VMDK ({})", descriptor.create_type);
        reader.info.is_sparse = reader.is_sparse;
        reader.info.parent = if descriptor.parent_filename.is_empty() {
            None
        } else {
            Some(PathBuf::from(&descriptor.parent_filename))
        };

        Ok(reader)
    }

    fn open_sparse_extent(file: FileSource, _path: &Path) -> Result<Self> {
        // Monolithic sparse VMDK - header at start
        let file_len = file.len();
        let header_data = file.read_vec(0, 1024)?;
        if header_data.len() < 1024 {
            return Err(Error::Corrupt("VMDK sparse header too small".into()));
        }

        // KDMV header
        if &header_data[0..4] != b"KDMV" {
            return Err(Error::Corrupt("Invalid VMDK sparse header".into()));
        }

        let _version = u32::from_le_bytes(header_data[4..8].try_into().unwrap());
        let _flags = u32::from_le_bytes(header_data[8..12].try_into().unwrap());
        let capacity = u64::from_le_bytes(header_data[12..20].try_into().unwrap());
        let grain_size = u64::from_le_bytes(header_data[20..28].try_into().unwrap());
        let descriptor_offset = u64::from_le_bytes(header_data[28..36].try_into().unwrap());
        let descriptor_size = u64::from_le_bytes(header_data[36..44].try_into().unwrap());
        let num_gtes_per_gt = u32::from_le_bytes(header_data[44..48].try_into().unwrap());
        let rgd_offset = u64::from_le_bytes(header_data[48..56].try_into().unwrap());
        let gd_offset = u64::from_le_bytes(header_data[56..64].try_into().unwrap());
        let _over_head = u64::from_le_bytes(header_data[64..72].try_into().unwrap());
        let _uncompressed_grain_size = u64::from_le_bytes(header_data[72..80].try_into().unwrap());

        // Read embedded descriptor
        let desc_data = file.read_vec(descriptor_offset, descriptor_size)?;
        let desc_text = String::from_utf8_lossy(&desc_data);
        let descriptor = Self::parse_descriptor(&desc_text)?;

        let mut reader = Self {
            file,
            info: VirtualDiskInfo::default(),
            descriptor: descriptor.clone(),
            extent_files: Vec::new(),
            grain_size,
            grain_table_entries: num_gtes_per_gt as u64,
            grain_tables: Vec::new(),
            is_sparse: true,
        };

        // Read grain directory and grain tables
        reader.read_grain_directory_and_tables(gd_offset, rgd_offset)?;

        reader.info.format = VirtualDiskFormat::Vmdk;
        reader.info.virtual_size = capacity * descriptor.sector_size as u64;
        reader.info.physical_size = file_len;
        reader.info.sector_size = descriptor.sector_size;
        reader.info.block_size = grain_size * descriptor.sector_size as u64;
        reader.info.description = "VMDK (monolithic sparse)".into();
        reader.info.is_sparse = true;

        Ok(reader)
    }

    fn parse_descriptor(text: &str) -> Result<VmdkDescriptor> {
        let mut desc = VmdkDescriptor {
            version: 1,
            create_type: "monolithicFlat".into(),
            cid: 0,
            parent_cid: 0xffffffff,
            parent_filename: String::new(),
            num_sectors: 0,
            sector_size: 512,
            extent_lines: Vec::new(),
            geometry: VmdkGeometry {
                cylinders: 0,
                heads: 0,
                sectors: 0,
            },
        };

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                match key {
                    "version" => desc.version = value.parse().unwrap_or(1),
                    "CID" => {
                        desc.cid =
                            u32::from_str_radix(value.trim_start_matches("0x"), 16).unwrap_or(0)
                    }
                    "parentCID" => {
                        desc.parent_cid = u32::from_str_radix(value.trim_start_matches("0x"), 16)
                            .unwrap_or(0xffffffff)
                    }
                    "parentFileNameHint" => desc.parent_filename = value.into(),
                    "createType" => desc.create_type = value.into(),
                    "RW" => {
                        // RW 12345 FLAT "filename" 0
                        let parts: Vec<&str> = value.split_whitespace().collect();
                        if parts.len() >= 4 {
                            let size_sectors = parts[0].parse().unwrap_or(0);
                            let type_ = parts[1].into();
                            let filename = parts[2].trim_matches('"').into();
                            desc.extent_lines.push(VmdkExtent {
                                size_sectors,
                                type_,
                                filename,
                            });
                            desc.num_sectors += size_sectors;
                        }
                    }
                    "geometry" => {
                        let parts: Vec<&str> = value.split_whitespace().collect();
                        if parts.len() >= 3 {
                            desc.geometry.cylinders = parts[0].parse().unwrap_or(0);
                            desc.geometry.heads = parts[1].parse().unwrap_or(0);
                            desc.geometry.sectors = parts[2].parse().unwrap_or(0);
                        }
                    }
                    "sectorSize" => desc.sector_size = value.parse().unwrap_or(512),
                    _ => {}
                }
            }
        }

        Ok(desc)
    }

    fn read_grain_tables(&mut self) -> Result<()> {
        // For each extent file, read its grain table
        for extent_file in &self.extent_files {
            // VMDK sparse extent has header at start
            let header = extent_file.read_vec(0, 1024)?;
            if header.len() < 1024 {
                continue;
            }
            if &header[0..4] != b"KDMV" {
                continue;
            }

            self.grain_size = u64::from_le_bytes(header[20..28].try_into().unwrap());
            self.grain_table_entries =
                u32::from_le_bytes(header[44..48].try_into().unwrap()) as u64;
            let gd_offset = u64::from_le_bytes(header[56..64].try_into().unwrap());

            // Read grain directory
            let gd_size = (self.grain_table_entries / 512) * 8; // Each GT covers 512 grains
            let gd_data = extent_file.read_vec(gd_offset, gd_size)?;

            // Concatenate all grain tables into a single table per extent
            let mut combined_gt = Vec::new();
            for i in 0..(gd_data.len() / 8) {
                let gt_offset = u64::from_le_bytes(gd_data[i * 8..i * 8 + 8].try_into().unwrap());
                if gt_offset == 0 {
                    continue; // Skip empty
                }
                let gt_data = extent_file.read_vec(gt_offset, self.grain_table_entries * 8)?;
                for j in 0..self.grain_table_entries as usize {
                    let grain_offset =
                        u64::from_le_bytes(gt_data[j * 8..j * 8 + 8].try_into().unwrap());
                    combined_gt.push(grain_offset);
                }
            }
            self.grain_tables.push(combined_gt);
        }
        Ok(())
    }

    fn read_grain_directory_and_tables(&mut self, gd_offset: u64, _rgd_offset: u64) -> Result<()> {
        // Read grain directory
        let gd_data = self.file.read_vec(gd_offset, 512)?;
        let mut combined_gt = Vec::new();
        for i in 0..(gd_data.len() / 8) {
            let gt_offset = u64::from_le_bytes(gd_data[i * 8..i * 8 + 8].try_into().unwrap());
            if gt_offset == 0 {
                continue;
            }
            let gt_data = self
                .file
                .read_vec(gt_offset, self.grain_table_entries * 8)?;
            for j in 0..self.grain_table_entries as usize {
                let grain_offset =
                    u64::from_le_bytes(gt_data[j * 8..j * 8 + 8].try_into().unwrap());
                combined_gt.push(grain_offset);
            }
        }
        self.grain_tables = vec![combined_gt];
        Ok(())
    }
}

impl ReadOnlySource for VmdkReader {
    fn len(&self) -> u64 {
        self.info.virtual_size
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.info.virtual_size {
            return Ok(0);
        }

        // For non-sparse (flat) VMDK, just read from extent files
        if !self.is_sparse {
            return self.read_flat(offset, buf);
        }

        // Sparse VMDK - use grain tables
        self.read_sparse(offset, buf)
    }

    fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        let n = self.read_at(offset, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn identity(&self) -> SourceIdentity {
        SourceIdentity {
            kind: SourceKind::Image,
            path: self.file.identity().path.clone(),
            size: self.info.virtual_size,
            fingerprint: format!("vmdk:{}", self.descriptor.cid),
            device_id: None,
        }
    }

    fn description(&self) -> crate::source::SourceDescription {
        crate::source::SourceDescription {
            identity: self.identity(),
            sector_size: self.descriptor.sector_size,
            read_only: true,
            warnings: vec![format!(
                "VMDK virtual disk: {} bytes virtual",
                self.info.virtual_size
            )],
        }
    }
}

impl VmdkReader {
    fn read_flat(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let _sector_size = self.descriptor.sector_size as u64;
        let mut total_read = 0;
        let mut remaining = buf.len();
        let mut buf_offset = 0;
        let mut current_offset = offset;

        for extent_file in &self.extent_files {
            let extent_len = extent_file.len();
            if current_offset >= extent_len {
                current_offset -= extent_len;
                continue;
            }

            let to_read = remaining.min((extent_len - current_offset) as usize);
            let n =
                extent_file.read_at(current_offset, &mut buf[buf_offset..buf_offset + to_read])?;
            buf_offset += n;
            remaining -= n;
            total_read += n;
            current_offset = 0;

            if remaining == 0 || n < to_read {
                break;
            }
        }

        Ok(total_read)
    }

    fn read_sparse(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let grain_bytes = self.grain_size * self.descriptor.sector_size as u64;
        let mut total_read = 0;
        let mut remaining = buf.len();
        let mut buf_offset = 0;
        let mut current_offset = offset;

        while remaining > 0 {
            let grain_index = current_offset / grain_bytes;
            let grain_offset = current_offset % grain_bytes;

            if self.grain_tables.is_empty() || grain_index >= self.grain_tables[0].len() as u64 {
                // Beyond grain tables - zero fill
                let to_fill = remaining.min((grain_bytes - grain_offset) as usize);
                buf[buf_offset..buf_offset + to_fill].fill(0);
                buf_offset += to_fill;
                remaining -= to_fill;
                current_offset += to_fill as u64;
                continue;
            }

            let grain_entry = self.grain_tables[0][grain_index as usize];
            if grain_entry == 0 || grain_entry == 0xFFFFFFFFFFFFFFFF {
                // Unallocated - zero fill
                let to_fill = remaining.min((grain_bytes - grain_offset) as usize);
                buf[buf_offset..buf_offset + to_fill].fill(0);
                buf_offset += to_fill;
                remaining -= to_fill;
                current_offset += to_fill as u64;
                continue;
            }

            // Grain data is at grain_entry * sector_size
            let absolute_offset = grain_entry * self.descriptor.sector_size as u64 + grain_offset;
            let to_read = remaining.min((grain_bytes - grain_offset) as usize);
            let n = self
                .file
                .read_at(absolute_offset, &mut buf[buf_offset..buf_offset + to_read])?;

            buf_offset += n;
            remaining -= n;
            current_offset += n as u64;
            total_read += n;

            if n < to_read {
                break;
            }
        }

        Ok(total_read)
    }
}

impl VirtualDiskReader for VmdkReader {
    fn info(&self) -> &VirtualDiskInfo {
        &self.info
    }
    fn format(&self) -> VirtualDiskFormat {
        VirtualDiskFormat::Vmdk
    }
}

// ============================================================================
// QCOW/QCOW2 (QEMU Copy On Write)
// ============================================================================

struct QcowReader {
    file: FileSource,
    info: VirtualDiskInfo,
    header: QcowHeader,
    is_qcow2: bool,
}

#[derive(Debug, Clone, Copy)]
struct QcowHeader {
    cluster_bits: u8,
    size: u64,
    crypt_method: u32,
}

impl QcowReader {
    fn open(path: &Path) -> Result<Self> {
        let file = FileSource::open(path)?;
        let file_len = file.len();
        let header_data = file.read_vec(0, 128)?; // Max header size
        if header_data.len() < 72 {
            return Err(Error::Corrupt("QCOW header too small".into()));
        }

        let magic = u32::from_be_bytes(header_data[0..4].try_into().unwrap());
        let is_qcow2 = magic == 0x514649FB; // "QFI\FB"
        let is_qcow1 = magic == 0x514649FA; // "QFI\FA"

        if !is_qcow1 && !is_qcow2 {
            return Err(Error::Corrupt("Invalid QCOW magic".into()));
        }

        let header = Self::parse_header(&header_data, is_qcow2)?;

        let mut reader = Self {
            file,
            info: VirtualDiskInfo::default(),
            header,
            is_qcow2,
        };

        reader.info.format = if is_qcow2 {
            VirtualDiskFormat::Qcow2
        } else {
            VirtualDiskFormat::Qcow
        };
        reader.info.virtual_size = reader.header.size;
        reader.info.physical_size = file_len;
        reader.info.block_size = 1u64 << reader.header.cluster_bits;
        reader.info.sector_size = 512;
        reader.info.description = format!("QCOW{}", if is_qcow2 { "2" } else { "" });
        reader.info.is_sparse = true;
        reader.info.is_compressed = false;
        reader.info.is_encrypted = reader.header.crypt_method != 0;

        Ok(reader)
    }

    fn parse_header(data: &[u8], is_qcow2: bool) -> Result<QcowHeader> {
        let version = u32::from_be_bytes(data[4..8].try_into().unwrap());
        if (is_qcow2 && !(2..=3).contains(&version)) || (!is_qcow2 && version != 1) {
            return Err(Error::Corrupt("Unsupported QCOW version".into()));
        }

        let cluster_bits = data[20];
        if !(9..=21).contains(&cluster_bits) {
            return Err(Error::Corrupt("Invalid QCOW cluster size".into()));
        }

        Ok(QcowHeader {
            cluster_bits,
            size: u64::from_be_bytes(data[24..32].try_into().unwrap()),
            crypt_method: u32::from_be_bytes(data[32..36].try_into().unwrap()),
        })
    }
}

impl ReadOnlySource for QcowReader {
    fn len(&self) -> u64 {
        self.info.virtual_size
    }

    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> Result<usize> {
        // QCOW read implementation would go here
        // This is complex due to cluster allocation, compression, encryption
        // For now, return unimplemented
        Err(Error::NotImplemented(
            "QCOW read not fully implemented yet".into(),
        ))
    }

    fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        let n = self.read_at(offset, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn identity(&self) -> SourceIdentity {
        SourceIdentity {
            kind: SourceKind::Image,
            path: self.file.identity().path.clone(),
            size: self.info.virtual_size,
            fingerprint: format!("qcow{}", if self.is_qcow2 { "2" } else { "" }),
            device_id: None,
        }
    }

    fn description(&self) -> crate::source::SourceDescription {
        crate::source::SourceDescription {
            identity: self.identity(),
            sector_size: 512,
            read_only: true,
            warnings: vec!["QCOW/QCOW2 support is experimental".into()],
        }
    }
}

impl VirtualDiskReader for QcowReader {
    fn info(&self) -> &VirtualDiskInfo {
        &self.info
    }
    fn format(&self) -> VirtualDiskFormat {
        if self.is_qcow2 {
            VirtualDiskFormat::Qcow2
        } else {
            VirtualDiskFormat::Qcow
        }
    }
}

// ============================================================================
// VDI (VirtualBox Disk Image)
// ============================================================================

struct VdiReader {
    file: FileSource,
    info: VirtualDiskInfo,
    header: VdiHeader,
    block_map: Vec<u32>,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct VdiHeader {
    text: [u8; 64], // "<<< Oracle VM VirtualBox Disk Image >>>"
    signature: u32, // 0xBEDA107F
    version: u32,
    header_size: u32,
    image_type: u32, // 1=dynamic, 2=fixed, 3=diff
    flags: u32,
    description: [u8; 256],
    blocks: u32,
    block_size: u32,
    geometry: [u32; 4], // cylinders, heads, sectors, sector_size
    dummy: u32,
    uuid: [u8; 16],
    last_snap_uuid: [u8; 16],
    link_uuid: [u8; 16],
    parent_uuid: [u8; 16],
    lch_uuid: [u8; 16],
    parent_filename: [u8; 256],
}

impl VdiReader {
    fn open(path: &Path) -> Result<Self> {
        let file = FileSource::open(path)?;
        let header_data = file.read_vec(0, 512)?;
        if header_data.len() < 512 {
            return Err(Error::Corrupt("VDI header too small".into()));
        }

        if &header_data[0..8] != b"<<< Ora" {
            return Err(Error::Corrupt("Invalid VDI signature".into()));
        }

        let header = VdiHeader {
            text: header_data[0..64].try_into().unwrap(),
            signature: u32::from_le_bytes(header_data[64..68].try_into().unwrap()),
            version: u32::from_le_bytes(header_data[68..72].try_into().unwrap()),
            header_size: u32::from_le_bytes(header_data[72..76].try_into().unwrap()),
            image_type: u32::from_le_bytes(header_data[76..80].try_into().unwrap()),
            flags: u32::from_le_bytes(header_data[80..84].try_into().unwrap()),
            description: header_data[84..340].try_into().unwrap(),
            blocks: u32::from_le_bytes(header_data[340..344].try_into().unwrap()),
            block_size: u32::from_le_bytes(header_data[344..348].try_into().unwrap()),
            geometry: [
                u32::from_le_bytes(header_data[348..352].try_into().unwrap()),
                u32::from_le_bytes(header_data[352..356].try_into().unwrap()),
                u32::from_le_bytes(header_data[356..360].try_into().unwrap()),
                u32::from_le_bytes(header_data[360..364].try_into().unwrap()),
            ],
            dummy: u32::from_le_bytes(header_data[364..368].try_into().unwrap()),
            uuid: header_data[368..384].try_into().unwrap(),
            last_snap_uuid: header_data[384..400].try_into().unwrap(),
            link_uuid: header_data[400..416].try_into().unwrap(),
            parent_uuid: header_data[416..432].try_into().unwrap(),
            lch_uuid: header_data[432..448].try_into().unwrap(),
            parent_filename: header_data[448..704].try_into().unwrap(),
        };

        if header.signature != 0xBEDA107F {
            return Err(Error::Corrupt("Invalid VDI signature value".into()));
        }

        let file_len = file.len();

        // Read block map (after header)
        let block_map_offset = header.header_size as u64;
        let block_map_size = header.blocks as usize * 4;
        let block_map_data = file.read_vec(block_map_offset, block_map_size as u64)?;
        let block_map = (0..header.blocks as usize)
            .map(|i| u32::from_le_bytes(block_map_data[i * 4..i * 4 + 4].try_into().unwrap()))
            .collect();

        let mut reader = Self {
            file,
            info: VirtualDiskInfo::default(),
            header,
            block_map,
        };

        reader.info.format = VirtualDiskFormat::Vdi;
        reader.info.virtual_size = header.blocks as u64 * header.block_size as u64;
        reader.info.physical_size = file_len;
        reader.info.block_size = header.block_size as u64;
        reader.info.sector_size = header.geometry[3]; // sector_size
        reader.info.description = format!(
            "VDI (type {})",
            match header.image_type {
                1 => "dynamic",
                2 => "fixed",
                3 => "diff",
                _ => "unknown",
            }
        );
        reader.info.is_sparse = header.image_type == 1 || header.image_type == 3;
        reader.info.parent = if header.parent_uuid.iter().all(|&b| b == 0) {
            None
        } else {
            Some(PathBuf::from(
                String::from_utf8_lossy(&header.parent_filename)
                    .trim_end_matches('\0')
                    .to_string(),
            ))
        };
        reader.info.uuid = Some(format_uuid(&header.uuid));

        Ok(reader)
    }
}

impl ReadOnlySource for VdiReader {
    fn len(&self) -> u64 {
        self.info.virtual_size
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.info.virtual_size {
            return Ok(0);
        }

        let block_size = self.header.block_size as u64;
        let mut total_read = 0;
        let mut remaining = buf.len();
        let mut buf_offset = 0;
        let mut current_offset = offset;

        while remaining > 0 {
            let block_index = current_offset / block_size;
            let block_offset = current_offset % block_size;

            if block_index >= self.block_map.len() as u64 {
                break;
            }

            let block_map_entry = self.block_map[block_index as usize];
            if block_map_entry == 0xFFFFFFFF {
                // Unallocated - zero fill
                let to_fill = remaining.min((block_size - block_offset) as usize);
                buf[buf_offset..buf_offset + to_fill].fill(0);
                buf_offset += to_fill;
                remaining -= to_fill;
                current_offset += to_fill as u64;
                continue;
            }

            // Block data offset = block_map_entry * block_size
            let absolute_offset = (block_map_entry as u64) * block_size + block_offset;
            let to_read = remaining.min((block_size - block_offset) as usize);
            let n = self
                .file
                .read_at(absolute_offset, &mut buf[buf_offset..buf_offset + to_read])?;

            buf_offset += n;
            remaining -= n;
            current_offset += n as u64;
            total_read += n;

            if n < to_read {
                break;
            }
        }

        Ok(total_read)
    }

    fn read_vec(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        let n = self.read_at(offset, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    fn identity(&self) -> SourceIdentity {
        SourceIdentity {
            kind: SourceKind::Image,
            path: self.file.identity().path.clone(),
            size: self.info.virtual_size,
            fingerprint: format!("vdi:{}", self.info.uuid.as_deref().unwrap_or("unknown")),
            device_id: None,
        }
    }

    fn description(&self) -> crate::source::SourceDescription {
        crate::source::SourceDescription {
            identity: self.identity(),
            sector_size: self.header.geometry[3],
            read_only: true,
            warnings: vec![format!(
                "VDI virtual disk: {} bytes virtual",
                self.info.virtual_size
            )],
        }
    }
}

impl VirtualDiskReader for VdiReader {
    fn info(&self) -> &VirtualDiskInfo {
        &self.info
    }
    fn format(&self) -> VirtualDiskFormat {
        VirtualDiskFormat::Vdi
    }
}

// ============================================================================
// Forensic Image Formats (E01/Ex01, AFF4) - Placeholders
// ============================================================================

/// Open forensic image formats (E01/Ex01, AFF4)
pub fn open_forensic_image(path: &Path) -> Result<Box<dyn VirtualDiskReader>> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "e01" | "ex01" | "s01" | "l01" => open_ewf(path),
        "aff" | "afm" | "afd" | "aff4" => open_aff(path),
        _ => Err(Error::invalid("Unknown forensic image format")),
    }
}

fn open_ewf(_path: &Path) -> Result<Box<dyn VirtualDiskReader>> {
    Err(Error::NotImplemented(
        "E01/Ex01 (EnCase) support not yet implemented".into(),
    ))
}

fn open_aff(_path: &Path) -> Result<Box<dyn VirtualDiskReader>> {
    Err(Error::NotImplemented(
        "AFF/AFF4 support not yet implemented".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn vhd_fixed_disk() {
        let mut file = NamedTempFile::new().unwrap();
        let mut disk = vec![0u8; 10 * 1024 * 1024]; // 10 MB

        // Write some test data at offset 1 MB
        let test_data = b"TEST DATA IN VHD FIXED DISK";
        disk[1024 * 1024..1024 * 1024 + test_data.len()].copy_from_slice(test_data);

        // Write VHD footer at end
        let footer_start = disk.len() - 512;
        let mut footer = [0u8; 512];
        footer[0..8].copy_from_slice(b"conectix");
        footer[8..12].copy_from_slice(&0u32.to_be_bytes()); // features
        footer[12..16].copy_from_slice(&0x00010000u32.to_be_bytes()); // version
        footer[16..24].copy_from_slice(&0xFFFFFFFFFFFFFFFFu64.to_be_bytes()); // data_offset (fixed)
        footer[24..28].copy_from_slice(&0u32.to_be_bytes()); // timestamp
        footer[28..32].copy_from_slice(b"test"); // creator_app
        footer[32..36].copy_from_slice(&0u32.to_be_bytes()); // creator_version
        footer[36..40].copy_from_slice(b"Wi2k"); // creator_host_os
        footer[40..48].copy_from_slice(&(disk.len() as u64).to_be_bytes()); // original_size
        footer[48..56].copy_from_slice(&(disk.len() as u64).to_be_bytes()); // current_size
        footer[56..60].copy_from_slice(&[0, 0, 0, 0]); // geometry
        footer[60..64].copy_from_slice(&2u32.to_be_bytes()); // disk_type = fixed
        footer[68..84].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]); // UUID
        footer[84] = 0; // saved_state
                        // reserved already zero

        // Compute checksum (skip checksum field at 64-67)
        let mut sum: u32 = 0;
        for (i, byte) in footer.iter().enumerate() {
            if (64..68).contains(&i) {
                continue;
            }
            sum = sum.wrapping_add(*byte as u32);
        }
        footer[64..68].copy_from_slice(&(!sum).to_be_bytes());

        disk[footer_start..footer_start + 512].copy_from_slice(&footer);

        file.write_all(&disk).unwrap();
        file.flush().unwrap();

        // Verify checksum by reading back
        let src = FileSource::open(file.path()).unwrap();
        let file_len = src.len();
        let footer_data = src.read_vec(file_len - 512, 512).unwrap();
        let mut verify_sum: u32 = 0;
        for (i, byte) in footer_data.iter().enumerate() {
            if (64..68).contains(&i) {
                continue;
            }
            verify_sum = verify_sum.wrapping_add(*byte as u32);
        }
        let stored_checksum = u32::from_be_bytes(footer_data[64..68].try_into().unwrap());
        assert_eq!(
            !verify_sum, stored_checksum,
            "Checksum mismatch: computed={:x}, stored={:x}",
            !verify_sum, stored_checksum
        );

        let reader = VhdReader::open(file.path()).unwrap();
        assert_eq!(reader.info.virtual_size, disk.len() as u64);
        assert!(!reader.info.is_sparse);

        // Read test data
        let test_data = b"TEST DATA IN VHD FIXED DISK";
        let mut buf = vec![0u8; test_data.len()];
        let n = reader.read_at(1024 * 1024, &mut buf).unwrap();
        assert_eq!(n, test_data.len());
        assert_eq!(&buf, test_data);
    }
}
