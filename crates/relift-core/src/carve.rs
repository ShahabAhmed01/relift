//! Validating signature carving.
//!
//! A magic-byte hit alone is worthless: random data contains `MZ` and `%PDF-`
//! regularly. Every hit in this module must survive a structural validator that
//! walks the container, bounds every length, and (where the format provides one)
//! verifies a checksum or end marker. Only then is a candidate emitted, with the
//! validator's notes attached.
//!
//! Deliberately unsupported in this build: ZIP64 archives, RAR5, OLE compound
//! files, FLAC, ISO 9660 and formats that do not carry a reliable length. They
//! are documented as gaps rather than emitted with guessed ends.

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::candidate::{
    Candidate, Confidence, ConfidenceLabel, DataLocation, EntryState, RecoveryMethod,
};
use crate::error::Result;
use crate::fs::{CandidateSink, VolumeRef};
use crate::source::ReadOnlySource;
use crate::util::{u16_be, u16_le, u32_be, u32_le, u64_be, u64_le};

const CHUNK: u64 = 4 * 1024 * 1024;
const OVERLAP: u64 = 16;
const MAX_READ: u64 = 16 * 1024 * 1024;
const MAX_JPEG: u64 = 1024 * 1024 * 1024;
const MAX_PNG: u64 = 1024 * 1024 * 1024;
const MAX_PDF: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ZIP: u64 = 8 * 1024 * 1024 * 1024;
const MAX_GIF: u64 = 1024 * 1024 * 1024;
const MAX_TIFF: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MP4: u64 = 16 * 1024 * 1024 * 1024;
const MAX_RIFF: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MP3: u64 = 1024 * 1024 * 1024;
const MAX_OGG: u64 = 4 * 1024 * 1024 * 1024;
const MAX_GZIP: u64 = 1024 * 1024 * 1024;
const MAX_GZIP_OUT: u64 = 512 * 1024 * 1024;
const MAX_7Z: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CarveOptions {
    pub max_candidates: usize,
    pub min_size: u64,
    pub cancelled: Arc<AtomicBool>,
}

impl Default for CarveOptions {
    fn default() -> Self {
        Self {
            max_candidates: 1_000_000,
            min_size: 512,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CarveOptions {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CarveSummary {
    pub emitted: u64,
    pub signature_hits: u64,
    pub validated: u64,
    pub bytes_scanned: u64,
    pub truncated: bool,
    pub by_format: BTreeMap<String, u64>,
    pub warnings: Vec<String>,
}

impl CarveSummary {
    fn note(&mut self, message: impl Into<String>) {
        if self.warnings.len() < 32 {
            self.warnings.push(message.into());
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Marker {
    format: &'static str,
    ext: &'static str,
    magic: &'static [u8],
    /// Bytes to subtract from the magic position to find the file start.
    adjust: i64,
}

const MARKERS: &[Marker] = &[
    Marker {
        format: "jpg",
        ext: "jpg",
        magic: b"\xFF\xD8\xFF",
        adjust: 0,
    },
    Marker {
        format: "png",
        ext: "png",
        magic: b"\x89PNG\r\n\x1A\n",
        adjust: 0,
    },
    Marker {
        format: "pdf",
        ext: "pdf",
        magic: b"%PDF-",
        adjust: 0,
    },
    Marker {
        format: "zip",
        ext: "zip",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "gif",
        ext: "gif",
        magic: b"GIF87a",
        adjust: 0,
    },
    Marker {
        format: "gif",
        ext: "gif",
        magic: b"GIF89a",
        adjust: 0,
    },
    Marker {
        format: "bmp",
        ext: "bmp",
        magic: b"BM",
        adjust: 0,
    },
    Marker {
        format: "tiff",
        ext: "tiff",
        magic: b"II\x2A\x00",
        adjust: 0,
    },
    Marker {
        format: "tiff",
        ext: "tiff",
        magic: b"MM\x00\x2A",
        adjust: 0,
    },
    Marker {
        format: "mp4",
        ext: "mp4",
        magic: b"ftyp",
        adjust: -4,
    },
    Marker {
        format: "sqlite",
        ext: "sqlite",
        magic: b"SQLite format 3\0",
        adjust: 0,
    },
    Marker {
        format: "wav",
        ext: "wav",
        magic: b"RIFF",
        adjust: 0,
    },
    Marker {
        format: "mp3",
        ext: "mp3",
        magic: b"\xFF\xFB",
        adjust: 0,
    },
    Marker {
        format: "mp3",
        ext: "mp3",
        magic: b"\xFF\xFA",
        adjust: 0,
    },
    Marker {
        format: "mp3",
        ext: "mp3",
        magic: b"\xFF\xF3",
        adjust: 0,
    },
    Marker {
        format: "mp3",
        ext: "mp3",
        magic: b"\xFF\xF2",
        adjust: 0,
    },
    Marker {
        format: "mp3",
        ext: "mp3",
        magic: b"\xFF\xE3",
        adjust: 0,
    },
    Marker {
        format: "ogg",
        ext: "ogg",
        magic: b"OggS",
        adjust: 0,
    },
    Marker {
        format: "gz",
        ext: "gz",
        magic: b"\x1F\x8B\x08",
        adjust: 0,
    },
    Marker {
        format: "7z",
        ext: "7z",
        magic: b"7z\xBC\xAF\x27\x1C",
        adjust: 0,
    },
    Marker {
        format: "rar",
        ext: "rar",
        magic: b"Rar!\x1A\x07\x00",
        adjust: 0,
    },
    Marker {
        format: "exe",
        ext: "exe",
        magic: b"MZ",
        adjust: 0,
    },
    Marker {
        format: "elf",
        ext: "elf",
        magic: b"\x7FELF",
        adjust: 0,
    },
];

struct Validated {
    length: u64,
    score: u8,
    notes: Vec<String>,
}

impl Validated {
    fn new(length: u64, score: u8) -> Self {
        Self {
            length,
            score,
            notes: Vec::new(),
        }
    }

    fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// Scan one volume for validated embedded files.
pub fn scan_volume(
    src: &dyn ReadOnlySource,
    volume: &VolumeRef,
    options: &CarveOptions,
    sink: &mut dyn CandidateSink,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<CarveSummary> {
    let mut summary = CarveSummary::default();
    summary.note(
        "Carving validates container structure and checksums where the format provides them; \
         embedded compressed or encrypted payloads are recovered as bytes, not decoded.",
    );
    summary.note(
        "Not carved in this build: ZIP64, RAR5, OLE compound files, FLAC and ISO 9660. \
         See docs/LIMITATIONS.md.",
    );
    let source_end = src.len();
    let start = volume.offset.min(source_end);
    let end = volume.offset.saturating_add(volume.length).min(source_end);
    if end <= start || end - start < options.min_size {
        return Ok(summary);
    }
    let total = end - start;
    let mut pos = 0u64;
    let mut seen_starts = std::collections::HashSet::new();

    while pos < total {
        if options.is_cancelled() {
            summary.truncated = true;
            summary.note("carving cancelled");
            break;
        }
        let remaining = total - pos;
        let new_len = remaining.min(CHUNK);
        let scan_len = (new_len + OVERLAP).min(remaining);
        let buf = src.read_vec(start + pos, scan_len)?;
        let mut hits: Vec<(usize, usize)> = Vec::new();
        for (mi, marker) in MARKERS.iter().enumerate() {
            for hit in memchr::memmem::find_iter(&buf, marker.magic) {
                if (hit as u64) < new_len {
                    hits.push((hit, mi));
                }
            }
        }
        hits.sort_unstable();
        for (hit, mi) in hits {
            if options.is_cancelled() {
                summary.truncated = true;
                break;
            }
            if summary.emitted as usize >= options.max_candidates {
                summary.truncated = true;
                summary.note("candidate limit reached during carving");
                break;
            }
            let marker = &MARKERS[mi];
            let adjusted = hit as i64 + marker.adjust;
            if adjusted < 0 {
                continue;
            }
            let abs_start = start + pos + adjusted as u64;
            if abs_start < start
                || abs_start
                    .checked_add(marker.magic.len() as u64)
                    .map(|e| e > end)
                    .unwrap_or(true)
            {
                continue;
            }
            if !seen_starts.insert(abs_start) {
                continue;
            }
            summary.signature_hits += 1;
            let Some(validated) = validate(src, abs_start, end, marker) else {
                continue;
            };
            if validated.length < options.min_size.max(16)
                || abs_start
                    .checked_add(validated.length)
                    .map(|e| e > end)
                    .unwrap_or(true)
            {
                continue;
            }
            if validated.length > marker_max(marker.format) {
                continue;
            }
            summary.validated += 1;
            let name = format!("carved_{:012x}.{}", abs_start, marker.ext);
            let mut reasons = vec![
                "byte signature matched".to_string(),
                "container structure and bounds validated".to_string(),
            ];
            reasons.extend(validated.notes.iter().cloned());
            let confidence = Confidence::new(ConfidenceLabel::Verified, validated.score, reasons);
            let mut c = Candidate::new(
                name,
                validated.length,
                marker.format,
                DataLocation::raw(abs_start, validated.length),
                RecoveryMethod::Carved,
                confidence,
                "carved",
            );
            c.state = EntryState::Carved;
            c.complete = true;
            c.volume_offset = volume.offset;
            c.record_id = Some(format!("carve@{abs_start:#x}"));
            c.validation = vec![format!(
                "{} validator accepted {} bytes at {:#x}",
                marker.format, validated.length, abs_start
            )];
            c.notes.extend(validated.notes);
            sink.emit(c)?;
            summary.emitted += 1;
            *summary
                .by_format
                .entry(marker.format.to_string())
                .or_default() += 1;
        }
        pos += new_len;
        summary.bytes_scanned = pos;
        progress(pos, total);
        if summary.truncated {
            break;
        }
    }
    Ok(summary)
}

fn marker_max(format: &str) -> u64 {
    match format {
        "jpg" => MAX_JPEG,
        "png" => MAX_PNG,
        "pdf" => MAX_PDF,
        "zip" => MAX_ZIP,
        "gif" => MAX_GIF,
        "tiff" => MAX_TIFF,
        "mp4" => MAX_MP4,
        "wav" => MAX_RIFF,
        "mp3" => MAX_MP3,
        "ogg" => MAX_OGG,
        "gz" => MAX_GZIP,
        "7z" => MAX_7Z,
        _ => 4 * 1024 * 1024 * 1024,
    }
}

fn read_bounded(src: &dyn ReadOnlySource, off: u64, len: u64, limit: u64) -> Option<Vec<u8>> {
    if len > MAX_READ {
        return None;
    }
    if off.checked_add(len)? > limit {
        return None;
    }
    src.read_vec(off, len).ok()
}

fn find_from(src: &dyn ReadOnlySource, mut from: u64, to: u64, needle: &[u8]) -> Option<u64> {
    if from >= to || needle.is_empty() {
        return None;
    }
    let mut buf = vec![0u8; 1024 * 1024];
    let mut carry = 0usize;
    while from < to {
        let want = ((to - from) as usize).min(buf.len() - carry);
        if want == 0 {
            break;
        }
        let n = src.read_at(from, &mut buf[carry..carry + want]).ok()?;
        if n == 0 {
            break;
        }
        let window_len = carry + n;
        if let Some(i) = memchr::memmem::find(&buf[..window_len], needle) {
            return Some(from - carry as u64 + i as u64);
        }
        carry = needle.len().saturating_sub(1).min(window_len);
        buf.copy_within(window_len - carry.., 0);
        from += n as u64;
    }
    None
}

fn crc_range(src: &dyn ReadOnlySource, mut off: u64, len: u64, limit: u64) -> Option<u32> {
    if off.checked_add(len)? > limit {
        return None;
    }
    let mut hasher = crc32fast::Hasher::new();
    let mut left = len;
    let mut buf = vec![0u8; 1024 * 1024];
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        src.read_exact_at(off, &mut buf[..n]).ok()?;
        hasher.update(&buf[..n]);
        off += n as u64;
        left -= n as u64;
    }
    Some(hasher.finalize())
}

fn validate(
    src: &dyn ReadOnlySource,
    start: u64,
    limit: u64,
    marker: &Marker,
) -> Option<Validated> {
    match marker.format {
        "jpg" => validate_jpeg(src, start, limit),
        "png" => validate_png(src, start, limit),
        "pdf" => validate_pdf(src, start, limit),
        "zip" => validate_zip(src, start, limit),
        "gif" => validate_gif(src, start, limit),
        "bmp" => validate_bmp(src, start, limit),
        "tiff" => validate_tiff(src, start, limit),
        "mp4" => validate_mp4(src, start, limit),
        "sqlite" => validate_sqlite(src, start, limit),
        "wav" => validate_riff(src, start, limit),
        "mp3" => validate_mp3(src, start, limit),
        "ogg" => validate_ogg(src, start, limit),
        "gz" => validate_gzip(src, start, limit),
        "7z" => validate_7z(src, start, limit),
        "rar" => validate_rar4(src, start, limit),
        "exe" => validate_pe(src, start, limit),
        "elf" => validate_elf(src, start, limit),
        _ => None,
    }
}

fn validate_jpeg(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let soi = read_bounded(src, start, 2, limit)?;
    if soi != [0xFF, 0xD8] {
        return None;
    }
    let mut pos = start + 2;
    let mut saw_sof = false;
    let mut segments = 0u32;
    while pos + 2 <= limit {
        if pos - start > MAX_JPEG {
            return None;
        }
        let b = read_bounded(src, pos, 2, limit)?;
        if b[0] != 0xFF {
            return None;
        }
        if b[1] == 0xFF {
            pos += 1;
            continue;
        }
        let marker = b[1];
        match marker {
            0xD8 => return None,
            0xD9 => {
                return if saw_sof {
                    Some(
                        Validated::new(pos + 2 - start, 92)
                            .note("JPEG SOI/segments/SOF/EOI validated"),
                    )
                } else {
                    None
                };
            }
            0x01 | 0xD0..=0xD7 => {
                pos += 2;
                continue;
            }
            _ => {}
        }
        if pos + 4 > limit {
            return None;
        }
        let lenbuf = read_bounded(src, pos + 2, 2, limit)?;
        let len = u16_be(&lenbuf, 0)? as u64;
        if len < 2 || pos + 2 + len > limit {
            return None;
        }
        if is_sof(marker) {
            if len < 8 {
                return None;
            }
            saw_sof = true;
        }
        if marker == 0xDA {
            let mut scan = pos + 2 + len;
            let (mpos, m) = next_jpeg_marker(src, scan, limit)?;
            if m == 0xD9 {
                return if saw_sof {
                    Some(
                        Validated::new(mpos + 2 - start, 92)
                            .note("JPEG scan data and EOI validated"),
                    )
                } else {
                    None
                };
            }
            if mpos <= pos {
                return None;
            }
            pos = mpos;
            continue;
        }
        pos += 2 + len;
        segments += 1;
        if segments > 100_000 {
            return None;
        }
    }
    None
}

fn is_sof(m: u8) -> bool {
    matches!(
        m,
        0xC0 | 0xC1 | 0xC2 | 0xC3 | 0xC5 | 0xC6 | 0xC7 | 0xC9 | 0xCA | 0xCB | 0xCD | 0xCE | 0xCF
    )
}

/// Find the next real JPEG marker after entropy-coded data, skipping stuffed
/// `FF 00` bytes and restart markers.
fn next_jpeg_marker(src: &dyn ReadOnlySource, from: u64, limit: u64) -> Option<(u64, u8)> {
    let mut at = from;
    let mut pending: Option<u64> = None;
    let mut buf = vec![0u8; 1024 * 1024];
    while at < limit {
        let want = ((limit - at) as usize).min(buf.len());
        let n = src.read_at(at, &mut buf[..want]).ok()?;
        if n == 0 {
            return None;
        }
        for (i, b) in buf[..n].iter().enumerate() {
            let b = *b;
            if let Some(mpos) = pending {
                match b {
                    0x00 => pending = None,
                    0xFF => {}
                    0xD0..=0xD7 => pending = None,
                    _ => return Some((mpos, b)),
                }
                continue;
            }
            if b == 0xFF {
                pending = Some(at + i as u64);
            }
        }
        at += n as u64;
    }
    None
}

fn validate_png(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let sig = read_bounded(src, start, 8, limit)?;
    if sig != b"\x89PNG\r\n\x1A\n" {
        return None;
    }
    let mut pos = start + 8;
    let mut saw_ihdr = false;
    let mut saw_idat = false;
    let mut chunks = 0u32;
    while pos + 12 <= limit {
        if pos - start > MAX_PNG {
            return None;
        }
        let head = read_bounded(src, pos, 8, limit)?;
        let len = u32_be(&head, 0)? as u64;
        if len > 512 * 1024 * 1024 {
            return None;
        }
        let typ = &head[4..8];
        let end = pos.checked_add(12 + len)?;
        if end > limit {
            return None;
        }
        let mut crc = crc32fast::Hasher::new();
        crc.update(typ);
        let mut at = pos + 8;
        let mut left = len;
        let mut buf = vec![0u8; 64 * 1024];
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            src.read_exact_at(at, &mut buf[..n]).ok()?;
            crc.update(&buf[..n]);
            at += n as u64;
            left -= n as u64;
        }
        let stored = u32_be(&read_bounded(src, pos + 8 + len, 4, limit)?, 0)?;
        if crc.finalize() != stored {
            return None;
        }
        match typ {
            b"IHDR" => {
                if saw_ihdr || pos != start + 8 || len != 13 {
                    return None;
                }
                let ihdr = read_bounded(src, pos + 8, 13, limit)?;
                let w = u32_be(&ihdr, 0)?;
                let h = u32_be(&ihdr, 4)?;
                if w == 0 || h == 0 || w > 1_000_000 || h > 1_000_000 {
                    return None;
                }
                if ihdr[10] != 0 || ihdr[11] != 0 || ihdr[12] > 1 {
                    return None;
                }
                saw_ihdr = true;
            }
            b"IDAT" => saw_idat = true,
            b"IEND" => {
                if len != 0 {
                    return None;
                }
                return if saw_ihdr && saw_idat {
                    Some(
                        Validated::new(end - start, 96)
                            .note("PNG chunks and CRCs validated through IEND"),
                    )
                } else {
                    None
                };
            }
            _ => {}
        }
        chunks += 1;
        if chunks > 1_000_000 {
            return None;
        }
        pos = end;
    }
    None
}

fn validate_pdf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let head = read_bounded(src, start, 8, limit)?;
    if !head.starts_with(b"%PDF-") {
        return None;
    }
    if !head[5].is_ascii_digit() || head[6] != b'.' || !head[7].is_ascii_digit() {
        return None;
    }
    let mut search = start + 5;
    let mut eof = None;
    for _ in 0..8 {
        let p = find_from(src, search, limit, b"%%EOF")?;
        if p + 5 <= limit {
            eof = Some(p + 5);
            break;
        }
        search = p + 5;
    }
    let mut end = eof?;
    if end < start + 64 {
        return None;
    }
    // Include conventional trailing newline bytes but no arbitrary data.
    while end < limit {
        let b = read_bounded(src, end, 1, limit)?;
        if b[0] == b'\r' || b[0] == b'\n' {
            end += 1;
        } else {
            break;
        }
    }
    if end - start > MAX_PDF {
        return None;
    }
    if find_from(src, start, end, b" obj").is_none() {
        return None;
    }
    let has_cross_reference = find_from(src, start, end, b"xref").is_some()
        || find_from(src, start, end, b"startxref").is_some()
        || find_from(src, start, end, b"trailer").is_some();
    if !has_cross_reference {
        return None;
    }
    let mut v = Validated::new(end - start, 92);
    v = v.note("PDF header, cross-reference data and %%EOF validated");
    Some(v)
}

fn validate_zip(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let mut pos = start;
    let mut local_entries = 0u64;
    let mut encrypted = false;
    loop {
        let sig = read_bounded(src, pos, 4, limit)?;
        match sig.as_slice() {
            b"PK\x03\x04" => {
                if local_entries > 1_000_000 {
                    return None;
                }
                let h = read_bounded(src, pos, 30, limit)?;
                let flags = u16_le(&h, 6)?;
                if flags & 0x0008 != 0 || flags & 0x0040 != 0 {
                    // Data descriptors make the local size unknown; multi-disk
                    // archives cannot be carved as one file.
                    return None;
                }
                if flags & 0x0001 != 0 {
                    encrypted = true;
                }
                let comp = u32_le(&h, 18)? as u64;
                let name_len = u16_le(&h, 26)? as u64;
                let extra_len = u16_le(&h, 28)? as u64;
                if comp == 0xFFFF_FFFF {
                    return None;
                }
                let next = pos
                    .checked_add(30)?
                    .checked_add(name_len)?
                    .checked_add(extra_len)?
                    .checked_add(comp)?;
                if next > limit || next <= pos {
                    return None;
                }
                pos = next;
                local_entries += 1;
            }
            b"PK\x01\x02" => {
                let cd_start = pos;
                let mut cd_entries = 0u64;
                loop {
                    let s = read_bounded(src, pos, 4, limit)?;
                    if s.as_slice() != b"PK\x01\x02" {
                        break;
                    }
                    let h = read_bounded(src, pos, 46, limit)?;
                    let name_len = u16_le(&h, 28)? as u64;
                    let extra_len = u16_le(&h, 30)? as u64;
                    let comment_len = u16_le(&h, 32)? as u64;
                    pos = pos
                        .checked_add(46)?
                        .checked_add(name_len)?
                        .checked_add(extra_len)?
                        .checked_add(comment_len)?;
                    if pos > limit {
                        return None;
                    }
                    cd_entries += 1;
                    if cd_entries > 1_000_000 {
                        return None;
                    }
                }
                let eocd = read_bounded(src, pos, 22, limit)?;
                if &eocd[0..4] != b"PK\x05\x06" {
                    return None;
                }
                let disk = u16_le(&eocd, 4)?;
                let cd_disk = u16_le(&eocd, 6)?;
                let on_disk = u16_le(&eocd, 8)? as u64;
                let total_entries = u16_le(&eocd, 10)? as u64;
                let cd_size = u32_le(&eocd, 12)? as u64;
                let cd_offset = u32_le(&eocd, 16)? as u64;
                let comment_len = u16_le(&eocd, 20)? as u64;
                if disk != 0 || cd_disk != 0 {
                    return None;
                }
                if total_entries != 0xFFFF
                    && (total_entries != local_entries || on_disk != cd_entries)
                {
                    return None;
                }
                if cd_offset != cd_start - start || cd_size != pos - cd_start {
                    return None;
                }
                let end = pos.checked_add(22 + comment_len)?;
                if end > limit || end - start > MAX_ZIP {
                    return None;
                }
                let mut v = Validated::new(end - start, 94);
                v = v.note("ZIP local headers, central directory and EOCD validated");
                if encrypted {
                    v = v.note(
                        "at least one ZIP entry is encrypted; bytes were carved, not decrypted",
                    );
                }
                return Some(v);
            }
            b"PK\x05\x06" => {
                if local_entries != 0 {
                    return None;
                }
                let h = read_bounded(src, pos, 22, limit)?;
                let comment_len = u16_le(&h, 20)? as u64;
                let end = pos + 22 + comment_len;
                if end > limit {
                    return None;
                }
                return Some(Validated::new(end - start, 88).note("empty ZIP archive (EOCD only)"));
            }
            _ => return None,
        }
    }
}

fn skip_subblocks(src: &dyn ReadOnlySource, pos: &mut u64, limit: u64) -> Option<()> {
    let mut guard = 0u32;
    loop {
        if *pos >= limit {
            return None;
        }
        let n = read_bounded(src, *pos, 1, limit)?[0] as u64;
        *pos += 1;
        if n == 0 {
            return Some(());
        }
        *pos = pos.checked_add(n)?;
        if *pos > limit {
            return None;
        }
        guard += 1;
        if guard > 10_000_000 {
            return None;
        }
    }
}

fn validate_gif(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let head = read_bounded(src, start, 13, limit)?;
    if &head[0..6] != b"GIF87a" && &head[0..6] != b"GIF89a" {
        return None;
    }
    let packed = head[10];
    let mut pos = start + 13;
    if packed & 0x80 != 0 {
        let table = 3u64 * (1u64 << ((packed & 0x07) + 1));
        pos = pos.checked_add(table)?;
    }
    let mut images = 0u32;
    while pos < limit {
        if pos - start > MAX_GIF {
            return None;
        }
        let b = read_bounded(src, pos, 1, limit)?[0];
        pos += 1;
        match b {
            0x3B => {
                return if images > 0 {
                    Some(Validated::new(pos - start, 90).note("GIF blocks and trailer validated"))
                } else {
                    None
                };
            }
            0x2C => {
                let desc = read_bounded(src, pos, 9, limit)?;
                let lp = desc[8];
                pos += 9;
                if lp & 0x80 != 0 {
                    let table = 3u64 * (1u64 << ((lp & 0x07) + 1));
                    pos = pos.checked_add(table)?;
                }
                if pos >= limit {
                    return None;
                }
                pos += 1; // LZW minimum code size
                skip_subblocks(src, &mut pos, limit)?;
                images += 1;
            }
            0x21 => {
                if pos >= limit {
                    return None;
                }
                pos += 1; // extension label
                skip_subblocks(src, &mut pos, limit)?;
            }
            0x00 => {}
            _ => return None,
        }
    }
    None
}

fn validate_bmp(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 26, limit)?;
    if &h[0..2] != b"BM" {
        return None;
    }
    let size = u32_le(&h, 2)? as u64;
    if size < 26 || start.checked_add(size)? > limit {
        return None;
    }
    let pixel_offset = u32_le(&h, 10)? as u64;
    let dib = u32_le(&h, 14)? as u64;
    if dib < 12 || pixel_offset < 14 + dib || pixel_offset > size {
        return None;
    }
    Some(Validated::new(size, 90).note("BMP file-size and pixel-offset fields are consistent"))
}

fn validate_tiff(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 8, limit)?;
    let le = match &h[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let magic = if le { u16_le(&h, 2)? } else { u16_be(&h, 2)? };
    if magic != 42 {
        return None;
    }
    let ifd = if le { u32_le(&h, 4)? } else { u32_be(&h, 4)? } as u64;
    let mut pos = start.checked_add(ifd)?;
    let mut max_end = start + 8;
    let mut ifds = 0;
    let mut offsets: Vec<u64> = Vec::new();
    let mut counts: Vec<u64> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    while pos != 0 && ifds < 256 {
        if !visited.insert(pos) {
            break;
        }
        let cnt = u16_at(src, pos, le, limit)? as u64;
        pos += 2;
        let entries_end = pos.checked_add(cnt * 12)?;
        if entries_end + 4 > limit {
            return None;
        }
        max_end = max_end.max(entries_end + 4);
        for i in 0..cnt {
            let epos = pos + i * 12;
            let e = read_bounded(src, epos, 12, limit)?;
            let tag = if le { u16_le(&e, 0)? } else { u16_be(&e, 0)? };
            let typ = if le { u16_le(&e, 2)? } else { u16_be(&e, 2)? };
            let count = if le { u32_le(&e, 4)? } else { u32_be(&e, 4)? } as u64;
            if !matches!(tag, 273 | 279 | 324 | 325) {
                continue;
            }
            if count == 0 || count > 1_000_000 {
                return None;
            }
            let unit = match typ {
                3 => 2u64,
                4 => 4u64,
                16 => 8u64,
                _ => return None,
            };
            let total = count.checked_mul(unit)?;
            let data_off = if total <= 4 {
                epos + 8
            } else {
                let off = if le { u32_le(&e, 8)? } else { u32_be(&e, 8)? } as u64;
                start.checked_add(off)?
            };
            if data_off.checked_add(total)? > limit {
                return None;
            }
            let raw = read_bounded(src, data_off, total, limit)?;
            for k in 0..count as usize {
                let v = match unit {
                    2 => {
                        (if le {
                            u16_le(&raw, k * 2)?
                        } else {
                            u16_be(&raw, k * 2)?
                        }) as u64
                    }
                    4 => {
                        (if le {
                            u32_le(&raw, k * 4)?
                        } else {
                            u32_be(&raw, k * 4)?
                        }) as u64
                    }
                    8 => {
                        if le {
                            u64_le(&raw, k * 8)?
                        } else {
                            u64_be(&raw, k * 8)?
                        }
                    }
                    _ => return None,
                };
                if tag == 273 || tag == 324 {
                    offsets.push(v);
                } else {
                    counts.push(v);
                }
            }
            max_end = max_end.max(data_off + total);
        }
        let next_off = if le {
            u32_le(&read_bounded(src, entries_end, 4, limit)?, 0)? as u64
        } else {
            u32_be(&read_bounded(src, entries_end, 4, limit)?, 0)? as u64
        };
        pos = if next_off == 0 {
            0
        } else {
            start.checked_add(next_off)?
        };
        if pos != 0 && pos + 2 > limit {
            return None;
        }
        ifds += 1;
    }
    if offsets.is_empty() || offsets.len() != counts.len() {
        return None;
    }
    for (o, c) in offsets.iter().zip(counts.iter()) {
        let end = start.checked_add(*o)?.checked_add(*c)?;
        if end > limit {
            return None;
        }
        max_end = max_end.max(end);
    }
    if max_end - start > MAX_TIFF {
        return None;
    }
    Some(
        Validated::new(max_end - start, 88).note("TIFF IFD chain and strip/tile offsets validated"),
    )
}

fn u16_at(src: &dyn ReadOnlySource, off: u64, le: bool, limit: u64) -> Option<u16> {
    let b = read_bounded(src, off, 2, limit)?;
    if le {
        u16_le(&b, 0)
    } else {
        u16_be(&b, 0)
    }
}

fn validate_mp4(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    if start < 4 {
        return None;
    }
    let mut pos = start;
    let mut boxes = 0u32;
    let mut saw_ftyp = false;
    let mut saw_moov = false;
    let mut saw_moof = false;
    let mut saw_mdat = false;
    while pos + 8 <= limit {
        let h = read_bounded(src, pos, 8, limit)?;
        let size32 = u32_be(&h, 0)?;
        let typ = &h[4..8];
        let (size, header) = if size32 == 1 {
            let h2 = read_bounded(src, pos, 16, limit)?;
            (u64_be(&h2, 8)?, 16u64)
        } else if size32 == 0 {
            return None;
        } else {
            (size32 as u64, 8u64)
        };
        if size < header || size > MAX_MP4 {
            return None;
        }
        let end = pos.checked_add(size)?;
        if end > limit {
            return None;
        }
        if boxes == 0 && typ != b"ftyp" {
            return None;
        }
        match typ {
            b"ftyp" => saw_ftyp = true,
            b"moov" => saw_moov = true,
            b"moof" => saw_moof = true,
            b"mdat" => saw_mdat = true,
            _ => {}
        }
        pos = end;
        boxes += 1;
        if boxes > 1_000_000 {
            return None;
        }
    }
    if saw_ftyp && (saw_moov || saw_moof) && saw_mdat && pos > start {
        Some(Validated::new(pos - start, 92).note("MP4 box chain validated from ftyp"))
    } else {
        None
    }
}

fn validate_sqlite(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 100, limit)?;
    if &h[0..16] != b"SQLite format 3\0" {
        return None;
    }
    let raw_page = u16_be(&h, 16)?;
    let page_size = if raw_page == 1 {
        65536u64
    } else {
        raw_page as u64
    };
    if page_size < 512 || page_size > 65536 || !page_size.is_power_of_two() {
        return None;
    }
    let write_version = h[18];
    let read_version = h[19];
    if !(1..=2).contains(&write_version) || !(1..=2).contains(&read_version) {
        return None;
    }
    if h[20] != 0 || h[21] != 64 || h[22] != 32 || h[23] != 32 {
        return None;
    }
    let page_count = u32_be(&h, 28)? as u64;
    if page_count == 0 {
        return None;
    }
    let size = page_size.checked_mul(page_count)?;
    if size < 512 || start.checked_add(size)? > limit {
        return None;
    }
    Some(Validated::new(size, 96).note("SQLite header, page size and page count validated"))
}

fn validate_riff(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 12, limit)?;
    if &h[0..4] != b"RIFF" {
        return None;
    }
    let size = u32_le(&h, 4)? as u64;
    let kind = &h[8..12];
    let end = start.checked_add(size)?.checked_add(8)?;
    if size < 4 || end > limit || end - start > MAX_RIFF {
        return None;
    }
    let mut pos = start + 12;
    let mut saw_fmt = false;
    let mut saw_data = false;
    let mut saw_codec = false;
    let mut chunks = 0u32;
    while pos + 8 <= end {
        let ch = read_bounded(src, pos, 8, end)?;
        let id = &ch[0..4];
        let csize = u32_le(&ch, 4)? as u64;
        let next = pos.checked_add(8)?.checked_add(csize)?;
        if next > end {
            return None;
        }
        match id {
            b"fmt " => saw_fmt = true,
            b"data" => saw_data = true,
            b"VP8 " | b"VP8L" | b"VP8X" => saw_codec = true,
            b"LIST" => {
                if csize >= 4 {
                    let list_type = read_bounded(src, pos + 8, 4, end)?;
                    if &list_type == b"hdrl" {
                        saw_fmt = true;
                    }
                }
            }
            _ => {}
        }
        pos = next + (csize & 1);
        if pos > end {
            return None;
        }
        chunks += 1;
        if chunks > 1_000_000 {
            return None;
        }
    }
    let ok = match kind {
        b"WAVE" => saw_fmt && saw_data,
        b"AVI " => saw_fmt,
        b"WEBP" => saw_codec,
        _ => false,
    };
    if ok {
        let label = String::from_utf8_lossy(kind).to_string();
        Some(Validated::new(end - start, 90).note(format!("RIFF/{label} chunks validated")))
    } else {
        None
    }
}

fn validate_mp3(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let head = read_bounded(src, start, 10.min(limit - start), limit)?;
    let mut pos = start;
    if head.len() >= 10 && &head[0..3] == b"ID3" {
        if head[3] > 4 || head[4] == 0xFF {
            return None;
        }
        let sz = syncsafe(&head[6..10])? as u64;
        if sz > 256 * 1024 * 1024 {
            return None;
        }
        pos = start + 10 + sz;
        if head[5] & 0x10 != 0 {
            pos += 10;
        }
        if pos >= limit {
            return None;
        }
    }
    let mut frames = 0u32;
    let mut end = pos;
    while end + 4 <= limit {
        if end - start > MAX_MP3 {
            return None;
        }
        let four = read_bounded(src, end, 4, limit)?;
        if &four[0..3] == b"TAG" && end + 128 <= limit {
            end += 128;
            break;
        }
        let Some(len) = mp3_frame_len(&four) else {
            break;
        };
        if len < 24 || end.checked_add(len)? > limit {
            break;
        }
        end += len;
        frames += 1;
        if frames > 10_000_000 {
            return None;
        }
    }
    if frames >= 3 && end - start >= 1024 {
        Some(Validated::new(end - start, 88).note(format!(
            "{frames} consecutive MPEG Layer III frames validated"
        )))
    } else {
        None
    }
}

fn syncsafe(b: &[u8]) -> Option<u32> {
    if b.len() < 4 || b.iter().any(|v| v & 0x80 != 0) {
        return None;
    }
    Some(((b[0] as u32) << 21) | ((b[1] as u32) << 14) | ((b[2] as u32) << 7) | b[3] as u32)
}

fn mp3_frame_len(h: &[u8]) -> Option<u64> {
    if h.len() < 4 || h[0] != 0xFF || h[1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = (h[1] >> 3) & 0x03;
    let layer = (h[1] >> 1) & 0x03;
    if version == 1 || layer == 0 {
        return None;
    }
    let bitrate_idx = ((h[2] >> 4) & 0x0F) as usize;
    let sample_idx = ((h[2] >> 2) & 0x03) as usize;
    let padding = ((h[2] >> 1) & 0x01) as u64;
    if bitrate_idx == 0 || bitrate_idx == 15 || sample_idx == 3 {
        return None;
    }
    const MPEG1_L1: [u32; 15] = [
        0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
    ];
    const MPEG1_L2: [u32; 15] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
    ];
    const MPEG1_L3: [u32; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const MPEG2_L1: [u32; 15] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
    ];
    const MPEG2_L23: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let bitrate = if version == 3 {
        match layer {
            3 => MPEG1_L1[bitrate_idx],
            2 => MPEG1_L2[bitrate_idx],
            _ => MPEG1_L3[bitrate_idx],
        }
    } else {
        match layer {
            3 => MPEG2_L1[bitrate_idx],
            _ => MPEG2_L23[bitrate_idx],
        }
    } as u64;
    let sample = if version == 3 {
        [44100u64, 48000, 32000][sample_idx]
    } else if version == 2 {
        [22050u64, 24000, 16000][sample_idx]
    } else {
        [11025u64, 12000, 8000][sample_idx]
    };
    let base = match layer {
        3 => (12 * bitrate * 1000 / sample) * 4,
        2 => 144 * bitrate * 1000 / sample,
        _ => {
            let factor = if version == 3 { 144 } else { 72 };
            factor * bitrate * 1000 / sample
        }
    };
    Some(base + padding)
}

fn validate_ogg(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let mut pos = start;
    let mut serial: Option<u32> = None;
    let mut seq: Option<u32> = None;
    let mut first = true;
    let mut pages = 0u32;
    while pos + 27 <= limit {
        if pos - start > MAX_OGG {
            return None;
        }
        let head = read_bounded(src, pos, 27, limit)?;
        if &head[0..4] != b"OggS" || head[4] != 0 {
            return None;
        }
        let htype = head[5];
        let page_serial = u32_le(&head, 14)?;
        let page_seq = u32_le(&head, 18)?;
        let stored_crc = u32_le(&head, 22)?;
        let nsegs = head[26] as usize;
        let seg = read_bounded(src, pos + 27, nsegs as u64, limit)?;
        let body: u64 = seg.iter().map(|v| *v as u64).sum();
        let page_len = 27u64 + nsegs as u64 + body;
        let end = pos.checked_add(page_len)?;
        if end > limit {
            return None;
        }
        let mut page = read_bounded(src, pos, page_len, limit)?;
        page[22..26].fill(0);
        if ogg_crc(&page) != stored_crc {
            return None;
        }
        if first {
            if htype & 0x02 == 0 {
                return None;
            }
            serial = Some(page_serial);
            first = false;
        } else {
            if serial != Some(page_serial) {
                return None;
            }
            if let Some(prev) = seq {
                if page_seq != prev.wrapping_add(1) {
                    return None;
                }
            }
        }
        seq = Some(page_seq);
        pos = end;
        pages += 1;
        if pages > 1_000_000 {
            return None;
        }
        if htype & 0x04 != 0 {
            return Some(
                Validated::new(pos - start, 94)
                    .note(format!("{pages} Ogg pages validated with CRC and EOS")),
            );
        }
    }
    None
}

fn ogg_crc(data: &[u8]) -> u32 {
    const POLY: u32 = 0x04C1_1DB7;
    let mut crc = 0u32;
    for b in data {
        crc ^= (*b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ POLY
            } else {
                crc << 1
            };
        }
    }
    crc
}

struct SliceReader<'a> {
    src: &'a dyn ReadOnlySource,
    pos: u64,
    end: u64,
}

impl Read for SliceReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.end || buf.is_empty() {
            return Ok(0);
        }
        let want = buf.len().min((self.end - self.pos) as usize);
        let n = self
            .src
            .read_at(self.pos, &mut buf[..want])
            .map_err(io::Error::other)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "gzip stream ended at source boundary",
            ));
        }
        self.pos += n as u64;
        Ok(n)
    }
}

fn validate_gzip(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let mut pos = start;
    let head = read_bounded(src, pos, 10, limit)?;
    if head[0] != 0x1F || head[1] != 0x8B || head[2] != 8 {
        return None;
    }
    let flags = head[3];
    if flags & 0xE0 != 0 {
        return None;
    }
    pos += 10;
    if flags & 0x04 != 0 {
        let xlen = u16_le(&read_bounded(src, pos, 2, limit)?, 0)? as u64;
        pos = pos.checked_add(2 + xlen)?;
    }
    if flags & 0x08 != 0 {
        loop {
            if pos >= limit {
                return None;
            }
            let b = read_bounded(src, pos, 1, limit)?[0];
            pos += 1;
            if b == 0 {
                break;
            }
            if pos - start > 64 * 1024 {
                return None;
            }
        }
    }
    if flags & 0x10 != 0 {
        loop {
            if pos >= limit {
                return None;
            }
            let b = read_bounded(src, pos, 1, limit)?[0];
            pos += 1;
            if b == 0 {
                break;
            }
            if pos - start > 128 * 1024 {
                return None;
            }
        }
    }
    if flags & 0x02 != 0 {
        pos = pos.checked_add(2)?;
    }
    let data_start = pos;
    if data_start >= limit || data_start - start > MAX_GZIP {
        return None;
    }
    let stream_end = limit.min(start + MAX_GZIP);
    let mut decoder = flate2::read::DeflateDecoder::new(SliceReader {
        src,
        pos: data_start,
        end: stream_end,
    });
    let mut crc = crc32fast::Hasher::new();
    let mut total = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match decoder.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                if total > MAX_GZIP_OUT {
                    return None;
                }
                crc.update(&buf[..n]);
            }
            Err(_) => return None,
        }
    }
    let consumed = decoder.total_in();
    let trailer_off = data_start.checked_add(consumed)?;
    let trailer = read_bounded(src, trailer_off, 8, limit)?;
    let stored_crc = u32_le(&trailer, 0)?;
    let stored_size = u32_le(&trailer, 4)?;
    if crc.finalize() != stored_crc || (total as u32) != stored_size {
        return None;
    }
    let end = trailer_off + 8;
    Some(Validated::new(end - start, 95).note("gzip deflate stream, CRC32 and ISIZE validated"))
}

fn validate_7z(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 32, limit)?;
    if &h[0..6] != b"7z\xBC\xAF\x27\x1C" {
        return None;
    }
    let start_crc = u32_le(&h, 8)?;
    if crc32fast::hash(&h[12..32]) != start_crc {
        return None;
    }
    let next_off = u64_le(&h, 12)?;
    let next_size = u64_le(&h, 20)?;
    let next_crc = u32_le(&h, 28)?;
    let next_start = start.checked_add(32)?.checked_add(next_off)?;
    let end = next_start.checked_add(next_size)?;
    if end > limit || end - start > MAX_7Z {
        return None;
    }
    if next_size > 256 * 1024 * 1024 {
        return None;
    }
    if next_size > 0 {
        if crc_range(src, next_start, next_size, limit)? != next_crc {
            return None;
        }
    }
    Some(Validated::new(end - start, 96).note("7z start-header CRC and next-header CRC validated"))
}

fn validate_rar4(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let sig = read_bounded(src, start, 7, limit)?;
    if &sig != b"Rar!\x1A\x07\x00" {
        return None;
    }
    let mut pos = start + 7;
    let mut blocks = 0u32;
    let mut saw_main = false;
    while pos + 7 <= limit {
        if pos - start > 4 * 1024 * 1024 * 1024 {
            return None;
        }
        let h = read_bounded(src, pos, 7, limit)?;
        let typ = h[2];
        let flags = u16_le(&h, 3)?;
        let size = u16_le(&h, 5)? as u64;
        if size < 7 || pos.checked_add(size)? > limit {
            return None;
        }
        let mut total = size;
        if flags & 0x8000 != 0 {
            let add = read_bounded(src, pos + 7, 4, limit)?;
            total += u32_le(&add, 0)? as u64;
        }
        if typ == 0x73 {
            saw_main = true;
        }
        if typ == 0x7B {
            let end = pos.checked_add(total)?;
            return if saw_main {
                Some(
                    Validated::new(end - start, 86)
                        .note("RAR4 blocks validated through the end block"),
                )
            } else {
                None
            };
        }
        let next = pos.checked_add(total)?;
        if next <= pos {
            return None;
        }
        pos = next;
        blocks += 1;
        if blocks > 1_000_000 {
            return None;
        }
    }
    None
}

fn validate_pe(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let dos = read_bounded(src, start, 64, limit)?;
    if &dos[0..2] != b"MZ" {
        return None;
    }
    let lfanew = u32_le(&dos, 0x3C)? as u64;
    if lfanew < 64 || lfanew > 0x1000 {
        return None;
    }
    let pe_off = start.checked_add(lfanew)?;
    let pe = read_bounded(src, pe_off, 24, limit)?;
    if &pe[0..4] != b"PE\0\0" {
        return None;
    }
    let nsections = u16_le(&pe, 6)? as u64;
    let opt_size = u16_le(&pe, 20)? as u64;
    if nsections == 0 || nsections > 96 || opt_size > 0x10000 {
        return None;
    }
    let opt = read_bounded(src, pe_off + 24, opt_size.min(2), limit)?;
    if opt.len() < 2 || (opt[0] != 0x0B && opt[0] != 0x07) {
        return None;
    }
    let table = pe_off + 24 + opt_size;
    let table_bytes = nsections.checked_mul(40)?;
    if table.checked_add(table_bytes)? > limit {
        return None;
    }
    let mut end = table + table_bytes;
    for i in 0..nsections {
        let sec = read_bounded(src, table + i * 40, 40, limit)?;
        let raw_size = u32_le(&sec, 16)? as u64;
        let raw_ptr = u32_le(&sec, 20)? as u64;
        if raw_size == 0 {
            continue;
        }
        let sec_end = start.checked_add(raw_ptr)?.checked_add(raw_size)?;
        if sec_end > limit {
            return None;
        }
        end = end.max(sec_end);
    }
    Some(Validated::new(end - start, 90).note("PE DOS/COFF headers and section extents validated"))
}

fn validate_elf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let ident = read_bounded(src, start, 64, limit)?;
    if &ident[0..4] != b"\x7FELF" {
        return None;
    }
    let class = ident[4];
    let data = ident[5];
    if !(class == 1 || class == 2) || !(data == 1 || data == 2) {
        return None;
    }
    let le = data == 1;
    let is64 = class == 2;
    let read16 = |b: &[u8], o: usize| if le { u16_le(b, o) } else { u16_be(b, o) };
    let read32 = |b: &[u8], o: usize| if le { u32_le(b, o) } else { u32_be(b, o) };
    let read64 = |b: &[u8], o: usize| if le { u64_le(b, o) } else { u64_be(b, o) };
    let (phoff, shoff, phentsize, phnum, shentsize, shnum, hdr_size) = if is64 {
        (
            read64(&ident, 32)?,
            read64(&ident, 40)?,
            read16(&ident, 54)? as u64,
            read16(&ident, 56)? as u64,
            read16(&ident, 58)? as u64,
            read16(&ident, 60)? as u64,
            64u64,
        )
    } else {
        (
            read32(&ident, 28)? as u64,
            read32(&ident, 32)? as u64,
            read16(&ident, 42)? as u64,
            read16(&ident, 44)? as u64,
            read16(&ident, 46)? as u64,
            read16(&ident, 48)? as u64,
            52u64,
        )
    };
    if phnum > 4096 || shnum > 65535 {
        return None;
    }
    let mut end = start + hdr_size;
    if phnum > 0 {
        if phentsize < if is64 { 56 } else { 32 } {
            return None;
        }
        let bytes = phnum.checked_mul(phentsize)?;
        if start.checked_add(phoff)?.checked_add(bytes)? > limit {
            return None;
        }
        for i in 0..phnum {
            let off = start + phoff + i * phentsize;
            let e = read_bounded(src, off, phentsize, limit)?;
            let (poff, pfilesz) = if is64 {
                (read64(&e, 8)?, read64(&e, 32)?)
            } else {
                (read32(&e, 4)? as u64, read32(&e, 16)? as u64)
            };
            if pfilesz == 0 {
                continue;
            }
            let e2 = start.checked_add(poff)?.checked_add(pfilesz)?;
            if e2 > limit {
                return None;
            }
            end = end.max(e2);
        }
    }
    if shnum > 0 {
        if shentsize < if is64 { 64 } else { 40 } {
            return None;
        }
        let bytes = shnum.checked_mul(shentsize)?;
        if start.checked_add(shoff)?.checked_add(bytes)? > limit {
            return None;
        }
        for i in 0..shnum {
            let off = start + shoff + i * shentsize;
            let e = read_bounded(src, off, shentsize, limit)?;
            let (soff, ssize) = if is64 {
                (read64(&e, 24)?, read64(&e, 32)?)
            } else {
                (read32(&e, 16)? as u64, read32(&e, 20)? as u64)
            };
            if ssize == 0 {
                continue;
            }
            let e2 = start.checked_add(soff)?.checked_add(ssize)?;
            if e2 > limit {
                return None;
            }
            end = end.max(e2);
        }
    }
    if end == start + hdr_size && phnum == 0 && shnum == 0 {
        return None;
    }
    Some(Validated::new(end - start, 90).note("ELF program/section header extents validated"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::VecSink;
    use crate::source::FileSource;
    use std::io::Write;

    #[test]
    fn ogg_crc_reference_vector() {
        // CRC of the ASCII bytes "OggS" with the Ogg polynomial
        // (0x04C11DB7, MSB-first, init 0, no final xor).
        assert_eq!(ogg_crc(b"OggS"), 0x5FB0_A94F);
    }

    #[test]
    fn random_mz_is_rejected() {
        let dir = std::env::temp_dir().join(format!("relift-carve-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("noise.img");
        let mut f = std::fs::File::create(&path).unwrap();
        let mut data = vec![0u8; 8192];
        data[100] = b'M';
        data[101] = b'Z';
        f.write_all(&data).unwrap();
        drop(f);
        let src = FileSource::open(&path).unwrap();
        let volume = VolumeRef::new(0, src.len());
        let mut sink = VecSink::default();
        let mut progress = |_a: u64, _b: u64| {};
        let summary = scan_volume(
            &src,
            &volume,
            &CarveOptions::default(),
            &mut sink,
            &mut progress,
        )
        .unwrap();
        assert_eq!(summary.emitted, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_sqlite_is_carved() {
        let dir = std::env::temp_dir().join(format!("relift-carve2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("db.img");
        let page = 4096usize;
        let mut data = vec![0u8; page * 2 + 1024];
        data[..16].copy_from_slice(b"SQLite format 3\0");
        data[16..18].copy_from_slice(&(page as u16).to_be_bytes());
        data[18] = 1;
        data[19] = 1;
        data[21] = 64;
        data[22] = 32;
        data[23] = 32;
        data[28..32].copy_from_slice(&2u32.to_be_bytes());
        // Add a random embedded valid header later in the image.
        data[page * 2 + 100..page * 2 + 116].copy_from_slice(b"SQLite format 3\0");
        data[page * 2 + 116..page * 2 + 118].copy_from_slice(&(page as u16).to_be_bytes());
        data[page * 2 + 118] = 1;
        data[page * 2 + 119] = 1;
        data[page * 2 + 121] = 64;
        data[page * 2 + 122] = 32;
        data[page * 2 + 123] = 32;
        data[page * 2 + 128..page * 2 + 132].copy_from_slice(&1u32.to_be_bytes());
        std::fs::write(&path, &data).unwrap();
        let src = FileSource::open(&path).unwrap();
        let volume = VolumeRef::new(0, src.len());
        let mut sink = VecSink::default();
        let mut progress = |_a: u64, _b: u64| {};
        let summary = scan_volume(
            &src,
            &volume,
            &CarveOptions {
                min_size: 16,
                ..Default::default()
            },
            &mut sink,
            &mut progress,
        )
        .unwrap();
        assert!(summary.emitted >= 1);
        assert!(sink.items.iter().any(|c| c.format == "sqlite"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
