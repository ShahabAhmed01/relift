//! Deterministic filesystem images used by Relift's tests.
//!
//! These are real, structurally valid FAT32 (and later exFAT/NTFS/ext) images
//! built byte by byte. They contain live and deleted files with known content
//! and SHA-256 digests, so recovery is verified by byte comparison rather than
//! by trusting the parser's own output.

use sha2::{Digest, Sha256};

pub const FAT32_BYTES_PER_SECTOR: u32 = 512;
pub const FAT32_SECTORS_PER_CLUSTER: u32 = 1;
pub const FAT32_RESERVED_SECTORS: u32 = 32;
pub const FAT32_NUM_FATS: u32 = 2;
pub const FAT32_TOTAL_SECTORS: u32 = 69_632;
pub const FAT32_FAT_SECTORS: u32 = 536;
pub const FAT32_CLUSTER_SIZE: u32 = FAT32_BYTES_PER_SECTOR * FAT32_SECTORS_PER_CLUSTER;
pub const FAT32_DATA_START: u64 = (FAT32_RESERVED_SECTORS as u64
    + FAT32_NUM_FATS as u64 * FAT32_FAT_SECTORS as u64)
    * FAT32_BYTES_PER_SECTOR as u64;
pub const FAT32_CLUSTERS: u32 =
    FAT32_TOTAL_SECTORS - FAT32_RESERVED_SECTORS - FAT32_NUM_FATS * FAT32_FAT_SECTORS;

#[derive(Debug, Clone)]
pub struct FixtureFile {
    pub path: String,
    pub data: Vec<u8>,
    pub sha256: String,
    pub deleted: bool,
    /// Whether the FAT chain was kept (true) or zeroed like Windows does.
    pub chain_retained: bool,
    pub first_cluster: u32,
}

impl FixtureFile {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[derive(Debug, Clone)]
pub struct Fat32Fixture {
    pub bytes: Vec<u8>,
    pub files: Vec<FixtureFile>,
    pub deleted_name: String,
    pub live_name: String,
}

impl Fat32Fixture {
    pub fn image(&self) -> &[u8] {
        &self.bytes
    }

    pub fn file(&self, path: &str) -> Option<&FixtureFile> {
        self.files.iter().find(|f| f.path == path)
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn pattern(len: usize, seed: u32) -> Vec<u8> {
    (0..len)
        .map(|i| ((i as u32).wrapping_mul(37).wrapping_add(seed) % 251) as u8)
        .collect()
}

struct Fat32Builder {
    bytes: Vec<u8>,
    fat: Vec<u32>,
    next_cluster: u32,
    entries: Vec<(String, Vec<u8>, bool, bool)>, // path, data, deleted, chain_retained
}

impl Fat32Builder {
    fn new() -> Self {
        let total = FAT32_TOTAL_SECTORS as usize * FAT32_BYTES_PER_SECTOR as usize;
        let mut bytes = vec![0u8; total];
        write_boot(&mut bytes);
        let mut fat = vec![0u32; (FAT32_CLUSTERS + 2) as usize];
        fat[0] = 0x0FFF_FFF8;
        fat[1] = 0x0FFF_FFFF;
        fat[2] = 0x0FFF_FFFF; // root directory
        Self {
            bytes,
            fat,
            next_cluster: 3,
            entries: Vec::new(),
        }
    }

    fn alloc(&mut self, data_len: usize) -> Vec<u32> {
        let needed = (data_len.max(1) as u64).div_ceil(FAT32_CLUSTER_SIZE as u64) as u32;
        let mut clusters = Vec::with_capacity(needed as usize);
        for _ in 0..needed {
            let c = self.next_cluster;
            self.next_cluster += 1;
            if c >= FAT32_CLUSTERS + 2 {
                panic!("fat32 fixture exhausted its cluster heap");
            }
            clusters.push(c);
        }
        clusters
    }

    fn add_file(&mut self, path: &str, data: Vec<u8>, deleted: bool, chain_retained: bool) {
        self.entries
            .push((path.to_string(), data, deleted, chain_retained));
    }

    fn write_file_data(&mut self, path: &str, data: &[u8], clusters: &[u32]) {
        let mut left = data.len();
        let mut at = 0usize;
        for c in clusters {
            let off = FAT32_DATA_START as usize + (*c as usize - 2) * FAT32_CLUSTER_SIZE as usize;
            let n = left.min(FAT32_CLUSTER_SIZE as usize);
            self.bytes[off..off + n].copy_from_slice(&data[at..at + n]);
            at += n;
            left -= n;
        }
        if left != 0 {
            panic!("fat32 fixture file {path} did not fit its clusters");
        }
    }

    fn finalize(mut self) -> Fat32Fixture {
        let mut files = Vec::new();
        let mut root_entries: Vec<[u8; 32]> = Vec::new();
        let mut docs_entries: Vec<[u8; 32]> = Vec::new();
        let docs_cluster = self.alloc(512)[0];
        self.fat[docs_cluster as usize] = 0x0FFF_FFFF;
        root_entries.push(dirent(b"RELIFTTEST ", 0x08, 0, 0));

        // Live root file. Allocate before writing the directory entry so the
        // entry carries the real first cluster and byte length.
        let hello = b"hello from the live root\n".to_vec();
        let hc = self.alloc(hello.len());
        for (i, c) in hc.iter().enumerate() {
            self.fat[*c as usize] = if i + 1 == hc.len() {
                0x0FFF_FFFF
            } else {
                hc[i + 1]
            };
        }
        self.write_file_data("/hello.txt", &hello, &hc);
        let mut hello_set = Vec::new();
        push_entry_set(
            &mut hello_set,
            "hello.txt",
            b"HELLO   TXT",
            0x20,
            hc[0],
            hello.len() as u32,
            false,
        );
        root_entries.extend(hello_set);
        let mut docs_set = Vec::new();
        push_entry_set(
            &mut docs_set,
            "docs",
            b"DOCS       ",
            0x10,
            docs_cluster,
            0,
            false,
        );
        root_entries.extend(docs_set);

        let _dir_entries: Vec<[u8; 32]> = vec![
            dirent(b".          ", 0x10, docs_cluster, 0),
            dirent(b"..         ", 0x10, 0, 0),
        ];

        let mut deleted_name = String::new();
        let mut live_name = String::new();

        let entries = std::mem::take(&mut self.entries);
        for (idx, (path, data, deleted, chain_retained)) in entries.into_iter().enumerate() {
            let chain = self.alloc(data.len());
            for (i, c) in chain.iter().enumerate() {
                self.fat[*c as usize] = if !chain_retained {
                    0
                } else if i + 1 == chain.len() {
                    0x0FFF_FFFF
                } else {
                    chain[i + 1]
                };
            }
            self.write_file_data(&path, &data, &chain);
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            let short = short_alias(&name, idx);
            let mut set = Vec::new();
            push_entry_set(
                &mut set,
                &name,
                &short,
                0x20,
                chain[0],
                data.len() as u32,
                deleted,
            );
            if deleted && deleted_name.is_empty() {
                deleted_name = name.clone();
            }
            if !deleted && live_name.is_empty() {
                live_name = name.clone();
            }
            if path.starts_with("/docs/") {
                docs_entries.extend(set);
            } else if path == "/hello.txt" {
                // already emitted above with a hand-built short entry
            } else {
                root_entries.extend(set);
            }
            files.push(FixtureFile {
                path: path.clone(),
                sha256: sha256_hex(&data),
                data,
                deleted,
                chain_retained,
                first_cluster: chain[0],
            });
        }

        root_entries.push([0u8; 32]);
        docs_entries.push([0u8; 32]);
        self.put_directory(2, &root_entries);
        self.put_directory(docs_cluster, &docs_entries);
        self.flush_fat();
        Fat32Fixture {
            bytes: self.bytes,
            files,
            deleted_name,
            live_name,
        }
    }

    fn put_directory(&mut self, cluster: u32, entries: &[[u8; 32]]) {
        let off = FAT32_DATA_START as usize + (cluster as usize - 2) * FAT32_CLUSTER_SIZE as usize;
        for (i, e) in entries.iter().enumerate() {
            let at = off + i * 32;
            self.bytes[at..at + 32].copy_from_slice(e);
        }
    }

    fn flush_fat(&mut self) {
        let fat_off = FAT32_RESERVED_SECTORS as usize * FAT32_BYTES_PER_SECTOR as usize;
        for (i, v) in self.fat.iter().enumerate() {
            let at = fat_off + i * 4;
            if at + 4 <= self.bytes.len() {
                self.bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        let copy_off = fat_off + FAT32_FAT_SECTORS as usize * FAT32_BYTES_PER_SECTOR as usize;
        let len = FAT32_FAT_SECTORS as usize * FAT32_BYTES_PER_SECTOR as usize;
        let first = self.bytes[fat_off..fat_off + len].to_vec();
        self.bytes[copy_off..copy_off + len].copy_from_slice(&first);
    }
}

fn write_boot(bytes: &mut [u8]) {
    bytes[0] = 0xEB;
    bytes[1] = 0x58;
    bytes[2] = 0x90;
    bytes[3..11].copy_from_slice(b"MSWIN4.1");
    bytes[11..13].copy_from_slice(&(FAT32_BYTES_PER_SECTOR as u16).to_le_bytes());
    bytes[13] = FAT32_SECTORS_PER_CLUSTER as u8;
    bytes[14..16].copy_from_slice(&(FAT32_RESERVED_SECTORS as u16).to_le_bytes());
    bytes[16] = FAT32_NUM_FATS as u8;
    bytes[17..19].copy_from_slice(&0u16.to_le_bytes());
    bytes[19..21].copy_from_slice(&0u16.to_le_bytes());
    bytes[21] = 0xF8;
    bytes[22..24].copy_from_slice(&0u16.to_le_bytes());
    bytes[24..26].copy_from_slice(&32u16.to_le_bytes());
    bytes[26..28].copy_from_slice(&64u16.to_le_bytes());
    bytes[28..32].copy_from_slice(&0u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&FAT32_TOTAL_SECTORS.to_le_bytes());
    bytes[36..40].copy_from_slice(&FAT32_FAT_SECTORS.to_le_bytes());
    bytes[40..42].copy_from_slice(&0u16.to_le_bytes());
    bytes[42..44].copy_from_slice(&0u16.to_le_bytes());
    bytes[44..48].copy_from_slice(&2u32.to_le_bytes());
    bytes[48..50].copy_from_slice(&1u16.to_le_bytes());
    bytes[50..52].copy_from_slice(&6u16.to_le_bytes());
    bytes[64] = 0x80;
    bytes[66] = 0x29;
    bytes[67..71].copy_from_slice(&0x5245_4C49u32.to_le_bytes());
    bytes[71..82].copy_from_slice(b"RELIFTTEST ");
    bytes[82..90].copy_from_slice(b"FAT32   ");
    bytes[510] = 0x55;
    bytes[511] = 0xAA;

    // FSInfo at sector 1.
    let fsinfo = FAT32_BYTES_PER_SECTOR as usize;
    bytes[fsinfo..fsinfo + 4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    bytes[fsinfo + 484..fsinfo + 488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    bytes[fsinfo + 488..fsinfo + 492].copy_from_slice(&0u32.to_le_bytes());
    bytes[fsinfo + 492..fsinfo + 496].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    bytes[fsinfo + 508..fsinfo + 512].copy_from_slice(&0xAA55_0000u32.to_le_bytes());

    // Backup boot sector at sector 6 and backup FSInfo at sector 7.
    let boot = bytes[..512].to_vec();
    bytes[6 * 512..7 * 512].copy_from_slice(&boot);
    bytes[7 * 512 + 484..7 * 512 + 488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    bytes[7 * 512 + 508..7 * 512 + 512].copy_from_slice(&0xAA55_0000u32.to_le_bytes());
}

fn dirent(name: &[u8; 11], attr: u8, cluster: u32, size: u32) -> [u8; 32] {
    set_entry(name, attr, cluster, 0, 0, size)
}

fn set_entry(
    name: &[u8; 11],
    attr: u8,
    cluster: u32,
    _unused: u32,
    _unused2: u32,
    size: u32,
) -> [u8; 32] {
    let mut e = [0u8; 32];
    e[..11].copy_from_slice(name);
    e[11] = attr;
    let (date, time) = fat_stamp();
    e[14..16].copy_from_slice(&time.to_le_bytes());
    e[16..18].copy_from_slice(&date.to_le_bytes());
    e[18..20].copy_from_slice(&date.to_le_bytes());
    e[22..24].copy_from_slice(&time.to_le_bytes());
    e[24..26].copy_from_slice(&date.to_le_bytes());
    e[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    e[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    e[28..32].copy_from_slice(&size.to_le_bytes());
    e
}

fn fat_stamp() -> (u16, u16) {
    // 2024-01-02 03:04:06
    let date = ((2024u16 - 1980) << 9) | (1 << 5) | 2;
    let time = (3 << 11) | (4 << 5) | (6 / 2);
    (date, time)
}

fn short_alias(name: &str, idx: usize) -> [u8; 11] {
    let upper = name.to_ascii_uppercase();
    let (stem, ext) = match upper.rsplit_once('.') {
        Some((s, e)) => (s, e),
        None => (upper.as_str(), ""),
    };
    let mut out = [b' '; 11];
    let clean: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if clean.len() <= 8 && ext.len() <= 3 {
        for (i, b) in clean.bytes().take(8).enumerate() {
            out[i] = b;
        }
        for (i, b) in ext.bytes().take(3).enumerate() {
            out[8 + i] = b;
        }
    } else {
        let base = format!("FILE{idx:03}");
        for (i, b) in base.bytes().take(8).enumerate() {
            out[i] = b;
        }
        for (i, b) in ext.bytes().take(3).enumerate() {
            out[8 + i] = b;
        }
    }
    out
}

fn lfn_checksum(short: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for b in short {
        sum = sum.rotate_right(1).wrapping_add(*b);
    }
    sum
}

fn short_display(short: &[u8; 11]) -> String {
    let stem = String::from_utf8_lossy(&short[..8]);
    let ext = String::from_utf8_lossy(&short[8..11]);
    let stem = stem.trim_end_matches(' ');
    let ext = ext.trim_end_matches(' ');
    if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{ext}")
    }
}

fn push_entry_set(
    out: &mut Vec<[u8; 32]>,
    long_name: &str,
    short: &[u8; 11],
    attr: u8,
    cluster: u32,
    size: u32,
    deleted: bool,
) {
    let checksum = lfn_checksum(short);
    let units: Vec<u16> = long_name.encode_utf16().collect();
    let needs_lfn = units.len() > 13 || long_name != short_display(short);
    let lfn_count = if needs_lfn {
        units.len().div_ceil(13)
    } else {
        0
    };
    if needs_lfn {
        let mut lfn_entries = Vec::new();
        for seq in 1..=lfn_count {
            let mut e = [0u8; 32];
            e[0] = seq as u8 | if seq == lfn_count { 0x40 } else { 0 };
            e[11] = 0x0F;
            e[13] = checksum;
            let start = (seq - 1) * 13;
            for k in 0..13 {
                let u = units
                    .get(start + k)
                    .copied()
                    .unwrap_or(if start + k == units.len() { 0 } else { 0xFFFF });
                let at = match k {
                    0..=4 => 1 + k * 2,
                    5..=10 => 14 + (k - 5) * 2,
                    _ => 28 + (k - 11) * 2,
                };
                e[at..at + 2].copy_from_slice(&u.to_le_bytes());
            }
            lfn_entries.push(e);
        }
        if deleted {
            for e in &mut lfn_entries {
                e[0] = 0xE5;
            }
        }
        for e in lfn_entries.into_iter().rev() {
            out.push(e);
        }
    }
    let mut e = set_entry(short, attr, cluster, 0, 0, size);
    if deleted {
        e[0] = 0xE5;
    }
    out.push(e);
}

/// Build a deterministic 34 MiB FAT32 image. It contains:
///
/// - `/hello.txt` — live
/// - `/docs/report.txt` — live in a subdirectory
/// - `/docs/deleted_notes.txt` — deleted, FAT chain cleared
/// - `/holiday photo 2024.jpg` — deleted, LFN, FAT chain cleared
/// - `/archive.bin` — deleted, FAT chain retained
pub fn fat32_image() -> Fat32Fixture {
    let mut b = Fat32Builder::new();
    b.add_file(
        "/docs/report.txt",
        b"quarterly report: 42\n".to_vec(),
        false,
        true,
    );
    b.add_file(
        "/docs/deleted_notes.txt",
        b"these notes were deleted but the clusters still hold them\n".to_vec(),
        true,
        false,
    );
    b.add_file("/holiday photo 2024.jpg", pattern(4096, 7), true, false);
    b.add_file("/archive.bin", pattern(1536, 19), true, true);
    b.finalize()
}

// ---------------------------------------------------------------------------
// exFAT fixture
// ---------------------------------------------------------------------------

pub const EXFAT_BYTES_PER_SECTOR: u32 = 512;
pub const EXFAT_SECTORS_PER_CLUSTER: u32 = 1;
pub const EXFAT_CLUSTER_SIZE: u32 = EXFAT_BYTES_PER_SECTOR * EXFAT_SECTORS_PER_CLUSTER;
pub const EXFAT_FAT_OFFSET: u32 = 24;
pub const EXFAT_FAT_LENGTH: u32 = 33;
pub const EXFAT_HEAP_OFFSET: u32 = EXFAT_FAT_OFFSET + EXFAT_FAT_LENGTH;
pub const EXFAT_CLUSTERS: u32 = 4096;
pub const EXFAT_TOTAL_SECTORS: u32 = EXFAT_HEAP_OFFSET + EXFAT_CLUSTERS;
pub const EXFAT_DATA_START: u64 = EXFAT_HEAP_OFFSET as u64 * EXFAT_BYTES_PER_SECTOR as u64;

#[derive(Debug, Clone)]
pub struct ExfatFixture {
    pub bytes: Vec<u8>,
    pub files: Vec<FixtureFile>,
    pub deleted_name: String,
    pub live_name: String,
}

impl ExfatFixture {
    pub fn image(&self) -> &[u8] {
        &self.bytes
    }

    pub fn file(&self, path: &str) -> Option<&FixtureFile> {
        self.files.iter().find(|f| f.path == path)
    }
}

struct ExfatBuilder {
    bytes: Vec<u8>,
    fat: Vec<u32>,
    next_cluster: u32,
    entries: Vec<(String, Vec<u8>, bool, bool)>, // path, data, deleted, no_fat_chain
}

impl ExfatBuilder {
    fn new() -> Self {
        let total = EXFAT_TOTAL_SECTORS as usize * EXFAT_BYTES_PER_SECTOR as usize;
        let mut bytes = vec![0u8; total];
        write_exfat_boot(&mut bytes);
        let mut fat = vec![0u32; (EXFAT_CLUSTERS + 2) as usize];
        fat[0] = 0xFFFF_FFF8;
        fat[1] = 0xFFFF_FFFF;
        fat[2] = 0xFFFF_FFFF; // root directory
        Self {
            bytes,
            fat,
            next_cluster: 3,
            entries: Vec::new(),
        }
    }

    fn alloc(&mut self, count: u32) -> Vec<u32> {
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let c = self.next_cluster;
            self.next_cluster += 1;
            if c >= EXFAT_CLUSTERS + 2 {
                panic!("exFAT fixture exhausted its cluster heap");
            }
            out.push(c);
        }
        out
    }

    fn write_clusters(&mut self, data: &[u8], clusters: &[u32]) {
        let mut at = 0usize;
        for c in clusters {
            let off = EXFAT_DATA_START as usize + (*c as usize - 2) * EXFAT_CLUSTER_SIZE as usize;
            let n = (data.len() - at).min(EXFAT_CLUSTER_SIZE as usize);
            self.bytes[off..off + n].copy_from_slice(&data[at..at + n]);
            at += n;
            if at >= data.len() {
                break;
            }
        }
    }

    fn write_dir(&mut self, cluster: u32, entries: &[[u8; 32]]) {
        let off = EXFAT_DATA_START as usize + (cluster as usize - 2) * EXFAT_CLUSTER_SIZE as usize;
        for (i, e) in entries.iter().enumerate() {
            let at = off + i * 32;
            self.bytes[at..at + 32].copy_from_slice(e);
        }
    }

    fn flush_fat(&mut self) {
        let fat_off = EXFAT_FAT_OFFSET as usize * EXFAT_BYTES_PER_SECTOR as usize;
        for (i, v) in self.fat.iter().enumerate() {
            let at = fat_off + i * 4;
            if at + 4 <= self.bytes.len() {
                self.bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
    }

    fn add_file(&mut self, path: &str, data: Vec<u8>, deleted: bool, no_fat_chain: bool) {
        self.entries
            .push((path.to_string(), data, deleted, no_fat_chain));
    }

    fn finalize(mut self) -> ExfatFixture {
        let root_cluster = 2u32;
        // Allocate and write the live root file first.
        let hello = b"hello from exFAT\n".to_vec();
        let hello_clusters = self.alloc(1);
        self.write_clusters(&hello, &hello_clusters);

        // Live subdirectory /docs.
        let docs_cluster = self.alloc(1)[0];
        self.fat[docs_cluster as usize] = 0xFFFF_FFFF;

        let mut root_entries: Vec<[u8; 32]> = Vec::new();
        root_entries.push(exfat_label_entry("RELIFTEXFAT"));

        let mut files: Vec<FixtureFile> = Vec::new();
        files.push(FixtureFile {
            path: "/hello.txt".to_string(),
            sha256: sha256_hex(&hello),
            data: hello.clone(),
            deleted: false,
            chain_retained: true,
            first_cluster: hello_clusters[0],
        });
        exfat_push_set(
            &mut root_entries,
            "hello.txt",
            0x20,
            hello_clusters[0],
            hello.len() as u64,
            false,
            true,
            0,
        );
        exfat_push_set(
            &mut root_entries,
            "docs",
            0x10,
            docs_cluster,
            EXFAT_CLUSTER_SIZE as u64,
            false,
            true,
            0,
        );

        let mut docs_entries: Vec<[u8; 32]> = Vec::new();
        let mut deleted_name = String::new();
        let mut live_name = "hello.txt".to_string();

        let entries = std::mem::take(&mut self.entries);
        for (idx, (path, data, deleted, no_fat_chain)) in entries.into_iter().enumerate() {
            let needed = (data.len() as u64)
                .max(1)
                .div_ceil(EXFAT_CLUSTER_SIZE as u64);
            let clusters = self.alloc(needed as u32);
            for (i, c) in clusters.iter().enumerate() {
                if no_fat_chain {
                    self.fat[*c as usize] = 0;
                } else if deleted {
                    // Windows clears the chain on delete; the data clusters
                    // themselves are left untouched.
                    self.fat[*c as usize] = 0;
                } else if i + 1 == clusters.len() {
                    self.fat[*c as usize] = 0xFFFF_FFFF;
                } else {
                    self.fat[*c as usize] = clusters[i + 1];
                }
            }
            self.write_clusters(&data, &clusters);
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            let mut set = Vec::new();
            exfat_push_set(
                &mut set,
                &name,
                0x20,
                clusters[0],
                data.len() as u64,
                deleted,
                no_fat_chain,
                idx as u32,
            );
            if path.starts_with("/docs/") {
                docs_entries.extend(set);
            } else {
                root_entries.extend(set);
            }
            if deleted && deleted_name.is_empty() {
                deleted_name = name.clone();
            }
            if !deleted && live_name == "hello.txt" {
                live_name = name.clone();
            }
            files.push(FixtureFile {
                path: path.clone(),
                sha256: sha256_hex(&data),
                data,
                deleted,
                chain_retained: !deleted || no_fat_chain,
                first_cluster: clusters[0],
            });
        }

        root_entries.push([0u8; 32]);
        docs_entries.push([0u8; 32]);
        self.write_dir(root_cluster, &root_entries);
        self.write_dir(docs_cluster, &docs_entries);
        self.flush_fat();

        ExfatFixture {
            bytes: self.bytes,
            files,
            deleted_name,
            live_name,
        }
    }
}

fn exfat_timestamp() -> u32 {
    ((2024u32 - 1980) << 25) | (1 << 21) | (2 << 16) | (3 << 11) | (4 << 5) | 3
}

fn exfat_label_entry(label: &str) -> [u8; 32] {
    let mut e = [0u8; 32];
    e[0] = 0x83;
    for (i, u) in label.encode_utf16().take(11).enumerate() {
        e[2 + i * 2..4 + i * 2].copy_from_slice(&u.to_le_bytes());
    }
    e
}

fn exfat_set_checksum(entries: &[[u8; 32]]) -> u16 {
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

// Fixture helper mirroring the exFAT directory-entry fields one-to-one.
#[allow(clippy::too_many_arguments)]
fn exfat_push_set(
    out: &mut Vec<[u8; 32]>,
    name: &str,
    attributes: u16,
    first_cluster: u32,
    data_length: u64,
    deleted: bool,
    no_fat_chain: bool,
    _serial: u32,
) {
    let units: Vec<u16> = name.encode_utf16().collect();
    let name_entries = units.len().div_ceil(15).max(1);
    let secondary = (1 + name_entries) as u8;

    let mut file = [0u8; 32];
    file[0] = if deleted { 0x05 } else { 0x85 };
    file[1] = secondary;
    file[4..6].copy_from_slice(&attributes.to_le_bytes());
    let stamp = exfat_timestamp();
    file[8..12].copy_from_slice(&stamp.to_le_bytes());
    file[12..16].copy_from_slice(&stamp.to_le_bytes());
    file[16..20].copy_from_slice(&stamp.to_le_bytes());

    let mut stream = [0u8; 32];
    stream[0] = if deleted { 0x40 } else { 0xC0 };
    stream[1] = if no_fat_chain { 0x02 } else { 0x00 };
    stream[3] = units.len() as u8;
    stream[8..16].copy_from_slice(&data_length.to_le_bytes());
    stream[20..24].copy_from_slice(&first_cluster.to_le_bytes());
    stream[24..32].copy_from_slice(&data_length.to_le_bytes());

    let mut names: Vec<[u8; 32]> = Vec::new();
    for seq in 0..name_entries {
        let mut e = [0u8; 32];
        e[0] = if deleted { 0x41 } else { 0xC1 };
        for k in 0..15 {
            let idx = seq * 15 + k;
            if idx >= units.len() {
                break;
            }
            let at = 2 + k * 2;
            e[at..at + 2].copy_from_slice(&units[idx].to_le_bytes());
        }
        names.push(e);
    }

    let mut all = Vec::with_capacity(2 + names.len());
    all.push(file);
    all.push(stream);
    all.extend(names);
    let checksum = exfat_set_checksum(&all);
    all[0][2..4].copy_from_slice(&checksum.to_le_bytes());
    out.extend(all);
}

fn write_exfat_boot(bytes: &mut [u8]) {
    bytes[0] = 0xEB;
    bytes[1] = 0x76;
    bytes[2] = 0x90;
    bytes[3..11].copy_from_slice(b"EXFAT   ");
    bytes[64..72].copy_from_slice(&0u64.to_le_bytes());
    bytes[72..80].copy_from_slice(&(EXFAT_TOTAL_SECTORS as u64).to_le_bytes());
    bytes[80..84].copy_from_slice(&EXFAT_FAT_OFFSET.to_le_bytes());
    bytes[84..88].copy_from_slice(&EXFAT_FAT_LENGTH.to_le_bytes());
    bytes[88..92].copy_from_slice(&EXFAT_HEAP_OFFSET.to_le_bytes());
    bytes[92..96].copy_from_slice(&EXFAT_CLUSTERS.to_le_bytes());
    bytes[96..100].copy_from_slice(&2u32.to_le_bytes());
    bytes[100..104].copy_from_slice(&0x5245_4C49u32.to_le_bytes());
    bytes[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
    bytes[106..108].copy_from_slice(&0u16.to_le_bytes());
    bytes[108] = 9; // 512-byte sectors
    bytes[109] = 0; // 1 sector per cluster
    bytes[110] = 1; // one FAT
    bytes[111] = 0x80;
    bytes[112] = 1;
    bytes[510] = 0x55;
    bytes[511] = 0xAA;

    // The 12-sector boot region: main at 0, backup at 12, and a repeating
    // checksum in the last sector of each region.
    let main = bytes[..512].to_vec();
    let backup_off = 12 * 512;
    bytes[backup_off..backup_off + 512].copy_from_slice(&main);

    let mut checksum = 0u32;
    for i in 0..11 {
        for b in &bytes[i * 512..(i + 1) * 512] {
            checksum = checksum.rotate_right(1).wrapping_add(*b as u32);
        }
    }
    for chunk in bytes[11 * 512..12 * 512].chunks_exact_mut(4) {
        chunk.copy_from_slice(&checksum.to_le_bytes());
    }
    for chunk in bytes[(backup_off + 11 * 512)..(backup_off + 12 * 512)].chunks_exact_mut(4) {
        chunk.copy_from_slice(&checksum.to_le_bytes());
    }
}

/// Build a deterministic exFAT image. It contains:
///
/// - `/hello.txt` — live
/// - `/docs/report.txt` — live in a subdirectory
/// - `/docs/deleted_notes.txt` — deleted, retained contiguous layout
/// - `/holiday photo 2024.jpg` — deleted, multi-entry name, NoFatChain
/// - `/archive.bin` — deleted, FAT chain cleared like Windows leaves it
pub fn exfat_image() -> ExfatFixture {
    let mut b = ExfatBuilder::new();
    b.add_file(
        "/docs/report.txt",
        b"exFAT quarterly report: 42\n".to_vec(),
        false,
        true,
    );
    b.add_file(
        "/docs/deleted_notes.txt",
        b"deleted exFAT notes remain in the cluster heap\n".to_vec(),
        true,
        true,
    );
    b.add_file("/holiday photo 2024.jpg", pattern(4096, 41), true, true);
    b.add_file("/archive.bin", pattern(1536, 67), true, false);
    b.finalize()
}

// ---------------------------------------------------------------------------
// NTFS fixture
// ---------------------------------------------------------------------------

pub const NTFS_BYTES_PER_SECTOR: u32 = 512;
pub const NTFS_SECTORS_PER_CLUSTER: u32 = 1;
pub const NTFS_CLUSTER_SIZE: u32 = NTFS_BYTES_PER_SECTOR * NTFS_SECTORS_PER_CLUSTER;
pub const NTFS_TOTAL_SECTORS: u32 = 2048;
pub const NTFS_MFT_LCN: u64 = 16;
pub const NTFS_MFT_RECORDS: u64 = 64;
pub const NTFS_RECORD_SIZE: u32 = 1024;
pub const NTFS_MFT_CLUSTERS: u64 = 128;
pub const NTFS_FILETIME_2024: u64 = 133_486_382_460_000_000;

#[derive(Debug, Clone)]
pub struct NtfsFixture {
    pub bytes: Vec<u8>,
    pub files: Vec<FixtureFile>,
    pub deleted_name: String,
    pub live_name: String,
}

impl NtfsFixture {
    pub fn image(&self) -> &[u8] {
        &self.bytes
    }

    pub fn file(&self, path: &str) -> Option<&FixtureFile> {
        self.files.iter().find(|f| f.path == path)
    }
}

struct NtfsBuilder {
    bytes: Vec<u8>,
}

impl NtfsBuilder {
    fn new() -> Self {
        let total = NTFS_TOTAL_SECTORS as usize * NTFS_BYTES_PER_SECTOR as usize;
        let mut bytes = vec![0u8; total];
        write_ntfs_boot(&mut bytes);
        Self { bytes }
    }

    fn cluster_off(&self, lcn: u64) -> usize {
        (lcn * NTFS_CLUSTER_SIZE as u64) as usize
    }

    fn write_clusters(&mut self, lcn: u64, data: &[u8]) {
        let off = self.cluster_off(lcn);
        self.bytes[off..off + data.len()].copy_from_slice(data);
    }

    fn write_record(&mut self, index: u64, record: Vec<u8>) {
        assert_eq!(record.len(), NTFS_RECORD_SIZE as usize);
        let off = NTFS_MFT_LCN as usize * NTFS_CLUSTER_SIZE as usize
            + index as usize * NTFS_RECORD_SIZE as usize;
        self.bytes[off..off + record.len()].copy_from_slice(&record);
    }
}

fn ntfs_record_base(number: u64, in_use: bool, is_dir: bool, sequence: u16) -> Vec<u8> {
    let mut record = vec![0u8; 56];
    record[0..4].copy_from_slice(b"FILE");
    record[4..6].copy_from_slice(&48u16.to_le_bytes()); // USA offset
    record[6..8].copy_from_slice(&3u16.to_le_bytes()); // 2 sectors + USN
    record[16..18].copy_from_slice(&sequence.to_le_bytes());
    record[18..20].copy_from_slice(&1u16.to_le_bytes());
    record[20..22].copy_from_slice(&56u16.to_le_bytes()); // attributes offset
    let flags = (if in_use { 1u16 } else { 0 }) | (if is_dir { 2 } else { 0 });
    record[22..24].copy_from_slice(&flags.to_le_bytes());
    record[28..32].copy_from_slice(&NTFS_RECORD_SIZE.to_le_bytes());
    record[44..48].copy_from_slice(&(number as u32).to_le_bytes());
    record
}

fn ntfs_finish_record(mut record: Vec<u8>) -> Vec<u8> {
    // End marker.
    record.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    while record.len() % 8 != 0 {
        record.push(0);
    }
    let used = record.len() as u32;
    record.resize(NTFS_RECORD_SIZE as usize, 0);
    record[24..28].copy_from_slice(&used.to_le_bytes());
    // Capture the two sector-tail words, install the update sequence number,
    // and store the originals in the USA array.
    let tail1 = u16::from_le_bytes([record[510], record[511]]);
    let tail2 = u16::from_le_bytes([record[1022], record[1023]]);
    let usn = 1u16;
    record[48..50].copy_from_slice(&usn.to_le_bytes());
    record[50..52].copy_from_slice(&tail1.to_le_bytes());
    record[52..54].copy_from_slice(&tail2.to_le_bytes());
    record[510..512].copy_from_slice(&usn.to_le_bytes());
    record[1022..1024].copy_from_slice(&usn.to_le_bytes());
    record
}

fn ntfs_push_attr(record: &mut Vec<u8>, attr: &[u8]) {
    record.extend_from_slice(attr);
    while record.len() % 8 != 0 {
        record.push(0);
    }
}

fn ntfs_standard_info() -> Vec<u8> {
    let mut value = vec![0u8; 48];
    for at in [0usize, 8, 16, 24] {
        value[at..at + 8].copy_from_slice(&NTFS_FILETIME_2024.to_le_bytes());
    }
    let mut attr = vec![0u8; 24];
    attr[0..4].copy_from_slice(&0x10u32.to_le_bytes());
    attr[4..8].copy_from_slice(&((24 + value.len()) as u32).to_le_bytes());
    attr[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes());
    attr[20..22].copy_from_slice(&24u16.to_le_bytes());
    attr.extend_from_slice(&value);
    attr
}

fn ntfs_file_name(name: &str, parent: u64, real_size: u64, namespace: u8) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut value = vec![0u8; 66 + units.len() * 2];
    let parent_ref = parent | (1u64 << 48);
    value[0..8].copy_from_slice(&parent_ref.to_le_bytes());
    for at in [8usize, 16, 24, 32] {
        value[at..at + 8].copy_from_slice(&NTFS_FILETIME_2024.to_le_bytes());
    }
    value[40..48].copy_from_slice(&real_size.to_le_bytes());
    value[48..56].copy_from_slice(&real_size.to_le_bytes());
    value[56..60].copy_from_slice(&0x20u32.to_le_bytes());
    value[64] = units.len() as u8;
    value[65] = namespace;
    for (i, u) in units.iter().enumerate() {
        value[66 + i * 2..68 + i * 2].copy_from_slice(&u.to_le_bytes());
    }
    let mut attr = vec![0u8; 24];
    attr[0..4].copy_from_slice(&0x30u32.to_le_bytes());
    attr[4..8].copy_from_slice(&((24 + value.len()) as u32).to_le_bytes());
    attr[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes());
    attr[20..22].copy_from_slice(&24u16.to_le_bytes());
    attr.extend_from_slice(&value);
    attr
}

fn ntfs_resident_data(data: &[u8], named: bool) -> Vec<u8> {
    let name: Vec<u16> = if named {
        "$ALTERNATE".encode_utf16().collect()
    } else {
        Vec::new()
    };
    let mut attr = vec![0u8; 24];
    attr[0..4].copy_from_slice(&0x80u32.to_le_bytes());
    attr[8] = 0;
    attr[9] = name.len() as u8;
    attr[10..12].copy_from_slice(&24u16.to_le_bytes());
    attr[16..20].copy_from_slice(&(data.len() as u32).to_le_bytes());
    let value_off = 24 + name.len() * 2;
    attr[20..22].copy_from_slice(&(value_off as u16).to_le_bytes());
    attr.resize(value_off, 0);
    for (i, u) in name.iter().enumerate() {
        attr[24 + i * 2..26 + i * 2].copy_from_slice(&u.to_le_bytes());
    }
    attr.extend_from_slice(data);
    let len = attr.len() as u32;
    attr[4..8].copy_from_slice(&len.to_le_bytes());
    attr
}

fn ntfs_runlist(runs: &[(u64, Option<u64>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut previous_lcn: i64 = 0;
    for (clusters, lcn) in runs {
        let len_bytes = if *clusters <= 0xFF {
            1
        } else if *clusters <= 0xFFFF {
            2
        } else if *clusters <= 0xFF_FFFF {
            3
        } else {
            4
        };
        match lcn {
            Some(lcn) => {
                let delta = *lcn as i64 - previous_lcn;
                previous_lcn = *lcn as i64;
                let off_bytes = if (-128..=127).contains(&delta) {
                    1
                } else if (-32768..=32767).contains(&delta) {
                    2
                } else if (-8_388_608..=8_388_607).contains(&delta) {
                    3
                } else {
                    4
                };
                out.push((off_bytes << 4) as u8 | len_bytes);
                for i in 0..len_bytes {
                    out.push(((*clusters >> (i * 8)) & 0xFF) as u8);
                }
                let raw = delta as u64;
                for i in 0..off_bytes {
                    out.push(((raw >> (i * 8)) & 0xFF) as u8);
                }
            }
            None => {
                out.push(len_bytes);
                for i in 0..len_bytes {
                    out.push(((*clusters >> (i * 8)) & 0xFF) as u8);
                }
            }
        }
    }
    out.push(0);
    out
}

fn ntfs_nonresident_data(
    runs: &[(u64, Option<u64>)],
    real_size: u64,
    initialized_size: u64,
) -> Vec<u8> {
    let runlist = ntfs_runlist(runs);
    let clusters: u64 = runs.iter().map(|(c, _)| *c).sum();
    let mut attr = vec![0u8; 64];
    attr[0..4].copy_from_slice(&0x80u32.to_le_bytes());
    attr[8] = 1;
    attr[16..24].copy_from_slice(&0u64.to_le_bytes()); // lowest VCN
    attr[24..32].copy_from_slice(&(clusters - 1).to_le_bytes());
    attr[32..34].copy_from_slice(&64u16.to_le_bytes()); // mapping pairs
    attr[40..48].copy_from_slice(&(clusters * NTFS_CLUSTER_SIZE as u64).to_le_bytes());
    attr[48..56].copy_from_slice(&real_size.to_le_bytes());
    attr[56..64].copy_from_slice(&initialized_size.to_le_bytes());
    attr.extend_from_slice(&runlist);
    while attr.len() % 8 != 0 {
        attr.push(0);
    }
    let len = attr.len() as u32;
    attr[4..8].copy_from_slice(&len.to_le_bytes());
    attr
}

fn ntfs_volume_name(label: &str) -> Vec<u8> {
    let units: Vec<u16> = label.encode_utf16().collect();
    let mut value = vec![0u8; units.len() * 2];
    for (i, u) in units.iter().enumerate() {
        value[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
    }
    let mut attr = vec![0u8; 24];
    attr[0..4].copy_from_slice(&0x60u32.to_le_bytes());
    attr[4..8].copy_from_slice(&((24 + value.len()) as u32).to_le_bytes());
    attr[16..20].copy_from_slice(&(value.len() as u32).to_le_bytes());
    attr[20..22].copy_from_slice(&24u16.to_le_bytes());
    attr.extend_from_slice(&value);
    attr
}

fn write_ntfs_boot(bytes: &mut [u8]) {
    bytes[0] = 0xEB;
    bytes[1] = 0x52;
    bytes[2] = 0x90;
    bytes[3..11].copy_from_slice(b"NTFS    ");
    bytes[11..13].copy_from_slice(&(NTFS_BYTES_PER_SECTOR as u16).to_le_bytes());
    bytes[13] = NTFS_SECTORS_PER_CLUSTER as u8;
    bytes[40..48].copy_from_slice(&(NTFS_TOTAL_SECTORS as u64).to_le_bytes());
    bytes[48..56].copy_from_slice(&NTFS_MFT_LCN.to_le_bytes());
    bytes[56..64].copy_from_slice(&(NTFS_MFT_LCN + NTFS_MFT_CLUSTERS).to_le_bytes());
    bytes[64] = (-10i8) as u8; // 1024-byte MFT record
    bytes[68] = 1; // 512-byte index record when positive
    bytes[72..80].copy_from_slice(&0x5245_4C49_4654_0001u64.to_le_bytes());
    bytes[510] = 0x55;
    bytes[511] = 0xAA;
}

/// Build a deterministic NTFS image with resident and non-resident deleted
/// files, a deleted parent directory, sparse runs and a fragmented runlist.
pub fn ntfs_image() -> NtfsFixture {
    let mut b = NtfsBuilder::new();
    let mut files = Vec::new();

    // Record 0: $MFT itself, with a non-resident run over the whole table.
    let mut rec = ntfs_record_base(0, true, false, 1);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(
        &mut rec,
        &ntfs_file_name("$MFT", 5, NTFS_MFT_RECORDS * 1024, 1),
    );
    ntfs_push_attr(
        &mut rec,
        &ntfs_nonresident_data(
            &[(NTFS_MFT_CLUSTERS, Some(NTFS_MFT_LCN))],
            NTFS_MFT_RECORDS * 1024,
            NTFS_MFT_RECORDS * 1024,
        ),
    );
    b.write_record(0, ntfs_finish_record(rec));

    // Record 3: volume name.
    let mut rec = ntfs_record_base(3, true, false, 1);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(&mut rec, &ntfs_file_name("$Volume", 3, 0, 1));
    ntfs_push_attr(&mut rec, &ntfs_volume_name("RELIFTNTFS"));
    b.write_record(3, ntfs_finish_record(rec));

    // Record 5: root directory.
    let mut rec = ntfs_record_base(5, true, true, 1);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(&mut rec, &ntfs_file_name(".", 5, 0, 1));
    b.write_record(5, ntfs_finish_record(rec));

    // Record 16: live /docs directory.
    let mut rec = ntfs_record_base(16, true, true, 1);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(&mut rec, &ntfs_file_name("docs", 5, 0, 1));
    b.write_record(16, ntfs_finish_record(rec));

    // Record 17: live file that must not be emitted by default.
    let live = b"this file is still allocated\n".to_vec();
    let mut rec = ntfs_record_base(17, true, false, 1);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(
        &mut rec,
        &ntfs_file_name("live.txt", 5, live.len() as u64, 1),
    );
    ntfs_push_attr(&mut rec, &ntfs_resident_data(&live, false));
    b.write_record(17, ntfs_finish_record(rec));
    files.push(FixtureFile {
        path: "/live.txt".to_string(),
        sha256: sha256_hex(&live),
        data: live,
        deleted: false,
        chain_retained: true,
        first_cluster: 0,
    });

    // Record 18: deleted resident file.
    let deleted = b"deleted NTFS resident content\n".to_vec();
    let mut rec = ntfs_record_base(18, false, false, 2);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(
        &mut rec,
        &ntfs_file_name("deleted.txt", 5, deleted.len() as u64, 1),
    );
    ntfs_push_attr(&mut rec, &ntfs_resident_data(&deleted, false));
    b.write_record(18, ntfs_finish_record(rec));
    files.push(FixtureFile {
        path: "/deleted.txt".to_string(),
        sha256: sha256_hex(&deleted),
        data: deleted,
        deleted: true,
        chain_retained: true,
        first_cluster: 0,
    });

    // Record 19: deleted non-resident JPEG under /docs.
    let photo = pattern(4096, 83);
    b.write_clusters(200, &photo);
    let mut rec = ntfs_record_base(19, false, false, 3);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(
        &mut rec,
        &ntfs_file_name("photo.jpg", 16, photo.len() as u64, 1),
    );
    ntfs_push_attr(
        &mut rec,
        &ntfs_nonresident_data(&[(8, Some(200))], photo.len() as u64, photo.len() as u64),
    );
    b.write_record(19, ntfs_finish_record(rec));
    files.push(FixtureFile {
        path: "/docs/photo.jpg".to_string(),
        sha256: sha256_hex(&photo),
        data: photo,
        deleted: true,
        chain_retained: true,
        first_cluster: 200,
    });

    // Record 20: deleted sparse file (data, hole, data).
    b.write_clusters(300, &pattern(512, 91));
    b.write_clusters(301, &pattern(512, 92));
    let mut rec = ntfs_record_base(20, false, false, 2);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(&mut rec, &ntfs_file_name("sparse.bin", 5, 1536, 1));
    ntfs_push_attr(
        &mut rec,
        &ntfs_nonresident_data(&[(1, Some(300)), (1, None), (1, Some(301))], 1536, 1536),
    );
    b.write_record(20, ntfs_finish_record(rec));
    let mut sparse_expected = pattern(512, 91);
    sparse_expected.extend(std::iter::repeat_n(0u8, 512));
    sparse_expected.extend(pattern(512, 92));
    files.push(FixtureFile {
        path: "/sparse.bin".to_string(),
        sha256: sha256_hex(&sparse_expected),
        data: sparse_expected,
        deleted: true,
        chain_retained: true,
        first_cluster: 300,
    });

    // Record 21: deleted file whose parent directory is also deleted.
    let hidden = b"a file inside a deleted directory\n".to_vec();
    let mut rec = ntfs_record_base(21, false, false, 2);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(
        &mut rec,
        &ntfs_file_name("hidden.txt", 22, hidden.len() as u64, 1),
    );
    ntfs_push_attr(&mut rec, &ntfs_resident_data(&hidden, false));
    b.write_record(21, ntfs_finish_record(rec));
    files.push(FixtureFile {
        path: "/old dir/hidden.txt".to_string(),
        sha256: sha256_hex(&hidden),
        data: hidden,
        deleted: true,
        chain_retained: true,
        first_cluster: 0,
    });

    // Record 22: deleted directory.
    let mut rec = ntfs_record_base(22, false, true, 2);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(&mut rec, &ntfs_file_name("old dir", 5, 0, 1));
    b.write_record(22, ntfs_finish_record(rec));

    // Record 23: deleted fragmented non-resident file.
    let mut fragmented = pattern(512, 101);
    fragmented.extend(pattern(512, 102));
    b.write_clusters(310, &fragmented[..512]);
    b.write_clusters(320, &fragmented[512..]);
    let mut rec = ntfs_record_base(23, false, false, 4);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(&mut rec, &ntfs_file_name("archive.bin", 5, 1024, 1));
    ntfs_push_attr(
        &mut rec,
        &ntfs_nonresident_data(&[(1, Some(310)), (1, Some(320))], 1024, 1024),
    );
    b.write_record(23, ntfs_finish_record(rec));
    files.push(FixtureFile {
        path: "/archive.bin".to_string(),
        sha256: sha256_hex(&fragmented),
        data: fragmented,
        deleted: true,
        chain_retained: true,
        first_cluster: 310,
    });

    // Record 24: deleted file with no $DATA attribute.
    let mut rec = ntfs_record_base(24, false, false, 2);
    ntfs_push_attr(&mut rec, &ntfs_standard_info());
    ntfs_push_attr(&mut rec, &ntfs_file_name("no-content.bin", 5, 4096, 1));
    b.write_record(24, ntfs_finish_record(rec));

    NtfsFixture {
        bytes: b.bytes,
        files,
        deleted_name: "deleted.txt".to_string(),
        live_name: "live.txt".to_string(),
    }
}

// ---------------------------------------------------------------------------
// ext4 fixture
// ---------------------------------------------------------------------------

pub const EXT4_BLOCK_SIZE: u32 = 1024;
pub const EXT4_TOTAL_BLOCKS: u32 = 1024;
pub const EXT4_INODE_COUNT: u32 = 64;
pub const EXT4_INODE_SIZE: u32 = 256;
pub const EXT4_BLOCK_BITMAP: u32 = 3;
pub const EXT4_INODE_BITMAP: u32 = 4;
pub const EXT4_INODE_TABLE: u32 = 5;
pub const EXT4_INODE_TABLE_BLOCKS: u32 = (EXT4_INODE_COUNT * EXT4_INODE_SIZE) / EXT4_BLOCK_SIZE;
pub const EXT4_FIRST_DATA_BLOCK: u32 = 21;
pub const EXT4_TIMESTAMP: u32 = 1_704_164_646; // 2024-01-02T03:04:06Z

#[derive(Debug, Clone)]
pub struct Ext4Fixture {
    pub bytes: Vec<u8>,
    pub files: Vec<FixtureFile>,
    pub deleted_name: String,
    pub live_name: String,
}

impl Ext4Fixture {
    pub fn image(&self) -> &[u8] {
        &self.bytes
    }

    pub fn file(&self, path: &str) -> Option<&FixtureFile> {
        self.files.iter().find(|f| f.path == path)
    }
}

#[derive(Debug, Clone)]
struct Ext4Inode {
    mode: u16,
    size: u64,
    links: u16,
    sectors: u32,
    flags: u32,
    i_block: [u8; 60],
    dtime: u32,
}

impl Ext4Inode {
    fn empty() -> Self {
        Self {
            mode: 0,
            size: 0,
            links: 0,
            sectors: 0,
            flags: 0,
            i_block: [0u8; 60],
            dtime: 0,
        }
    }
}

struct Ext4Builder {
    bytes: Vec<u8>,
    next_block: u32,
    block_used: Vec<bool>,
    inode_used: Vec<bool>,
    inodes: Vec<Option<Ext4Inode>>,
}

impl Ext4Builder {
    fn new() -> Self {
        let total = EXT4_TOTAL_BLOCKS as usize * EXT4_BLOCK_SIZE as usize;
        let mut builder = Self {
            bytes: vec![0u8; total],
            next_block: EXT4_FIRST_DATA_BLOCK,
            block_used: vec![false; EXT4_TOTAL_BLOCKS as usize],
            inode_used: vec![false; EXT4_INODE_COUNT as usize],
            inodes: vec![None; EXT4_INODE_COUNT as usize],
        };
        for block in 0..EXT4_FIRST_DATA_BLOCK {
            builder.block_used[block as usize] = true;
        }
        assert_eq!(
            EXT4_INODE_TABLE + EXT4_INODE_TABLE_BLOCKS,
            EXT4_FIRST_DATA_BLOCK,
            "fixture metadata layout must leave the inode table inside the reserved area"
        );
        write_ext4_superblock(&mut builder.bytes);
        write_ext4_group_descriptor(&mut builder.bytes);
        builder
    }

    fn alloc_block(&mut self) -> u32 {
        let block = self.next_block;
        self.next_block += 1;
        if block >= EXT4_TOTAL_BLOCKS {
            panic!("ext4 fixture exhausted its block heap");
        }
        self.block_used[block as usize] = true;
        block
    }

    fn write_block(&mut self, block: u32, data: &[u8]) {
        let off = block as usize * EXT4_BLOCK_SIZE as usize;
        let n = data.len().min(EXT4_BLOCK_SIZE as usize);
        self.bytes[off..off + n].copy_from_slice(&data[..n]);
    }

    fn set_inode(&mut self, number: u32, inode: Ext4Inode, allocated: bool) {
        self.inodes[(number - 1) as usize] = Some(inode);
        if allocated {
            self.inode_used[(number - 1) as usize] = true;
        }
    }

    fn flush_metadata(&mut self) {
        for (number, inode) in self.inodes.iter().enumerate() {
            let Some(inode) = inode else { continue };
            let off = EXT4_INODE_TABLE as usize * EXT4_BLOCK_SIZE as usize
                + number * EXT4_INODE_SIZE as usize;
            write_ext4_inode(&mut self.bytes[off..off + EXT4_INODE_SIZE as usize], inode);
        }
        let bitmap_off = EXT4_BLOCK_BITMAP as usize * EXT4_BLOCK_SIZE as usize;
        for (block, used) in self.block_used.iter().enumerate() {
            if *used {
                self.bytes[bitmap_off + block / 8] |= 1 << (block % 8);
            }
        }
        let bitmap_off = EXT4_INODE_BITMAP as usize * EXT4_BLOCK_SIZE as usize;
        for (inode, used) in self.inode_used.iter().enumerate() {
            if *used {
                self.bytes[bitmap_off + inode / 8] |= 1 << (inode % 8);
            }
        }
    }
}

fn write_ext4_superblock(bytes: &mut [u8]) {
    let sb = &mut bytes[EXT4_BLOCK_SIZE as usize..2 * EXT4_BLOCK_SIZE as usize];
    sb[0..4].copy_from_slice(&EXT4_INODE_COUNT.to_le_bytes());
    sb[4..8].copy_from_slice(&EXT4_TOTAL_BLOCKS.to_le_bytes());
    sb[8..12].copy_from_slice(&0u32.to_le_bytes());
    sb[12..16].copy_from_slice(&(EXT4_TOTAL_BLOCKS - EXT4_FIRST_DATA_BLOCK).to_le_bytes());
    sb[16..20].copy_from_slice(&(EXT4_INODE_COUNT - 8).to_le_bytes());
    sb[20..24].copy_from_slice(&1u32.to_le_bytes()); // first data block (1 KiB blocks)
    sb[24..28].copy_from_slice(&0u32.to_le_bytes()); // log block size
    sb[28..32].copy_from_slice(&0u32.to_le_bytes());
    sb[32..36].copy_from_slice(&8192u32.to_le_bytes());
    sb[36..40].copy_from_slice(&8192u32.to_le_bytes());
    sb[40..44].copy_from_slice(&EXT4_INODE_COUNT.to_le_bytes());
    sb[44..48].copy_from_slice(&EXT4_TIMESTAMP.to_le_bytes());
    sb[48..52].copy_from_slice(&EXT4_TIMESTAMP.to_le_bytes());
    sb[52..54].copy_from_slice(&1u16.to_le_bytes());
    sb[54..56].copy_from_slice(&0xFFFFu16.to_le_bytes());
    sb[56..58].copy_from_slice(&0xEF53u16.to_le_bytes());
    sb[58..60].copy_from_slice(&1u16.to_le_bytes());
    sb[60..62].copy_from_slice(&1u16.to_le_bytes());
    sb[76..80].copy_from_slice(&1u32.to_le_bytes()); // dynamic revision
    sb[84..88].copy_from_slice(&11u32.to_le_bytes()); // first non-reserved inode
    sb[88..90].copy_from_slice(&(EXT4_INODE_SIZE as u16).to_le_bytes());
    // INCOMPAT_FILETYPE | INCOMPAT_EXTENTS | INCOMPAT_64BIT
    sb[96..100].copy_from_slice(&0x00C2u32.to_le_bytes());
    // SPARSE_SUPER | LARGE_FILE | DIR_NLINK | EXTRA_ISIZE
    sb[100..104].copy_from_slice(&0x0063u32.to_le_bytes());
    sb[104..120].copy_from_slice(&0x5245_4C49_4654_4558u128.to_le_bytes());
    let label = b"RELIFTEXT4";
    sb[120..120 + label.len()].copy_from_slice(label);
    sb[254..256].copy_from_slice(&64u16.to_le_bytes()); // 64-byte group descriptors
    sb[336..340].copy_from_slice(&0u32.to_le_bytes()); // blocks_count_hi
}

fn write_ext4_group_descriptor(bytes: &mut [u8]) {
    let base = 2 * EXT4_BLOCK_SIZE as usize;
    let desc = &mut bytes[base..base + 64];
    desc[0..4].copy_from_slice(&EXT4_BLOCK_BITMAP.to_le_bytes());
    desc[4..8].copy_from_slice(&EXT4_INODE_BITMAP.to_le_bytes());
    desc[8..12].copy_from_slice(&EXT4_INODE_TABLE.to_le_bytes());
    desc[12..14].copy_from_slice(&32u16.to_le_bytes()); // free blocks
    desc[14..16].copy_from_slice(&16u16.to_le_bytes()); // free inodes
    desc[16..18].copy_from_slice(&1u16.to_le_bytes()); // used dirs
    desc[18..20].copy_from_slice(&0u16.to_le_bytes()); // flags
    desc[36..40].copy_from_slice(&0u32.to_le_bytes()); // inode bitmap hi
    desc[40..44].copy_from_slice(&0u32.to_le_bytes()); // inode table hi
}

fn write_ext4_inode(raw: &mut [u8], inode: &Ext4Inode) {
    raw.fill(0);
    raw[0..2].copy_from_slice(&inode.mode.to_le_bytes());
    raw[4..8].copy_from_slice(&(inode.size as u32).to_le_bytes());
    for at in [8usize, 12, 16] {
        raw[at..at + 4].copy_from_slice(&EXT4_TIMESTAMP.to_le_bytes());
    }
    raw[20..24].copy_from_slice(&inode.dtime.to_le_bytes());
    raw[26..28].copy_from_slice(&inode.links.to_le_bytes());
    raw[28..32].copy_from_slice(&inode.sectors.to_le_bytes());
    raw[32..36].copy_from_slice(&inode.flags.to_le_bytes());
    raw[40..100].copy_from_slice(&inode.i_block);
    raw[128..130].copy_from_slice(&32u16.to_le_bytes()); // extra_isize
    raw[144..148].copy_from_slice(&EXT4_TIMESTAMP.to_le_bytes()); // crtime
}

fn ext4_extent_tree(runs: &[(u32, u16, u64)]) -> [u8; 60] {
    let mut out = [0u8; 60];
    out[0..2].copy_from_slice(&0xF30Au16.to_le_bytes());
    out[2..4].copy_from_slice(&(runs.len() as u16).to_le_bytes());
    out[4..6].copy_from_slice(&4u16.to_le_bytes());
    for (index, (logical, len, physical)) in runs.iter().enumerate() {
        let at = 12 + index * 12;
        out[at..at + 4].copy_from_slice(&logical.to_le_bytes());
        out[at + 4..at + 6].copy_from_slice(&len.to_le_bytes());
        out[at + 6..at + 8].copy_from_slice(&((physical >> 32) as u16).to_le_bytes());
        out[at + 8..at + 12].copy_from_slice(&(*physical as u32).to_le_bytes());
    }
    out
}

fn ext4_direct_block(blocks: &[u32], single_indirect: u32) -> [u8; 60] {
    let mut out = [0u8; 60];
    for (index, block) in blocks.iter().take(12).enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&block.to_le_bytes());
    }
    out[48..52].copy_from_slice(&single_indirect.to_le_bytes());
    out
}

fn ext4_dirent(out: &mut [u8], at: usize, name: &str, inode: u32, file_type: u8, last: bool) {
    let rec_len = if last {
        EXT4_BLOCK_SIZE as usize - at
    } else {
        (8 + name.len() + 3) & !3
    };
    out[at..at + 4].copy_from_slice(&inode.to_le_bytes());
    out[at + 4..at + 6].copy_from_slice(&(rec_len as u16).to_le_bytes());
    out[at + 6] = name.len() as u8;
    out[at + 7] = file_type;
    out[at + 8..at + 8 + name.len()].copy_from_slice(name.as_bytes());
}

fn ext4_directory(entries: &[(&str, u32, u8)]) -> Vec<u8> {
    let mut out = vec![0u8; EXT4_BLOCK_SIZE as usize];
    let mut at = 0usize;
    for (index, (name, inode, file_type)) in entries.iter().enumerate() {
        ext4_dirent(
            &mut out,
            at,
            name,
            *inode,
            *file_type,
            index + 1 == entries.len(),
        );
        at += (8 + name.len() + 3) & !3;
    }
    out
}

/// Build a deterministic 1 MiB ext4 image. It contains:
///
/// - `/hello.txt` — live
/// - `/docs/report.txt` — live in a subdirectory
/// - `/deleted.txt` — deleted, extent tree retained
/// - `/holiday photo 2024.jpg` — deleted, fragmented extent tree
/// - `/docs/deleted_notes.txt` — deleted, stale directory entry
/// - `/sparse.bin` — deleted, a logical hole between two extents
/// - `/unwritten.bin` — deleted, an unwritten extent whose physical block
///   contains nonzero garbage that must not be emitted
/// - `/indirect.bin` — deleted, ext2-style direct + single-indirect mapping
/// - `inode_20` — deleted orphan with no directory entry
/// - `/zero.bin` — deleted empty file
/// - `/old dir/old.txt` — deleted file inside a deleted (stale) directory
pub fn ext4_image() -> Ext4Fixture {
    let mut b = Ext4Builder::new();
    let mut files = Vec::new();

    let root_block = b.alloc_block();
    let docs_block = b.alloc_block();
    let hello_block = b.alloc_block();
    let deleted_block = b.alloc_block();
    let photo_block_a = b.alloc_block();
    let report_block = b.alloc_block();
    let photo_block_b = b.alloc_block();
    let sparse_block_a = b.alloc_block();
    let sparse_block_b = b.alloc_block();
    let notes_block = b.alloc_block();
    let unwritten_block = b.alloc_block();
    let mut indirect_blocks = Vec::new();
    for _ in 0..12 {
        indirect_blocks.push(b.alloc_block());
    }
    let single_indirect = b.alloc_block();
    let indirect_tail = b.alloc_block();
    let orphan_block = b.alloc_block();
    let old_dir_block = b.alloc_block();
    let old_file_block = b.alloc_block();

    let hello = b"hello from the ext4 fixture\n".to_vec();
    let deleted = b"deleted ext4 content survives in the block\n".to_vec();
    let photo = pattern(2048, 113);
    let report = b"ext4 quarterly report: 42\n".to_vec();
    let notes = b"deleted notes inside docs on ext4\n".to_vec();
    let sparse_head = pattern(1024, 127);
    let sparse_tail = pattern(1024, 129);
    let orphan = pattern(300, 139);
    let old = b"file inside a stale deleted directory\n".to_vec();
    let indirect = pattern(13 * EXT4_BLOCK_SIZE as usize, 133);

    b.write_block(hello_block, &hello);
    b.write_block(deleted_block, &deleted);
    b.write_block(photo_block_a, &photo[..1024]);
    b.write_block(photo_block_b, &photo[1024..]);
    b.write_block(report_block, &report);
    b.write_block(sparse_block_a, &sparse_head);
    b.write_block(sparse_block_b, &sparse_tail);
    b.write_block(notes_block, &notes);
    // Unwritten extents must read back as zeroes. Put recognizable garbage in
    // the physical block so a parser that ignores the unwritten flag fails.
    b.write_block(unwritten_block, &pattern(1024, 131));
    for (index, block) in indirect_blocks.iter().enumerate() {
        b.write_block(*block, &indirect[index * 1024..(index + 1) * 1024]);
    }
    let mut indirect_table = vec![0u8; EXT4_BLOCK_SIZE as usize];
    indirect_table[0..4].copy_from_slice(&indirect_tail.to_le_bytes());
    b.write_block(single_indirect, &indirect_table);
    b.write_block(indirect_tail, &indirect[12 * 1024..]);
    b.write_block(orphan_block, &orphan);
    b.write_block(old_file_block, &old);

    let root_entries = [
        (".", 2u32, 2u8),
        ("..", 2, 2),
        ("hello.txt", 11, 1),
        ("docs", 12, 2),
        ("deleted.txt", 13, 1),
        ("holiday photo 2024.jpg", 14, 1),
        ("sparse.bin", 17, 1),
        ("unwritten.bin", 18, 1),
        ("indirect.bin", 19, 1),
        ("zero.bin", 21, 1),
        ("old dir", 22, 2),
    ];
    let docs_entries = [
        (".", 12u32, 2u8),
        ("..", 2, 2),
        ("report.txt", 15, 1),
        ("deleted_notes.txt", 16, 1),
    ];
    let old_entries = [(".", 22u32, 2u8), ("..", 2, 2), ("old.txt", 23, 1)];
    b.write_block(root_block, &ext4_directory(&root_entries));
    b.write_block(docs_block, &ext4_directory(&docs_entries));
    b.write_block(old_dir_block, &ext4_directory(&old_entries));

    let dir = |size: u64, block: u32| Ext4Inode {
        mode: 0o040755,
        size,
        links: 2,
        sectors: 2,
        flags: 0x0008_0000,
        i_block: ext4_extent_tree(&[(0, 1, block as u64)]),
        dtime: 0,
    };
    let file = |mode: u16, size: u64, sectors: u32, i_block: [u8; 60]| Ext4Inode {
        mode,
        size,
        links: 1,
        sectors,
        flags: 0x0008_0000,
        i_block,
        dtime: 0,
    };

    b.set_inode(1, Ext4Inode::empty(), true);
    b.set_inode(2, dir(EXT4_BLOCK_SIZE as u64, root_block), true);
    b.set_inode(
        11,
        file(
            0o100644,
            hello.len() as u64,
            2,
            ext4_extent_tree(&[(0, 1, hello_block as u64)]),
        ),
        true,
    );
    b.set_inode(12, dir(EXT4_BLOCK_SIZE as u64, docs_block), true);
    let mut deleted_inode = file(
        0o100644,
        deleted.len() as u64,
        2,
        ext4_extent_tree(&[(0, 1, deleted_block as u64)]),
    );
    deleted_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(13, deleted_inode, false);
    let mut photo_inode = file(
        0o100644,
        photo.len() as u64,
        4,
        ext4_extent_tree(&[(0, 1, photo_block_a as u64), (1, 1, photo_block_b as u64)]),
    );
    photo_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(14, photo_inode, false);
    b.set_inode(
        15,
        file(
            0o100644,
            report.len() as u64,
            2,
            ext4_extent_tree(&[(0, 1, report_block as u64)]),
        ),
        true,
    );
    let mut notes_inode = file(
        0o100644,
        notes.len() as u64,
        2,
        ext4_extent_tree(&[(0, 1, notes_block as u64)]),
    );
    notes_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(16, notes_inode, false);
    let mut sparse_inode = file(
        0o100644,
        3072,
        4,
        ext4_extent_tree(&[(0, 1, sparse_block_a as u64), (2, 1, sparse_block_b as u64)]),
    );
    sparse_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(17, sparse_inode, false);
    let mut unwritten_inode = file(
        0o100644,
        1024,
        2,
        ext4_extent_tree(&[(0, 32769, unwritten_block as u64)]),
    );
    unwritten_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(18, unwritten_inode, false);
    let mut indirect_inode = file(
        0o100644,
        indirect.len() as u64,
        28,
        ext4_direct_block(&indirect_blocks, single_indirect),
    );
    indirect_inode.flags = 0;
    indirect_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(19, indirect_inode, false);
    let mut orphan_inode = file(
        0o100644,
        orphan.len() as u64,
        2,
        ext4_extent_tree(&[(0, 1, orphan_block as u64)]),
    );
    orphan_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(20, orphan_inode, false);
    let mut zero_inode = file(0o100644, 0, 0, [0u8; 60]);
    zero_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(21, zero_inode, false);
    let mut old_dir_inode = dir(EXT4_BLOCK_SIZE as u64, old_dir_block);
    old_dir_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(22, old_dir_inode, false);
    let mut old_file_inode = file(
        0o100644,
        old.len() as u64,
        2,
        ext4_extent_tree(&[(0, 1, old_file_block as u64)]),
    );
    old_file_inode.dtime = EXT4_TIMESTAMP;
    b.set_inode(23, old_file_inode, false);

    b.flush_metadata();

    let mut expected_sparse = sparse_head.clone();
    expected_sparse.extend(std::iter::repeat_n(0u8, 1024));
    expected_sparse.extend_from_slice(&sparse_tail);

    let mut push = |path: &str, data: Vec<u8>, deleted: bool, first_block: u32| {
        files.push(FixtureFile {
            path: path.to_string(),
            sha256: sha256_hex(&data),
            data,
            deleted,
            chain_retained: true,
            first_cluster: first_block,
        });
    };
    push("/hello.txt", hello, false, hello_block);
    push("/docs/report.txt", report, false, report_block);
    push("/deleted.txt", deleted, true, deleted_block);
    push("/holiday photo 2024.jpg", photo, true, photo_block_a);
    push("/docs/deleted_notes.txt", notes, true, notes_block);
    push("/sparse.bin", expected_sparse, true, sparse_block_a);
    push("/unwritten.bin", vec![0u8; 1024], true, unwritten_block);
    push("/indirect.bin", indirect, true, indirect_blocks[0]);
    push("/inode_20", orphan, true, orphan_block);
    push("/zero.bin", Vec::new(), true, 0);
    push("/old dir/old.txt", old, true, old_file_block);

    Ext4Fixture {
        bytes: b.bytes,
        files,
        deleted_name: "deleted.txt".to_string(),
        live_name: "report.txt".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_boot_sector_is_fat32() {
        let f = fat32_image();
        assert_eq!(&f.bytes[3..11], b"MSWIN4.1");
        assert_eq!(&f.bytes[82..90], b"FAT32   ");
        let total = u32::from_le_bytes(f.bytes[32..36].try_into().unwrap());
        assert_eq!(total, FAT32_TOTAL_SECTORS);
        assert!(f.files.iter().any(|x| x.deleted));
        assert!(!f.deleted_name.is_empty());
    }

    #[test]
    fn deleted_file_data_is_present_in_the_image() {
        let f = fat32_image();
        let deleted = f.file("/holiday photo 2024.jpg").unwrap();
        assert!(f
            .bytes
            .windows(deleted.data.len())
            .any(|w| w == deleted.data));
        assert_eq!(deleted.sha256, sha256_hex(&deleted.data));
    }
}
