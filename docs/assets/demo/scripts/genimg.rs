//! Deterministic demo-image generator for the Relift demo.
//! Writes a raw FAT32 "disk image" byte-by-byte (superfloppy, boot sector at
//! offset 0) populated with originally-authored files, then "deletes" about 60
//! percent of them the way Windows does. Never calls Relift code.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const SECTOR: u64 = 512;
const SPC: u64 = 1; // sectors per cluster -> 512 B clusters (matches relift-testkit FAT32 fixture)
const RESERVED: u64 = 32;
const FATS: u64 = 2;
const TOTAL_SECTORS: u64 = 128 * 1024 * 1024 / SECTOR; // 128 MiB image
const IMAGE_BYTES: u64 = TOTAL_SECTORS * SECTOR;
const VOLUME_LABEL: &[u8; 11] = b"RELIFT DEMO";
const VOLUME_ID: u32 = 0x1122_3344;
const DOS_DATE: u16 = ((2026 - 1980) << 9) | (11 << 5) | 27;
const DOS_TIME: u16 = (10 << 11) | (30 << 5);

#[derive(Clone, Debug)]
struct DemoFile {
    path: String,
    name83: [u8; 11],
    dir_cluster: u32,
    data: Vec<u8>,
    deleted: bool,
    chain_retained: bool,
    clusters: Vec<u32>,
}

fn main() {
    let dir_order = ["photos", "docs", "data", "archive", "audio", "scans", "notes"];
    let mut dc: BTreeMap<&str, u32> = BTreeMap::new();
    let mut next = 3u32; // root dir = cluster 2; subdirs take 3..=9
    for d in &dir_order {
        dc.insert(d, next);
        next += 1;
    }

    // Build files with allocated clusters.
    let mut files: Vec<DemoFile> = Vec::new();
    let mut next = 4u32; // root directory occupies clusters 2 and 3
    for (path, name83, dir, data, deleted, chain_retained) in content() {
        let ncl = (data.len().div_ceil(512) as u32).max(1); // SPC=1 -> 512 B clusters
        let clusters: Vec<u32> = (next..next + ncl).collect();
        next += ncl;
        files.push(DemoFile {
            path,
            name83,
            dir_cluster: 2,
            data,
            deleted,
            chain_retained,
            clusters,
        });
    }

    let mut img = Fat32Image::new();
    // Root directory chain: cluster 2 -> cluster 3 (end of chain).
    img.fat[2] = 3;
    img.fat[3] = 0x0FFF_FFF8;
    // Link FAT chains for files whose data must remain readable:
    //   - all live (non-deleted) files
    //   - deleted files whose chain is retained
    // Deleted, chain-freed files keep FAT == 0 (simulating Windows freeing).
    for f in &files {
        if f.deleted && !f.chain_retained {
            continue;
        }
        for i in 0..f.clusters.len() {
            let nxt = if i + 1 < f.clusters.len() {
                f.clusters[i + 1]
            } else {
                0x0FFF_FFF8u32
            };
            img.fat[f.clusters[i] as usize] = nxt;
        }
    }
    for f in &files {
        img.write_clusters(f.clusters.as_slice(), &f.data);
    }

    // Root directory (cluster 2): every file lives here. Deleted entries get
    // their first byte set to 0xE5 (Windows-style deletion); live entries keep
    // their name intact.
    let mut root_entries: Vec<([u8; 11], u32, u32, u64, bool)> = vec![
        (dot_name(2), 2, 0x10, 0, true),
        (dotdot_name(2), 2, 0x10, 0, true),
    ];
    for f in files.iter() {
        let mut n83 = f.name83;
        if f.deleted {
            n83[0] = 0xE5;
        }
        root_entries.push((n83, f.clusters[0], 0x20, f.data.len() as u64, false));
    }
    // Root directory spans clusters 2 and 3 (544 bytes of entries > one 512 B
    // cluster). Serialize entries and write them across the dir cluster chain.
    let dir_clusters: [u32; 2] = [2, 3];
    let mut dir_bytes: Vec<u8> = Vec::new();
    for (name, first, attr, size, _is_dir) in &root_entries {
        let mut e = [0u8; 32];
        e[0..11].copy_from_slice(name);
        e[11] = *attr as u8;
        w16(&mut e[14..16], DOS_TIME);
        w16(&mut e[16..18], DOS_DATE);
        w16(&mut e[18..20], DOS_DATE);
        w16(&mut e[20..22], (*first >> 16) as u16);
        w16(&mut e[22..24], DOS_TIME);
        w16(&mut e[24..26], DOS_DATE);
        w16(&mut e[26..28], (*first & 0xFFFF) as u16);
        w32(&mut e[28..32], *size as u32);
        dir_bytes.extend_from_slice(&e);
    }
    let mut at = 0usize;
    for c in &dir_clusters {
        let off = img.cluster_offset(*c) as usize;
        let n = (dir_bytes.len() - at).min(img.cluster_bytes as usize);
        img.bytes[off..off + n].copy_from_slice(&dir_bytes[at..at + n]);
        at += n;
        if at >= dir_bytes.len() { break; }
    }

    // Free FAT chains for deleted, chain-freed files (chain-retained keep links).
    for f in &files {
        if f.deleted && !f.chain_retained {
            for c in &f.clusters {
                img.fat[*c as usize] = 0;
            }
        }
    }
    img.flush_fat();
    img.write_boot();

    // Self-check: re-read each file's bytes from the image using the same
    // cluster layout Relift would use, and compare to the in-memory data.
    let mut self_ok = 0u64;
    let mut self_bad = 0u64;
    for f in &files {
        let mut got: Vec<u8> = Vec::new();
        for c in &f.clusters {
            let off = img.cluster_offset(*c) as usize;
            let n = img.cluster_bytes as usize;
            got.extend_from_slice(&img.bytes[off..off + n]);
        }
        got.truncate(f.data.len());
        if got == f.data { self_ok += 1; } else { self_bad += 1; }
        if got != f.data {
            let show = f.data.len().min(16).min(got.len());
            eprintln!("genimg SELF-CHECK MISMATCH: {} disk[0..{}]= {:?} mem[0..{}]= {:?} off={}", f.path, show, &got[..show], show, &f.data[..show], img.cluster_offset(f.clusters[0]));
        }
    }
    println!("genimg: self-check disk==in-memory: {self_ok} ok, {self_bad} bad");
    if self_bad > 0 { eprintln!("genimg: WARNING: {} self-check mismatches", self_bad); }

    // Manifest of ORIGINAL files (computed before deletion).
    let mut manifest_lines = Vec::new();
    let mut expected_lines = Vec::new();
    for f in &files {
        let digest = Sha256::digest(&f.data);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        manifest_lines.push(format!("{hex}  {}", f.path));
        expected_lines.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            f.path,
            hex,
            f.deleted,
            f.chain_retained,
            f.data.len(),
            f.clusters.first().copied().unwrap_or(0),
        ));
    }
    let manifest = manifest_lines.join("\n") + "\n";

    let out_dir = Path::new("C:/relift-demo/demo");
    fs::create_dir_all(out_dir).expect("create demo dir");
    let image_path = out_dir.join("evidence.img");
    fs::write(&image_path, &img.bytes).expect("write image");
    fs::write(out_dir.join("manifest.sha256"), manifest.as_bytes()).expect("write manifest");
    fs::write(out_dir.join("expected.tsv"), expected_lines.join("\n") + "\n").expect("write expected");

    let img_hash = sha256_hex(&img.bytes);
    let deleted = files.iter().filter(|f| f.deleted).count();
    let retained = files.iter().filter(|f| f.deleted && f.chain_retained).count();
    let freed = files.iter().filter(|f| f.deleted && !f.chain_retained).count();
    let live = files.iter().filter(|f| !f.deleted).count();
    println!("genimg: wrote {}", image_path.display());
    println!("genimg: image sha256  {img_hash}  evidence.img");
    println!("genimg: {} files total: {} deleted ({} retained, {} freed), {} live controls",
        files.len(), deleted, retained, freed, live);
    println!("genimg: manifest -> {}", out_dir.join("manifest.sha256").display());
    println!("genimg: expected    -> {}", out_dir.join("expected.tsv").display());
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------

struct Fat32Image {
    bytes: Vec<u8>,
    fat: Vec<u32>,
    fat_sectors: u64,
    data_start: u64,
    cluster_bytes: u64,
}

impl Fat32Image {
    fn new() -> Self {
        let fat_sectors = compute_fat_sectors();
        let clusters = (TOTAL_SECTORS - RESERVED - FATS * fat_sectors) / SPC;
        let data_start = (RESERVED + FATS * fat_sectors) * SECTOR;
        let mut bytes = vec![0u8; IMAGE_BYTES as usize];
        let mut fat = vec![0u32; (clusters + 2) as usize];
        fat[0] = 0x0FFF_FFF8;
        fat[1] = 0x0FFF_FFFF;
        fat[2] = 0x0FFF_FFFF;
        Self { bytes, fat, fat_sectors, data_start, cluster_bytes: SPC * SECTOR }
    }

    fn cluster_offset(&self, cluster: u32) -> u64 {
        self.data_start + (cluster as u64 - 2) * self.cluster_bytes
    }

    fn write_clusters(&mut self, clusters: &[u32], data: &[u8]) {
        let mut at = 0usize;
        for c in clusters {
            let off = self.cluster_offset(*c) as usize;
            let n = (data.len() - at).min(self.cluster_bytes as usize);
            self.bytes[off..off + n].copy_from_slice(&data[at..at + n]);
            at += n;
            if at >= data.len() { break; }
        }
    }

    fn flush_fat(&mut self) {
        for fat_index in 0..FATS {
            let off = (RESERVED + fat_index * self.fat_sectors) * SECTOR;
            for (i, v) in self.fat.iter().enumerate() {
                let at = off as usize + i * 4;
                if at + 4 <= self.bytes.len() {
                    self.bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
    }

    fn write_boot(&mut self) {
        let mut bs = [0u8; 512];
        bs[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
        bs[3..11].copy_from_slice(b"MSWIN4.1");
        w16(&mut bs[11..13], SECTOR as u16);
        bs[13] = SPC as u8;
        w16(&mut bs[14..16], RESERVED as u16);
        bs[16] = FATS as u8;
        bs[21] = 0xF8;
        w32(&mut bs[28..32], 0); // hidden sectors
        w32(&mut bs[32..36], TOTAL_SECTORS as u32);
        w32(&mut bs[36..40], self.fat_sectors as u32);
        w32(&mut bs[44..48], 2); // root directory cluster
        w16(&mut bs[50..52], 6); // backup boot sector
        bs[58] = 0x80;
        bs[61] = 0x29;
        w32(&mut bs[66..70], VOLUME_ID);
        bs[71..82].copy_from_slice(VOLUME_LABEL);
        bs[82..90].copy_from_slice(b"FAT32   ");
        bs[510] = 0x55;
        bs[511] = 0xAA;
        let start = 0usize;
        let end = SECTOR as usize;
        self.bytes[start..end].copy_from_slice(&bs);
        self.bytes[6 * SECTOR as usize..7 * SECTOR as usize].copy_from_slice(&bs);
    }

    fn write_dir_cluster(
        &mut self,
        cluster: u32,
        entries: &[([u8; 11], u32, u32, u64, bool)], // name, first_cluster, attr, size, is_dir
    ) {
        let off = self.cluster_offset(cluster) as usize;
        let mut pos = off;
        for (name, first, attr, size, _is_dir) in entries {
            let mut e = [0u8; 32];
            e[0..11].copy_from_slice(name);
            e[11] = *attr as u8;
            w16(&mut e[14..16], DOS_TIME);
            w16(&mut e[16..18], DOS_DATE);
            w16(&mut e[18..20], DOS_DATE);
            w16(&mut e[20..22], (*first >> 16) as u16);
            w16(&mut e[22..24], DOS_TIME);
            w16(&mut e[24..26], DOS_DATE);
            w16(&mut e[26..28], (*first & 0xFFFF) as u16);
            w32(&mut e[28..32], *size as u32);
            self.bytes[pos..pos + 32].copy_from_slice(&e);
            pos += 32;
        }
    }
}

fn compute_fat_sectors() -> u64 {
    let mut fat = 1u64;
    for _ in 0..10 {
        let clusters = (TOTAL_SECTORS - RESERVED - FATS * fat) / SPC;
        let need = ((clusters + 2) * 4 + SECTOR - 1) / SECTOR;
        if need == fat { return fat; }
        fat = need;
    }
    panic!("fat sectors did not converge");
}

fn dot_name(cluster: u32) -> [u8; 11] { let mut n = [b' '; 11]; n[0] = b'.'; n }
fn dotdot_name(_parent: u32) -> [u8; 11] { let mut n = [b' '; 11]; n[0..2].copy_from_slice(b".."); n }
fn w16(dst: &mut [u8], v: u16) { dst.copy_from_slice(&v.to_le_bytes()); }
fn w32(dst: &mut [u8], v: u32) { dst.copy_from_slice(&v.to_le_bytes()); }
fn name83(name: &str) -> [u8; 11] {
    let mut n = [b' '; 11];
    let (base, ext) = match name.rsplit_once('.') { Some((b, e)) => (b, e), None => (name, "") };
    let b = base.as_bytes(); let nbase = b.len().min(8);
    n[0..nbase].copy_from_slice(&b[0..nbase]);
    let e = ext.as_bytes(); let ne = e.len().min(3);
    n[8..8 + ne].copy_from_slice(&e[0..ne]);
    n
}

// ---------------------------------------------------------------------------
// Content: all originally authored in this session by script.

fn pattern(len: usize, seed: u32) -> Vec<u8> {
    (0..len).map(|i| ((i as u32).wrapping_mul(37).wrapping_add(seed) % 251) as u8).collect()
}

fn draw_png(seed: u32, w: u32, h: u32) -> Vec<u8> {
    // Tiny PNG encoder: IHDR + IDAT (stored raw, no compression) + IEND.
    let mut raw: Vec<u8> = Vec::with_capacity((w as usize * h as usize * 3 + h as usize) + 8);
    for y in 0..h {
        raw.push(0); // filter type none
        for x in 0..w {
            let r = ((x as u32).wrapping_mul(seed).wrapping_add(y) % 256) as u8;
            let g = ((y as u32).wrapping_mul(seed).wrapping_add(x) % 256) as u8;
            let b = (((x + y) as u32).wrapping_mul(seed) % 256) as u8;
            raw.push(r); raw.push(g); raw.push(b);
        }
    }
    let ihdr: Vec<u8> = {
        let mut b = Vec::new();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.push(8); b.push(2); b.push(0); b.push(0); b.push(0);
        b
    };
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    png.extend_from_slice(&chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&chunk(b"IDAT", &raw));
    png.extend_from_slice(&chunk(b"IEND", &[]));
    png
}

fn chunk(ty: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    let mut crc = 0u32;
    for t in ty { crc = crc32_update(crc, *t); }
    for d in data { crc = crc32_update(crc, *d); }
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

fn crc32_update(mut crc: u32, b: u8) -> u32 {
    crc ^= b as u32;
    for _ in 0..8 {
        crc = if crc & 1 == 1 { 0xEDB88320 ^ (crc >> 1) } else { crc >> 1 };
    }
    crc
}

fn write_pdf(seed: u32, approx_len: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let title = format!("Relift demo original document seed={seed}\n");
    let body = pattern(approx_len.saturating_sub(title.len()), seed);
    out.extend_from_slice(b"%PDF-1.4\n");
    out.extend_from_slice(title.as_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(b"\n%%EOF\n");
    out
}

fn write_docx(seed: u32, approx_len: usize) -> Vec<u8> {
    // Minimal valid ZIP container with [Content_Types].xml + document.xml
    let mut zip = Vec::new();
    let types = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
</Types>";
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:body><w:p><w:r><w:t xml:space=\"preserve\">Relift demo original DOCX (seed {seed}): budget Q3 figures are preliminary.</w:t></w:r></w:p></w:body></w:document>"
    );
    let rels = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>";
    add_zip_entry(&mut zip, "[Content_Types].xml", types);
    add_zip_entry(&mut zip, "_rels/.rels", rels);
    add_zip_entry(&mut zip, "word/document.xml", doc.as_bytes());
    let pad = pattern(approx_len.saturating_sub(zip.len()), seed);
    add_zip_entry(&mut zip, "word/_rels/dummy.bin", &pad);
    finish_zip(&mut zip);
    zip
}

fn write_xlsx(seed: u32, approx_len: usize) -> Vec<u8> {
    let mut zip = Vec::new();
    let types = b"<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\
<Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\
</Types>";
    let wb = b"<?xml version=\"1.0\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>";
    let sheet = format!(
        "<?xml version=\"1.0\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
<sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Relift demo original XLSX seed {seed}</t></is></c></row>\
<row r=\"2\"><c r=\"A2\" t=\"n\"><v>42</v></c></row><row r=\"3\"><c r=\"A3\" t=\"n\"><v>7</v></c></row></sheetData></worksheet>"
    );
    add_zip_entry(&mut zip, "[Content_Types].xml", types);
    add_zip_entry(&mut zip, "xl/workbook.xml", wb);
    add_zip_entry(&mut zip, "xl/worksheets/sheet1.xml", sheet.as_bytes());
    let pad = pattern(approx_len.saturating_sub(zip.len()), seed);
    add_zip_entry(&mut zip, "xl/_rels/dummy.bin", &pad);
    finish_zip(&mut zip);
    zip
}

fn write_zip(seed: u32, files: Vec<(&'static str, &'static [u8])>, approx_len: usize) -> Vec<u8> {
    let count = files.len();
    let mut zip = Vec::new();
    let mut cd: Vec<u8> = Vec::new();
    for (name, data) in &files {
        add_zip_entry(&mut zip, name, data);
        let crc = crc32_bytes(data);
        let len = data.len() as u32;
        let name_len = name.len() as u16;
        cd.extend_from_slice(b"PK\x01\x02");
        cd.extend_from_slice(&20u16.to_le_bytes()); // version made by
        cd.extend_from_slice(&20u16.to_le_bytes()); // version needed
        cd.extend_from_slice(&0u16.to_le_bytes()); // flags
        cd.extend_from_slice(&8u16.to_le_bytes()); // method = deflate (stored here but fine for carving)
        cd.extend_from_slice(&0u16.to_le_bytes()); // time
        cd.extend_from_slice(&0u16.to_le_bytes()); // date
        cd.extend_from_slice(&crc.to_le_bytes());
        cd.extend_from_slice(&len.to_le_bytes());
        cd.extend_from_slice(&len.to_le_bytes());
        cd.extend_from_slice(&name_len.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes()); // extra
        cd.extend_from_slice(&0u16.to_le_bytes()); // comment
        cd.extend_from_slice(&0u16.to_le_bytes()); // disk start
        cd.extend_from_slice(&0u16.to_le_bytes()); // int attr
        cd.extend_from_slice(&0u16.to_le_bytes()); // ext attr
        cd.extend_from_slice(&0u32.to_le_bytes()); // relative offset of local header (not needed for carving)
        cd.extend_from_slice(name.as_bytes());
    }
    let eocd_len = cd.len() as u32;
    zip.extend_from_slice(b"PK\x05\x06");
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&count.to_le_bytes());
    zip.extend_from_slice(&count.to_le_bytes());
    zip.extend_from_slice(&eocd_len.to_le_bytes());
    zip.extend_from_slice(&(zip.len() as u32).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    let pad = pattern(approx_len.saturating_sub(zip.len()), seed);
    zip.extend_from_slice(&pad);
    zip
}

fn crc32_bytes(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &b in data { crc = crc32_update(crc, b); }
    crc
}

fn add_zip_entry(zip: &mut Vec<u8>, name: &str, data: &[u8]) {
    let crc = crc32_bytes(data);
    let name_len = name.len() as u16;
    zip.extend_from_slice(b"PK\x03\x04");
    zip.extend_from_slice(&20u16.to_le_bytes()); // version needed
    zip.extend_from_slice(&0u16.to_le_bytes()); // flags
    zip.extend_from_slice(&0u16.to_le_bytes()); // method = store
    zip.extend_from_slice(&0u16.to_le_bytes()); // time
    zip.extend_from_slice(&0u16.to_le_bytes()); // date
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(data.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(data.len() as u32).to_le_bytes());
    zip.extend_from_slice(&name_len.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes()); // extra
    zip.extend_from_slice(name.as_bytes());
    zip.extend_from_slice(data);
}

fn finish_zip(_zip: &mut Vec<u8>) {
    // EOCD written by callers inline for docx/xlsx too; kept minimal.
}

fn write_wav_tone(seed: u32, rate: u32, approx_len: usize) -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::new();
    let mut n = 0usize;
    while samples.len() * 2 < approx_len {
        let t = n as f32;
        let s = (2.0 * std::f32::consts::PI * 440.0 * t / rate as f32).sin();
        samples.push((s * 12000.0 * seed.wrapping_rem(9) as f32 / 9.0) as i16);
        n += 1;
    }
    riff_wav(rate, &samples)
}

fn write_wav_sweep(seed: u32, rate: u32, approx_len: usize) -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::new();
    let mut n = 0usize;
    while samples.len() * 2 < approx_len {
        let t = n as f32;
        let f = 220.0 + 880.0 * (t / rate as f32);
        let phase = 2.0 * std::f32::consts::PI * f * t / rate as f32;
        samples.push((phase.sin() * 8000.0) as i16);
        n += 1;
    }
    riff_wav(rate, &samples)
}

fn riff_wav(rate: u32, samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let fmt = [
        1u16, 1, rate as u16, (rate * 2) as u16, 2u16, 16u16,
    ];
    let mut fmt_bytes = Vec::new();
    for v in fmt { fmt_bytes.extend_from_slice(&v.to_le_bytes()); }
    let mut riff = Vec::new();
    riff.extend_from_slice(b"RIFF");
    riff.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    riff.extend_from_slice(b"WAVE");
    riff.extend_from_slice(b"fmt ");
    riff.extend_from_slice(&(16u32).to_le_bytes());
    riff.extend_from_slice(&fmt_bytes);
    riff.extend_from_slice(b"data");
    riff.extend_from_slice(&(data.len() as u32).to_le_bytes());
    riff.extend_from_slice(&data);
    riff
}

fn write_csv(seed: u32, approx_len: usize) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("date,station,reading,flag\n");
    let mut n = 0usize;
    while out.len() < approx_len {
        out.push_str(&format!("2026-11-{:02},ST-{:03},{:.4},{}\n",
            (n % 28) + 1, n % 50, (n as f32 * 0.371) % 100.0, if n % 7 == 0 { "REVIEW" } else { "OK" }));
        n += 1;
    }
    out.into_bytes()
}

fn write_journal(seed: u32, approx_len: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"Relift demo journal (synthetic, authored for the demo)\n\n");
    let body = pattern(approx_len.saturating_sub(out.len()), seed);
    out.extend_from_slice(&body);
    out.extend_from_slice(b"\n-- end --\n");
    out
}

fn write_readme(seed: u32, approx_len: usize) -> Vec<u8> {
    let head = b"Relift demo README\n===================\nThis image holds synthetic demo files authored by script.\n\n";
    let mut out: Vec<u8> = head.to_vec();
    out.extend_from_slice(&pattern(approx_len.saturating_sub(head.len()), seed));
    out
}

fn write_sqlite(seed: u32, approx_len: usize) -> Vec<u8> {
    // Valid enough for Relift's SQLite carver: header "SQLite format 3\0",
    // page size 4096, page count > 0, versions 1/1.
    let page_size: u16 = 4096;
    let mut page = vec![0u8; page_size as usize];
    page[0..16].copy_from_slice(b"SQLite format 3\0");
    page[16..18].copy_from_slice(&page_size.to_be_bytes());
    page[18] = 1; page[19] = 1; // write/read version
    page[20] = 0; page[21] = 64; page[22] = 32; page[23] = 32;
    let page_count = (approx_len as u32 + page_size as u32 - 1) / page_size as u32;
    page[28..32].copy_from_slice(&page_count.to_be_bytes());
    let mut out = Vec::new();
    for _ in 0..page_count.max(1) {
        out.extend_from_slice(&page);
    }
    // pad to approx_len
    let pad = pattern(approx_len.saturating_sub(out.len()), seed);
    out.extend_from_slice(&pad);
    out
}

fn content() -> Vec<(String, [u8; 11], &'static str, Vec<u8>, bool, bool)> {
    // (path, 8.3 name, dir, data, deleted, chain_retained)
    vec![
        ("photos/sunset.png".to_string(), name83("sunset.png"), "photos", draw_png(1, 450, 400), true, false),
        ("photos/portrait.png".to_string(), name83("portrait.png"), "photos", draw_png(2, 380, 380), true, true),
        ("docs/q3-report.pdf".to_string(), name83("q3-report.pdf"), "docs", write_pdf(3, 90 * 1024), true, false),
        ("docs/budget.docx".to_string(), name83("budget.docx"), "docs", write_docx(4, 75 * 1024), true, true),
        ("data/timeseries.xlsx".to_string(), name83("timeseries.xlsx"), "data", write_xlsx(5, 110 * 1024), true, false),
        ("data/readings.db".to_string(), name83("readings.db"), "data", write_sqlite(6, 300 * 1024), true, false),
        ("archive/backup.zip".to_string(), name83("backup.zip"), "archive", write_zip(7, vec![("budget Q3.txt", b"Relift demo original: quarterly budget backup set A.\n"), ("notes 2026.txt", b"Original archived notes.\n")], 520 * 1024), true, true),
        ("archive/old.zip".to_string(), name83("old.zip"), "archive", write_zip(8, vec![("old draft.txt", b"Relift demo original: legacy draft archive.\n")], 240 * 1024), true, false),
        ("audio/tone.wav".to_string(), name83("tone.wav"), "audio", write_wav_tone(9, 44100, 1_300_000), true, false),
        ("audio/sweep.wav".to_string(), name83("sweep.wav"), "audio", write_wav_sweep(10, 44100, 920 * 1024), true, true),
        ("scans/scan1.png".to_string(), name83("scan1.png"), "scans", draw_png(11, 1100, 850), true, false),
        ("scans/draft.pdf".to_string(), name83("draft.pdf"), "scans", write_pdf(12, 130 * 1024), false, false),
        ("notes/2026-11.csv".to_string(), name83("2026-11.csv"), "notes", write_csv(13, 150 * 1024), false, false),
        ("notes/journal.txt".to_string(), name83("journal.txt"), "notes", write_journal(14, 65 * 1024), false, false),
        ("notes/readme.txt".to_string(), name83("readme.txt"), "notes", write_readme(15, 40 * 1024), false, false),
    ]
}

