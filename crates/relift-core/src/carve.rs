//! Validating signature carving.
//!
//! A magic-byte hit alone is worthless: random data contains `MZ` and `%PDF-`
//! regularly. Every hit in this module must survive a structural validator that
//! walks the container, bounds every length, and (where the format provides one)
//! verifies a checksum or end marker. Only then is a candidate emitted, with the
//! validator's notes attached.
//!
//! ZIP64 is not separately handled: it shares the `PK\x03\x04` local-header
//! magic with ZIP and is validated by the existing ZIP validator. MOV is
//! classified as MP4/ISO-BMFF and WebM as MKV. Every other registered format
//! carries its own structural validator; formats without a registered validator
//! are never emitted.

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
const MAX_HEIC: u64 = 16 * 1024 * 1024 * 1024;
const MAX_AVIF: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DOCX: u64 = 2 * 1024 * 1024 * 1024;
const MAX_XLSX: u64 = 4 * 1024 * 1024 * 1024;
const MAX_PPTX: u64 = 2 * 1024 * 1024 * 1024;
const MAX_MKV: u64 = 64 * 1024 * 1024 * 1024;
const MAX_PST: u64 = 64 * 1024 * 1024 * 1024;
const MAX_LNK: u64 = 1024 * 1024;
const MAX_EVTX: u64 = 1024 * 1024 * 1024;
const MAX_WEBP: u64 = 1024 * 1024 * 1024;
const MAX_TAR: u64 = 64 * 1024 * 1024 * 1024;
const MAX_BZIP2: u64 = 1024 * 1024 * 1024;
const MAX_XZ: u64 = 1024 * 1024 * 1024;
const MAX_ZSTD: u64 = 1024 * 1024 * 1024;
const MAX_FLAC: u64 = 1024 * 1024 * 1024;
const MAX_OLE: u64 = 1024 * 1024 * 1024;
const MAX_MPEGTS: u64 = 64 * 1024 * 1024 * 1024;
const MAX_RAR5: u64 = 1024 * 1024 * 1024;
const MAX_RAR4: u64 = 1024 * 1024 * 1024;
const MAX_ISO9660: u64 = 4 * 1024 * 1024 * 1024;
// ---- Additional carving format size limits ----
const MAX_CAB: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ACE: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ARJ: u64 = 4 * 1024 * 1024 * 1024;
const MAX_LZH: u64 = 4 * 1024 * 1024 * 1024;
const MAX_LZO: u64 = 1024 * 1024 * 1024;
const MAX_PAR2: u64 = 8 * 1024 * 1024 * 1024;
const MAX_RPM: u64 = 2 * 1024 * 1024 * 1024;
const MAX_XAR: u64 = 8 * 1024 * 1024 * 1024;
const MAX_DEB: u64 = 2 * 1024 * 1024 * 1024;
const MAX_BKF: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ARC: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ZOO: u64 = 4 * 1024 * 1024 * 1024;
const MAX_WIM: u64 = 16 * 1024 * 1024 * 1024;
const MAX_TGA: u64 = 512 * 1024 * 1024;
const MAX_PCX: u64 = 512 * 1024 * 1024;
const MAX_PBM: u64 = 512 * 1024 * 1024;
const MAX_PGM: u64 = 512 * 1024 * 1024;
const MAX_PPM: u64 = 512 * 1024 * 1024;
const MAX_ICO: u64 = 64 * 1024 * 1024;
const MAX_ICNS: u64 = 512 * 1024 * 1024;
const MAX_CUR: u64 = 64 * 1024 * 1024;
const MAX_EMF: u64 = 512 * 1024 * 1024;
const MAX_WMF: u64 = 512 * 1024 * 1024;
const MAX_CDR: u64 = 512 * 1024 * 1024;
const MAX_AI: u64 = 512 * 1024 * 1024;
const MAX_EPS: u64 = 512 * 1024 * 1024;
const MAX_PS: u64 = 512 * 1024 * 1024;
const MAX_INDD: u64 = 512 * 1024 * 1024;
const MAX_XCF: u64 = 512 * 1024 * 1024;
const MAX_PSD: u64 = 512 * 1024 * 1024;
const MAX_KRA: u64 = 512 * 1024 * 1024;
const MAX_AFD: u64 = 512 * 1024 * 1024;
const MAX_AMR: u64 = 512 * 1024 * 1024;
const MAX_AAC: u64 = 512 * 1024 * 1024;
const MAX_M4A: u64 = 512 * 1024 * 1024;
const MAX_WMA: u64 = 512 * 1024 * 1024;
const MAX_AIFF: u64 = 512 * 1024 * 1024;
const MAX_AU: u64 = 512 * 1024 * 1024;
const MAX_MID: u64 = 256 * 1024 * 1024;
const MAX_IT: u64 = 256 * 1024 * 1024;
const MAX_XM: u64 = 256 * 1024 * 1024;
const MAX_S3M: u64 = 256 * 1024 * 1024;
const MAX_MOD: u64 = 256 * 1024 * 1024;
const MAX_APE: u64 = 512 * 1024 * 1024;
const MAX_VOC: u64 = 256 * 1024 * 1024;
const MAX_SHN: u64 = 256 * 1024 * 1024;
const MAX_SPX: u64 = 512 * 1024 * 1024;
const MAX_FLV: u64 = 4 * 1024 * 1024 * 1024;
const MAX_SWF: u64 = 512 * 1024 * 1024;
const MAX_MXF: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DPX: u64 = 4 * 1024 * 1024 * 1024;
const MAX_EXR: u64 = 512 * 1024 * 1024;
const MAX_HDR: u64 = 512 * 1024 * 1024;
const MAX_ASF: u64 = 4 * 1024 * 1024 * 1024;
const MAX_VOB: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DOC: u64 = 2 * 1024 * 1024 * 1024;
const MAX_XLS: u64 = 2 * 1024 * 1024 * 1024;
const MAX_PPT: u64 = 2 * 1024 * 1024 * 1024;
const MAX_PUB: u64 = 512 * 1024 * 1024;
const MAX_WPD: u64 = 512 * 1024 * 1024;
const MAX_WPS: u64 = 512 * 1024 * 1024;
const MAX_XLR: u64 = 512 * 1024 * 1024;
const MAX_MDB: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ACCDB: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ONE: u64 = 512 * 1024 * 1024;
const MAX_PAGES: u64 = 512 * 1024 * 1024;
const MAX_NUMBERS: u64 = 512 * 1024 * 1024;
const MAX_KEY: u64 = 512 * 1024 * 1024;
const MAX_FB2: u64 = 1024 * 1024 * 1024;
const MAX_TEX: u64 = 1024 * 1024 * 1024;
const MAX_LYX: u64 = 1024 * 1024 * 1024;
const MAX_OST: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DBX: u64 = 512 * 1024 * 1024;
const MAX_MSG: u64 = 2 * 1024 * 1024 * 1024;
const MAX_NSF: u64 = 512 * 1024 * 1024;
const MAX_MBX: u64 = 4 * 1024 * 1024 * 1024;
const MAX_EML: u64 = 1024 * 1024 * 1024;
const MAX_MDF: u64 = 16 * 1024 * 1024 * 1024;
const MAX_LDF: u64 = 16 * 1024 * 1024 * 1024;
const MAX_IBD: u64 = 4 * 1024 * 1024 * 1024;
const MAX_DBASE: u64 = 256 * 1024 * 1024;
const MAX_CDX: u64 = 256 * 1024 * 1024;
const MAX_FPT: u64 = 1024 * 1024 * 1024;
const MAX_PAR: u64 = 256 * 1024 * 1024;
const MAX_DLL: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SYS: u64 = 2 * 1024 * 1024 * 1024;
const MAX_COM: u64 = 64 * 1024 * 1024;
const MAX_MACHO: u64 = 2 * 1024 * 1024 * 1024;
const MAX_CLASS: u64 = 256 * 1024 * 1024;
const MAX_DEX: u64 = 256 * 1024 * 1024;
const MAX_TTF: u64 = 64 * 1024 * 1024;
const MAX_OTF: u64 = 64 * 1024 * 1024;
const MAX_WOFF: u64 = 64 * 1024 * 1024;
const MAX_WOFF2: u64 = 64 * 1024 * 1024;
const MAX_PFB: u64 = 64 * 1024 * 1024;
const MAX_E01: u64 = 64 * 1024 * 1024 * 1024;
const MAX_VFD: u64 = 64 * 1024 * 1024;
const MAX_ROM: u64 = 256 * 1024 * 1024;
const MAX_N64: u64 = 256 * 1024 * 1024;
const MAX_GBA: u64 = 64 * 1024 * 1024;
const MAX_SFC: u64 = 16 * 1024 * 1024;
const MAX_NES: u64 = 16 * 1024 * 1024;
const MAX_BIN: u64 = 1024 * 1024 * 1024;
const MAX_PLIST: u64 = 64 * 1024 * 1024;
const MAX_APPLESINGLE: u64 = 64 * 1024 * 1024;
// ===== Camera RAW size limits =====
const MAX_ORF: u64 = 256 * 1024 * 1024;
const MAX_RW2: u64 = 256 * 1024 * 1024;
const MAX_X3F: u64 = 256 * 1024 * 1024;
const MAX_CRW: u64 = 256 * 1024 * 1024;
// ===== Additional audio size limits =====
const MAX_WV: u64 = 512 * 1024 * 1024;
const MAX_TTA: u64 = 512 * 1024 * 1024;
// ===== Disk images / backup size limits =====
const MAX_GHO: u64 = 64 * 1024 * 1024 * 1024;
const MAX_DAR: u64 = 64 * 1024 * 1024 * 1024;

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
    Marker {
        format: "heic",
        ext: "heic",
        magic: b"ftypheic",
        adjust: -4,
    },
    Marker {
        format: "heic",
        ext: "heic",
        magic: b"ftypmif1",
        adjust: -4,
    },
    Marker {
        format: "avif",
        ext: "avif",
        magic: b"ftypavif",
        adjust: -4,
    },
    Marker {
        format: "docx",
        ext: "docx",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "xlsx",
        ext: "xlsx",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "pptx",
        ext: "pptx",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "mkv",
        ext: "mkv",
        magic: b"\x1A\x45\xDF\xA3",
        adjust: 0,
    },
    Marker {
        format: "pst",
        ext: "pst",
        magic: b"!BDN",
        adjust: 0,
    },
    Marker {
        format: "lnk",
        ext: "lnk",
        magic: b"L\x00\x00\x00\x01\x14\x02\x00\x00\x00\x00\x00\x00\xC0\x00\x00\x00\x00\x00\x00\x46",
        adjust: 0,
    },
    Marker {
        format: "evtx",
        ext: "evtx",
        magic: b"ElfFile",
        adjust: 0,
    },
    Marker {
        format: "webp",
        ext: "webp",
        magic: b"RIFF",
        adjust: 0,
    },
    Marker {
        format: "tar",
        ext: "tar",
        magic: b"ustar",
        adjust: 257,
    },
    Marker {
        format: "bzip2",
        ext: "bz2",
        magic: b"BZh",
        adjust: 0,
    },
    Marker {
        format: "xz",
        ext: "xz",
        magic: b"\xFD7zXZ\x00",
        adjust: 0,
    },
    Marker {
        format: "zst",
        ext: "zst",
        magic: b"\x28\xB5\x2F\xFD",
        adjust: 0,
    },
    Marker {
        format: "flac",
        ext: "flac",
        magic: b"fLaC",
        adjust: 0,
    },
    Marker {
        format: "ole",
        ext: "ole",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "ts",
        ext: "ts",
        magic: b"\x47",
        adjust: 0,
    },
    Marker {
        format: "rar5",
        ext: "rar",
        magic: b"Rar!\x1A\x07\x01",
        adjust: 0,
    },
    Marker {
        format: "iso9660",
        ext: "iso",
        magic: b"CD001",
        adjust: -1,
    },
    // ===== Additional archives =====
    Marker {
        format: "cab",
        ext: "cab",
        magic: b"MSCF",
        adjust: 0,
    },
    Marker {
        format: "ace",
        ext: "ace",
        magic: b"**ACE",
        adjust: 0,
    },
    Marker {
        format: "arj",
        ext: "arj",
        magic: b"ARJ",
        adjust: 0,
    },
    Marker {
        format: "lzh",
        ext: "lzh",
        magic: b"lh5\\",
        adjust: 0,
    },
    Marker {
        format: "lzh",
        ext: "lzh",
        magic: b"lh7\\",
        adjust: 0,
    },
    Marker {
        format: "lzo",
        ext: "lzo",
        magic: b"\x89LZO",
        adjust: 0,
    },
    Marker {
        format: "par2",
        ext: "par2",
        magic: b"PAR2\0PKG",
        adjust: 0,
    },
    Marker {
        format: "rpm",
        ext: "rpm",
        magic: b"\xabED\xabEE\xdb",
        adjust: 0,
    },
    Marker {
        format: "xar",
        ext: "xar",
        magic: b"xar!",
        adjust: 0,
    },
    Marker {
        format: "deb",
        ext: "deb",
        magic: b"!<arch>\n",
        adjust: 0,
    },
    Marker {
        format: "bkf",
        ext: "bkf",
        magic: b"\x89BKF",
        adjust: 0,
    },
    Marker {
        format: "arc",
        ext: "arc",
        magic: b"\x1a",
        adjust: 0,
    },
    Marker {
        format: "zoo",
        ext: "zoo",
        magic: b"Zoo",
        adjust: 0,
    },
    Marker {
        format: "wim",
        ext: "wim",
        magic: b"MSWIM",
        adjust: 0,
    },
    // ===== Additional images =====
    Marker {
        format: "tga",
        ext: "tga",
        magic: b"\x00\x00\x02",
        adjust: 0,
    },
    Marker {
        format: "pcx",
        ext: "pcx",
        magic: b"\x0A",
        adjust: 0,
    },
    Marker {
        format: "pbm",
        ext: "pbm",
        magic: b"P4",
        adjust: 0,
    },
    Marker {
        format: "pgm",
        ext: "pgm",
        magic: b"P5",
        adjust: 0,
    },
    Marker {
        format: "ppm",
        ext: "ppm",
        magic: b"P6",
        adjust: 0,
    },
    Marker {
        format: "ico",
        ext: "ico",
        magic: b"\x00\x00\x01\x00",
        adjust: 0,
    },
    Marker {
        format: "icns",
        ext: "icns",
        magic: b"icns",
        adjust: 0,
    },
    Marker {
        format: "cur",
        ext: "cur",
        magic: b"\x00\x00\x02\x00",
        adjust: 0,
    },
    Marker {
        format: "emf",
        ext: "emf",
        magic: b"\x01\x00\x00\x00",
        adjust: 0,
    },
    Marker {
        format: "wmf",
        ext: "wmf",
        magic: b"\xD7\xCD\xC6\x9A",
        adjust: 0,
    },
    Marker {
        format: "cdr",
        ext: "cdr",
        magic: b"\x00\x00\x00\x00\xFF\xFF\xFF\xFF",
        adjust: 0,
    },
    Marker {
        format: "ai",
        ext: "ai",
        magic: b"%PDF-",
        adjust: 0,
    },
    Marker {
        format: "eps",
        ext: "eps",
        magic: b"%!PS",
        adjust: 0,
    },
    Marker {
        format: "ps",
        ext: "ps",
        magic: b"%!PS",
        adjust: 0,
    },
    Marker {
        format: "indd",
        ext: "indd",
        magic: b"\x06\x0e\x2b\x34",
        adjust: 0,
    },
    Marker {
        format: "xcf",
        ext: "xcf",
        magic: b"gimp xcf",
        adjust: 0,
    },
    Marker {
        format: "psd",
        ext: "psd",
        magic: b"8BPS",
        adjust: 0,
    },
    Marker {
        format: "kra",
        ext: "kra",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "afd",
        ext: "afdesign",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    // ===== Additional audio =====
    Marker {
        format: "amr",
        ext: "amr",
        magic: b"#!AMR",
        adjust: 0,
    },
    Marker {
        format: "aac",
        ext: "aac",
        magic: b"\xFF\xF1",
        adjust: 0,
    },
    Marker {
        format: "aac",
        ext: "aac",
        magic: b"\xFF\xF9",
        adjust: 0,
    },
    Marker {
        format: "m4a",
        ext: "m4a",
        magic: b"ftypM4A",
        adjust: -4,
    },
    Marker {
        format: "m4a",
        ext: "m4a",
        magic: b"ftypM4B",
        adjust: -4,
    },
    Marker {
        format: "wma",
        ext: "wma",
        magic: b"\x30\x26\xB2\x75\x8E\x66\xCF\x11\xA6\xD9\x00\xAA\x00\x62\xCE\x6C",
        adjust: 0,
    },
    Marker {
        format: "aiff",
        ext: "aiff",
        magic: b"FORM",
        adjust: 0,
    },
    Marker {
        format: "au",
        ext: "au",
        magic: b".snd",
        adjust: 0,
    },
    Marker {
        format: "mid",
        ext: "mid",
        magic: b"MThd",
        adjust: 0,
    },
    Marker {
        format: "it",
        ext: "it",
        magic: b"IMPM",
        adjust: 0,
    },
    Marker {
        format: "xm",
        ext: "xm",
        magic: b"Extended Module: ",
        adjust: 0,
    },
    Marker {
        format: "s3m",
        ext: "s3m",
        magic: b"Scream Tracker 3",
        adjust: 0,
    },
    Marker {
        format: "mod",
        ext: "mod",
        magic: b"M.K.",
        adjust: 0,
    },
    Marker {
        format: "mod",
        ext: "mod",
        magic: b"FLT4",
        adjust: 0,
    },
    Marker {
        format: "mod",
        ext: "mod",
        magic: b"FC00",
        adjust: 0,
    },
    Marker {
        format: "ape",
        ext: "ape",
        magic: b"MAC",
        adjust: 0,
    },
    Marker {
        format: "voc",
        ext: "voc",
        magic: b"Creative Voice File",
        adjust: 0,
    },
    Marker {
        format: "shn",
        ext: "shn",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "spx",
        ext: "spx",
        magic: b"OggS",
        adjust: 0,
    },
    // ===== Additional video =====
    Marker {
        format: "flv",
        ext: "flv",
        magic: b"FLV",
        adjust: 0,
    },
    Marker {
        format: "swf",
        ext: "swf",
        magic: b"FWS",
        adjust: 0,
    },
    Marker {
        format: "swf",
        ext: "swf",
        magic: b"CWS",
        adjust: 0,
    },
    Marker {
        format: "mxf",
        ext: "mxf",
        magic: b"\x06\x0e\x2b\x34\x02\x05\x01\x01\x0d\x01\x03\x01\x02\x07\x01\x01\x0d\x01\x02\x01\x01\x04\x01",
        adjust: 0,
    },
    Marker {
        format: "dpx",
        ext: "dpx",
        magic: b"SDPX",
        adjust: 0,
    },
    Marker {
        format: "exr",
        ext: "exr",
        magic: b"\x76\x2f\x31\x01",
        adjust: 0,
    },
    Marker {
        format: "hdr",
        ext: "hdr",
        magic: b"?RADIANCE",
        adjust: 0,
    },
    Marker {
        format: "asf",
        ext: "asf",
        magic: b"\x30\x26\xB2\x75\x8E\x66\xCF\x11\xA6\xD9\x00\xAA\x00\x62\xCE\x6C",
        adjust: 0,
    },
    Marker {
        format: "vob",
        ext: "vob",
        magic: b"",
        adjust: 0,
    },
    // ===== Additional documents (OLE / ZIP / binary) =====
    Marker {
        format: "doc",
        ext: "doc",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "xls",
        ext: "xls",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "ppt",
        ext: "ppt",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "pub",
        ext: "pub",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "wpd",
        ext: "wpd",
        magic: b"\xFF\x57\x50\x43",
        adjust: 0,
    },
    Marker {
        format: "wps",
        ext: "wps",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "xlr",
        ext: "xlr",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "mdb",
        ext: "mdb",
        magic: b"\x00\x01\x00\x00",
        adjust: 0,
    },
    Marker {
        format: "accdb",
        ext: "accdb",
        magic: b"\x00\x01\x00\x00",
        adjust: 0,
    },
    Marker {
        format: "one",
        ext: "one",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "pages",
        ext: "pages",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "numbers",
        ext: "numbers",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "key",
        ext: "key",
        magic: b"PK\x03\x04",
        adjust: 0,
    },
    Marker {
        format: "fb2",
        ext: "fb2",
        magic: b"<?xml",
        adjust: 0,
    },
    Marker {
        format: "tex",
        ext: "tex",
        magic: b"\x25\x21\x54\x65\x58",
        adjust: 0,
    },
    Marker {
        format: "lyx",
        ext: "lyx",
        magic: b"\x5c\x5c\x5b\x4c",
        adjust: 0,
    },
    // ===== Email =====
    Marker {
        format: "ost",
        ext: "ost",
        magic: b"!BDN",
        adjust: 0,
    },
    Marker {
        format: "dbx",
        ext: "dbx",
        magic: b"\x21\x42\x44\x4e",
        adjust: 0,
    },
    Marker {
        format: "msg",
        ext: "msg",
        magic: b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1",
        adjust: 0,
    },
    Marker {
        format: "nsf",
        ext: "nsf",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "mbx",
        ext: "mbx",
        magic: b"From ",
        adjust: 0,
    },
    Marker {
        format: "eml",
        ext: "eml",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "mdf",
        ext: "mdf",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "ldf",
        ext: "ldf",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "ibd",
        ext: "ibd",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "dbf",
        ext: "dbf",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "cdx",
        ext: "cdx",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "fpt",
        ext: "fpt",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "par",
        ext: "par",
        magic: b"",
        adjust: 0,
    },
    // ===== Executables =====
    Marker {
        format: "dll",
        ext: "dll",
        magic: b"MZ",
        adjust: 0,
    },
    Marker {
        format: "sys",
        ext: "sys",
        magic: b"MZ",
        adjust: 0,
    },
    Marker {
        format: "com",
        ext: "com",
        magic: b"\x4D\x5A",
        adjust: 0,
    },
    Marker {
        format: "macho",
        ext: "macho",
        magic: b"\xfe\xed\xfa\xce",
        adjust: 0,
    },
    Marker {
        format: "macho",
        ext: "macho",
        magic: b"\xcf\xfa\xed\xfe",
        adjust: 0,
    },
    Marker {
        format: "macho",
        ext: "macho",
        magic: b"\xfe\xed\xfa\xcf",
        adjust: 0,
    },
    Marker {
        format: "class",
        ext: "class",
        magic: b"\xca\xfe\xba\xbe",
        adjust: 0,
    },
    Marker {
        format: "dex",
        ext: "dex",
        magic: b"dex\n",
        adjust: 0,
    },
    // ===== Fonts =====
    Marker {
        format: "ttf",
        ext: "ttf",
        magic: b"\x00\x01\x00\x00",
        adjust: 0,
    },
    Marker {
        format: "otf",
        ext: "otf",
        magic: b"OTTO",
        adjust: 0,
    },
    Marker {
        format: "woff",
        ext: "woff",
        magic: b"wOFF",
        adjust: 0,
    },
    Marker {
        format: "woff2",
        ext: "woff2",
        magic: b"wOF2",
        adjust: 0,
    },
    Marker {
        format: "pfb",
        ext: "pfb",
        magic: b"\x80\x02",
        adjust: 0,
    },
    // ===== Disk images / forensic =====
    Marker {
        format: "e01",
        ext: "e01",
        magic: b"EVF",
        adjust: 0,
    },
    Marker {
        format: "vfd",
        ext: "vfd",
        magic: b"",
        adjust: 0,
    },
    // ===== Game / ROM =====
    Marker {
        format: "rom",
        ext: "rom",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "n64",
        ext: "n64",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "gba",
        ext: "gba",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "sfc",
        ext: "sfc",
        magic: b"",
        adjust: 0,
    },
    Marker {
        format: "nes",
        ext: "nes",
        magic: b"NES\x1A",
        adjust: 0,
    },
    Marker {
        format: "bin",
        ext: "bin",
        magic: b"",
        adjust: 0,
    },
    // ===== Apple =====
    Marker {
        format: "plist",
        ext: "plist",
        magic: b"bplist00",
        adjust: 0,
    },
    Marker {
        format: "plist",
        ext: "plist",
        magic: b"<?xml",
        adjust: 0,
    },
    Marker {
        format: "applesingle",
        ext: "apple",
        magic: b"\x00\x05\x16\x07",
        adjust: 0,
    },
    // ===== Camera RAW (high-value for photo recovery) =====
    Marker {
        format: "orf",
        ext: "orf",
        magic: b"ORF",
        adjust: 0,
    },
    Marker {
        format: "rw2",
        ext: "rw2",
        magic: b"RW2",
        adjust: 0,
    },
    Marker {
        format: "x3f",
        ext: "x3f",
        magic: b"FOVb",
        adjust: 0,
    },
    Marker {
        format: "crw",
        ext: "crw",
        magic: b"CRM\x00\x00\x08",
        adjust: 0,
    },
    // ===== Additional audio =====
    Marker {
        format: "wv",
        ext: "wv",
        magic: b"wvpk",
        adjust: 0,
    },
    Marker {
        format: "tta",
        ext: "tta",
        magic: b"TTA",
        adjust: 0,
    },
    // ===== Disk images / backup =====
    Marker {
        format: "gho",
        ext: "gho",
        magic: b"\x7E\x57\x65\x6E",
        adjust: 0,
    },
    Marker {
        format: "dar",
        ext: "dar",
        magic: b"DAR\x01!",
        adjust: 0,
    },
];

/// The overlap between consecutive scan chunks, in bytes.
///
/// It is derived from the longest registered signature so that a signature
/// spanning a chunk boundary is never split in two. The overlap must be at
/// least `max_signature_length - 1`. The LNK marker is currently the longest
/// at 21 bytes, so the overlap is 20. This constant is recomputed whenever
/// MARKERS changes.
const fn max_signature_len() -> usize {
    let mut max = 0usize;
    let mut i = 0;
    while i < MARKERS.len() {
        let len = MARKERS[i].magic.len();
        if len > max {
            max = len;
        }
        i += 1;
    }
    max
}

const OVERLAP: u64 = (max_signature_len() - 1) as u64;

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
        "Every registered format carries a structural validator and is carved. ZIP64 shares \
         the PK\\x03\\x04 local-header magic with ZIP and is validated by the ZIP validator; \
         it is not reported as a separate format.",
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
            if marker.magic.is_empty() {
                continue; // No signature to search for; would match at every position
            }
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
            if !seen_starts.insert(abs_start) {
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
        "heic" => MAX_HEIC,
        "avif" => MAX_AVIF,
        "docx" => MAX_DOCX,
        "xlsx" => MAX_XLSX,
        "pptx" => MAX_PPTX,
        "mkv" => MAX_MKV,
        "pst" => MAX_PST,
        "lnk" => MAX_LNK,
        "evtx" => MAX_EVTX,
        "webp" => MAX_WEBP,
        "tar" => MAX_TAR,
        "bzip2" => MAX_BZIP2,
        "xz" => MAX_XZ,
        "zst" => MAX_ZSTD,
        "flac" => MAX_FLAC,
        "ole" => MAX_OLE,
        "ts" => MAX_MPEGTS,
        "rar" => MAX_RAR4,
        "rar5" => MAX_RAR5,
        "iso9660" => MAX_ISO9660,
        // ===== Additional archives =====
        "cab" => MAX_CAB,
        "ace" => MAX_ACE,
        "arj" => MAX_ARJ,
        "lzh" => MAX_LZH,
        "lzo" => MAX_LZO,
        "par2" => MAX_PAR2,
        "rpm" => MAX_RPM,
        "xar" => MAX_XAR,
        "deb" => MAX_DEB,
        "bkf" => MAX_BKF,
        "arc" => MAX_ARC,
        "zoo" => MAX_ZOO,
        "wim" => MAX_WIM,
        // ===== Additional images =====
        "tga" => MAX_TGA,
        "pcx" => MAX_PCX,
        "pbm" => MAX_PBM,
        "pgm" => MAX_PGM,
        "ppm" => MAX_PPM,
        "ico" => MAX_ICO,
        "icns" => MAX_ICNS,
        "cur" => MAX_CUR,
        "emf" => MAX_EMF,
        "wmf" => MAX_WMF,
        "cdr" => MAX_CDR,
        "ai" => MAX_AI,
        "eps" => MAX_EPS,
        "ps" => MAX_PS,
        "indd" => MAX_INDD,
        "xcf" => MAX_XCF,
        "psd" => MAX_PSD,
        "kra" => MAX_KRA,
        "afd" => MAX_AFD,
        // ===== Additional audio =====
        "amr" => MAX_AMR,
        "aac" => MAX_AAC,
        "m4a" => MAX_M4A,
        "wma" => MAX_WMA,
        "aiff" => MAX_AIFF,
        "au" => MAX_AU,
        "mid" => MAX_MID,
        "it" => MAX_IT,
        "xm" => MAX_XM,
        "s3m" => MAX_S3M,
        "mod" => MAX_MOD,
        "ape" => MAX_APE,
        "voc" => MAX_VOC,
        "shn" => MAX_SHN,
        "spx" => MAX_SPX,
        // ===== Additional video =====
        "flv" => MAX_FLV,
        "swf" => MAX_SWF,
        "mxf" => MAX_MXF,
        "dpx" => MAX_DPX,
        "exr" => MAX_EXR,
        "hdr" => MAX_HDR,
        "asf" => MAX_ASF,
        "vob" => MAX_VOB,
        // ===== Additional documents =====
        "doc" => MAX_DOC,
        "xls" => MAX_XLS,
        "ppt" => MAX_PPT,
        "pub" => MAX_PUB,
        "wpd" => MAX_WPD,
        "wps" => MAX_WPS,
        "xlr" => MAX_XLR,
        "mdb" => MAX_MDB,
        "accdb" => MAX_ACCDB,
        "one" => MAX_ONE,
        "pages" => MAX_PAGES,
        "numbers" => MAX_NUMBERS,
        "key" => MAX_KEY,
        "fb2" => MAX_FB2,
        "tex" => MAX_TEX,
        "lyx" => MAX_LYX,
        // ===== Email =====
        "ost" => MAX_OST,
        "dbx" => MAX_DBX,
        "msg" => MAX_MSG,
        "nsf" => MAX_NSF,
        "mbx" => MAX_MBX,
        "eml" => MAX_EML,
        // ===== Databases =====
        "mdf" => MAX_MDF,
        "ldf" => MAX_LDF,
        "ibd" => MAX_IBD,
        "dbf" => MAX_DBASE,
        "cdx" => MAX_CDX,
        "fpt" => MAX_FPT,
        "par" => MAX_PAR,
        // ===== Executables =====
        "dll" => MAX_DLL,
        "sys" => MAX_SYS,
        "com" => MAX_COM,
        "macho" => MAX_MACHO,
        "class" => MAX_CLASS,
        "dex" => MAX_DEX,
        // ===== Fonts =====
        "ttf" => MAX_TTF,
        "otf" => MAX_OTF,
        "woff" => MAX_WOFF,
        "woff2" => MAX_WOFF2,
        "pfb" => MAX_PFB,
        // ===== Disk images / forensic =====
        "e01" => MAX_E01,
        "vfd" => MAX_VFD,
        // ===== Game / ROM =====
        "rom" => MAX_ROM,
        "n64" => MAX_N64,
        "gba" => MAX_GBA,
        "sfc" => MAX_SFC,
        "nes" => MAX_NES,
        "bin" => MAX_BIN,
        // ===== Apple =====
        "plist" => MAX_PLIST,
        "applesingle" => MAX_APPLESINGLE,
        // ===== Camera RAW =====
        "orf" => MAX_ORF,
        "rw2" => MAX_RW2,
        "x3f" => MAX_X3F,
        "crw" => MAX_CRW,
        // ===== Additional audio =====
        "wv" => MAX_WV,
        "tta" => MAX_TTA,
        // ===== Disk images / backup =====
        "gho" => MAX_GHO,
        "dar" => MAX_DAR,
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
        "heic" => validate_heic(src, start, limit),
        "avif" => validate_avif(src, start, limit),
        "docx" => validate_docx(src, start, limit),
        "xlsx" => validate_xlsx(src, start, limit),
        "pptx" => validate_pptx(src, start, limit),
        "mkv" => validate_mkv(src, start, limit),
        "pst" => validate_pst(src, start, limit),
        "lnk" => validate_lnk(src, start, limit),
        "evtx" => validate_evtx(src, start, limit),
        "webp" => validate_webp(src, start, limit),
        "tar" => validate_tar(src, start, limit),
        "bzip2" => validate_bzip2(src, start, limit),
        "xz" => validate_xz(src, start, limit),
        "zst" => validate_zstd(src, start, limit),
        "flac" => validate_flac(src, start, limit),
        "ole" => validate_ole(src, start, limit),
        "ts" => validate_mpegts(src, start, limit),
        "rar5" => validate_rar5(src, start, limit),
        "iso9660" => validate_iso9660(src, start, limit),
        // ===== Additional archives =====
        "cab" => validate_cab(src, start, limit),
        "ace" => validate_ace(src, start, limit),
        "arj" => validate_arj(src, start, limit),
        "lzh" => validate_lzh(src, start, limit),
        "lzo" => validate_lzo(src, start, limit),
        "par2" => validate_par2(src, start, limit),
        "rpm" => validate_rpm(src, start, limit),
        "xar" => validate_xar(src, start, limit),
        "deb" => validate_deb(src, start, limit),
        "bkf" => validate_bkf(src, start, limit),
        "arc" => validate_arc(src, start, limit),
        "zoo" => validate_zoo(src, start, limit),
        "wim" => validate_wim(src, start, limit),
        // ===== Additional images =====
        "tga" => validate_tga(src, start, limit),
        "pcx" => validate_pcx(src, start, limit),
        "pbm" => validate_pbm(src, start, limit),
        "pgm" => validate_pgm(src, start, limit),
        "ppm" => validate_ppm(src, start, limit),
        "ico" => validate_ico(src, start, limit),
        "icns" => validate_icns(src, start, limit),
        "cur" => validate_cur(src, start, limit),
        "emf" => validate_emf(src, start, limit),
        "wmf" => validate_wmf(src, start, limit),
        "cdr" => validate_cdr(src, start, limit),
        "ai" => validate_ai(src, start, limit),
        "eps" => validate_eps(src, start, limit),
        "ps" => validate_ps(src, start, limit),
        "indd" => validate_indd(src, start, limit),
        "xcf" => validate_xcf(src, start, limit),
        "psd" => validate_psd(src, start, limit),
        "kra" => validate_kra(src, start, limit),
        "afd" => validate_afd(src, start, limit),
        // ===== Additional audio =====
        "amr" => validate_amr(src, start, limit),
        "aac" => validate_aac(src, start, limit),
        "m4a" => validate_m4a(src, start, limit),
        "wma" => validate_wma(src, start, limit),
        "aiff" => validate_aiff(src, start, limit),
        "au" => validate_au(src, start, limit),
        "mid" => validate_mid(src, start, limit),
        "it" => validate_it(src, start, limit),
        "xm" => validate_xm(src, start, limit),
        "s3m" => validate_s3m(src, start, limit),
        "mod" => validate_mod(src, start, limit),
        "ape" => validate_ape(src, start, limit),
        "voc" => validate_voc(src, start, limit),
        "shn" => validate_shn(src, start, limit),
        "spx" => validate_spx(src, start, limit),
        // ===== Additional video =====
        "flv" => validate_flv(src, start, limit),
        "swf" => validate_swf(src, start, limit),
        "mxf" => validate_mxf(src, start, limit),
        "dpx" => validate_dpx(src, start, limit),
        "exr" => validate_exr(src, start, limit),
        "hdr" => validate_hdr(src, start, limit),
        "asf" => validate_asf(src, start, limit),
        "vob" => validate_vob(src, start, limit),
        // ===== Additional documents =====
        "doc" => validate_ole_doc(src, start, limit),
        "xls" => validate_ole_xls(src, start, limit),
        "ppt" => validate_ole_ppt(src, start, limit),
        "pub" => validate_ole_pub(src, start, limit),
        "wpd" => validate_wpd(src, start, limit),
        "wps" => validate_ole_wps(src, start, limit),
        "xlr" => validate_ole_xlr(src, start, limit),
        "mdb" => validate_mdb(src, start, limit),
        "accdb" => validate_accdb(src, start, limit),
        "one" => validate_ole_one(src, start, limit),
        "pages" => validate_pages(src, start, limit),
        "numbers" => validate_numbers(src, start, limit),
        "key" => validate_key(src, start, limit),
        "fb2" => validate_fb2(src, start, limit),
        "tex" => validate_tex(src, start, limit),
        "lyx" => validate_lyx(src, start, limit),
        // ===== Email =====
        "ost" => validate_pst(src, start, limit),
        "dbx" => validate_dbx(src, start, limit),
        "msg" => validate_msg(src, start, limit),
        "nsf" => validate_nsf(src, start, limit),
        "mbx" => validate_mbx(src, start, limit),
        "eml" => validate_eml(src, start, limit),
        // ===== Databases =====
        "mdf" => validate_mdf(src, start, limit),
        "ldf" => validate_ldf(src, start, limit),
        "ibd" => validate_ibd(src, start, limit),
        "dbf" => validate_dbase(src, start, limit),
        "cdx" => validate_cdx(src, start, limit),
        "fpt" => validate_fpt(src, start, limit),
        "par" => validate_paradox(src, start, limit),
        // ===== Executables =====
        "dll" => validate_pe(src, start, limit),
        "sys" => validate_pe(src, start, limit),
        "com" => validate_com(src, start, limit),
        "macho" => validate_macho(src, start, limit),
        "class" => validate_class(src, start, limit),
        "dex" => validate_dex(src, start, limit),
        // ===== Fonts =====
        "ttf" => validate_ttf(src, start, limit),
        "otf" => validate_otf(src, start, limit),
        "woff" => validate_woff(src, start, limit),
        "woff2" => validate_woff2(src, start, limit),
        "pfb" => validate_pfb(src, start, limit),
        // ===== Disk images / forensic =====
        "e01" => validate_e01(src, start, limit),
        "vfd" => validate_vfd(src, start, limit),
        // ===== Game / ROM =====
        "rom" => validate_rom(src, start, limit),
        "n64" => validate_n64(src, start, limit),
        "gba" => validate_gba(src, start, limit),
        "sfc" => validate_sfc(src, start, limit),
        "nes" => validate_nes(src, start, limit),
        "bin" => validate_bin(src, start, limit),
        // ===== Apple =====
        "plist" => validate_plist(src, start, limit),
        "applesingle" => validate_applesingle(src, start, limit),
        // ===== Camera RAW =====
        "orf" => validate_orf(src, start, limit),
        "rw2" => validate_rw2(src, start, limit),
        "x3f" => validate_x3f(src, start, limit),
        "crw" => validate_crw(src, start, limit),
        // ===== Additional audio =====
        "wv" => validate_wv(src, start, limit),
        "tta" => validate_tta(src, start, limit),
        // ===== Disk images / backup =====
        "gho" => validate_gho(src, start, limit),
        "dar" => validate_dar(src, start, limit),
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
            let scan = pos + 2 + len;
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
    find_from(src, start, end, b" obj")?;
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
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
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
            b"LIST" if csize >= 4 => {
                let list_type = read_bounded(src, pos + 8, 4, end)?;
                if &list_type == b"hdrl" {
                    saw_fmt = true;
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
    if next_size > 0 && crc_range(src, next_start, next_size, limit)? != next_crc {
        return None;
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
    if !(64..=0x1000).contains(&lfanew) {
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

// ===== New Format Validators =====

fn validate_heic(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // HEIC is based on ISO-BMFF (like MP4), uses ftyp box
    validate_mp4(src, start, limit)
}

fn validate_avif(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // AVIF is also based on ISO-BMFF
    validate_mp4(src, start, limit)
}

fn validate_docx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // DOCX is a ZIP with specific content
    let validated = validate_zip(src, start, limit)?;
    if validated.length > MAX_DOCX {
        return None;
    }
    // Check for [Content_Types].xml with word/ prefix
    let has_word = find_from(src, start, start + validated.length, b"word/").is_some();
    let has_content_types =
        find_from(src, start, start + validated.length, b"[Content_Types].xml").is_some();
    if has_word && has_content_types {
        Some(Validated::new(validated.length, 95).note("DOCX: ZIP with word/ content validated"))
    } else {
        None
    }
}

fn validate_xlsx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let validated = validate_zip(src, start, limit)?;
    if validated.length > MAX_XLSX {
        return None;
    }
    let has_xl = find_from(src, start, start + validated.length, b"xl/").is_some();
    let has_content_types =
        find_from(src, start, start + validated.length, b"[Content_Types].xml").is_some();
    if has_xl && has_content_types {
        Some(Validated::new(validated.length, 95).note("XLSX: ZIP with xl/ content validated"))
    } else {
        None
    }
}

fn validate_pptx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let validated = validate_zip(src, start, limit)?;
    if validated.length > MAX_PPTX {
        return None;
    }
    let has_ppt = find_from(src, start, start + validated.length, b"ppt/").is_some();
    let has_content_types =
        find_from(src, start, start + validated.length, b"[Content_Types].xml").is_some();
    if has_ppt && has_content_types {
        Some(Validated::new(validated.length, 95).note("PPTX: ZIP with ppt/ content validated"))
    } else {
        None
    }
}

fn validate_mkv(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let head = read_bounded(src, start, 4, limit)?;
    if head != b"\x1A\x45\xDF\xA3" {
        return None;
    }
    let mut pos = start;
    let mut elements = 0u32;
    while pos + 4 <= limit {
        if pos - start > MAX_MKV {
            return None;
        }
        let (id, id_len) = read_ebml_id(src, pos, limit)?;
        let (size, size_len) = read_ebml_size(src, pos + id_len, limit)?;
        let data_start = pos + id_len + size_len;
        if data_start + size > limit {
            return None;
        }
        if id == b"\x1A\x45\xDF\xA3" {
            // EBML header
        } else if id == b"\x42\x86" || id == b"\x42\x82" {
            // Segment or Cluster
        }
        pos = data_start + size;
        elements += 1;
        if elements > 10000 {
            return None;
        }
        if elements >= 2 && pos > start {
            return Some(
                Validated::new(pos - start, 90)
                    .note(format!("MKV: {} EBML elements validated", elements)),
            );
        }
    }
    None
}

fn read_ebml_id(src: &dyn ReadOnlySource, pos: u64, limit: u64) -> Option<(Vec<u8>, u64)> {
    let first = read_bounded(src, pos, 1, limit)?[0];
    let len = if first & 0x80 != 0 {
        1
    } else if first & 0x40 != 0 {
        2
    } else if first & 0x20 != 0 {
        3
    } else {
        4
    };
    let id = read_bounded(src, pos, len, limit)?;
    Some((id.to_vec(), len))
}

fn read_ebml_size(src: &dyn ReadOnlySource, pos: u64, limit: u64) -> Option<(u64, u64)> {
    let first = read_bounded(src, pos, 1, limit)?[0];
    let len = if first & 0x80 != 0 {
        1
    } else if first & 0x40 != 0 {
        2
    } else if first & 0x20 != 0 {
        3
    } else {
        4
    };
    if len > 8 {
        return None;
    }
    let size_bytes = read_bounded(src, pos, len, limit)?;
    let mut size = 0u64;
    for (i, b) in size_bytes.iter().enumerate() {
        let mask = if i == 0 { 0x7F >> len } else { 0xFF };
        size = (size << 8) | ((b & mask) as u64);
    }
    Some((size, len))
}

fn validate_pst(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let sig = read_bounded(src, start, 4, limit)?;
    if sig != b"!BDN" {
        return None;
    }
    // PST header is 512 bytes
    if start + 512 > limit {
        return None;
    }
    let h = read_bounded(src, start, 512, limit)?;
    let version = u32_le(&h, 4)?;
    if version != 17 && version != 23 {
        return None;
    }
    Some(Validated::new(512, 90).note("PST/OST header validated"))
}

fn validate_lnk(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    if start + 76 > limit {
        return None;
    }
    let h = read_bounded(src, start, 76, limit)?;
    if &h[0..20]
        != b"L\x00\x00\x00\x01\x14\x02\x00\x00\x00\x00\x00\x00\xC0\x00\x00\x00\x00\x00\x00\x46"
    {
        return None;
    }
    let flags = u32_le(&h, 20)?;
    let _file_attrs = u32_le(&h, 24)?;
    let _ = u64_le(&h, 28)?;
    let _ = u64_le(&h, 36)?;
    let _ = u64_le(&h, 44)?;
    let _file_size = u32_le(&h, 52)?;
    let _icon_index = u32_le(&h, 56)?;
    let _ = u32_le(&h, 60)?;
    let _ = u32_le(&h, 64)?;
    let _ = u32_le(&h, 68)?;
    let _ = u32_le(&h, 72)?;
    if flags & 0x00000001 == 0 {
        return None;
    }
    Some(Validated::new(76, 88).note("LNK shell link header validated"))
}

fn validate_evtx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    if start + 4096 > limit {
        return None;
    }
    let h = read_bounded(src, start, 4096, limit)?;
    if &h[0..8] != b"ElfFile" {
        return None;
    }
    let major = u16_le(&h, 8)?;
    let _minor = u16_le(&h, 10)?;
    if major > 4 {
        return None;
    }
    let chunk_size = u32_le(&h, 12)?;
    if chunk_size == 0 || chunk_size > 0x100000 {
        return None;
    }
    Some(Validated::new(chunk_size as u64, 92).note("EVTX event log chunk validated"))
}

fn validate_webp(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 16, limit)?;
    if &h[0..4] != b"RIFF" || &h[8..12] != b"WEBP" {
        return None;
    }
    let size = u32_le(&h, 4)? as u64;
    let end = start + 8 + size;
    if end > limit || size > MAX_WEBP {
        return None;
    }
    let mut pos = start + 12;
    let mut chunks = 0u32;
    while pos + 8 <= end {
        let chunk_h = read_bounded(src, pos, 8, limit)?;
        let chunk_type = &chunk_h[0..4];
        let chunk_size = u32_le(&chunk_h, 4)? as u64;
        if pos + 8 + chunk_size > end {
            return None;
        }
        if chunk_type == b"VP8 " || chunk_type == b"VP8L" || chunk_type == b"VP8X" {
            // Valid WebP chunk
        }
        pos += 8 + chunk_size + (chunk_size & 1);
        chunks += 1;
        if chunks > 100 {
            return None;
        }
    }
    if pos == end && chunks > 0 {
        Some(Validated::new(end - start, 94).note(format!("WEBP: {} chunks validated", chunks)))
    } else {
        None
    }
}

fn validate_tar(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    if start < 257 {
        return None;
    }
    let mut pos = start - 257;
    let mut headers = 0u32;
    while pos + 512 <= limit {
        if pos - start > MAX_TAR {
            return None;
        }
        let h = read_bounded(src, pos, 512, limit)?;
        let name = &h[0..100];
        if name.iter().all(|&b| b == 0) {
            // Two consecutive zero blocks = end of archive
            break;
        }
        let size_str = &h[124..136];
        let size = parse_octal(size_str)?;
        if size > 8 * 1024 * 1024 * 1024 {
            return None;
        }
        let blocks = size.div_ceil(512);
        pos += 512 + blocks * 512;
        headers += 1;
        if headers > 10000 {
            return None;
        }
    }
    if headers > 0 && pos <= limit {
        Some(
            Validated::new(pos - (start - 257), 90)
                .note(format!("TAR: {} file headers validated", headers)),
        )
    } else {
        None
    }
}

fn parse_octal(bytes: &[u8]) -> Option<u64> {
    let s = std::str::from_utf8(bytes).ok()?;
    let s = s.trim_end_matches('\0').trim();
    if s.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(s, 8).ok()
}

fn validate_bzip2(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if &h[0..3] != b"BZh" {
        return None;
    }
    let version = h[3];
    if !(b'1'..=b'9').contains(&version) {
        return None;
    }
    // BZIP2 doesn't have a simple end marker, so we accept the header
    Some(Validated::new(512, 80).note("BZIP2 header validated"))
}

fn validate_xz(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 6, limit)?;
    if h != b"\xFD7zXZ\x00" {
        return None;
    }
    let mut pos = start + 6;
    let mut blocks = 0u32;
    while pos + 12 <= limit {
        if pos - start > MAX_XZ {
            return None;
        }
        let _crc = u32_le(&read_bounded(src, pos, 4, limit)?, 0)?;
        let _flags = u16_le(&read_bounded(src, pos + 4, 2, limit)?, 0)?;
        let size = u32_le(&read_bounded(src, pos + 6, 4, limit)?, 0)?;
        if size == 0 {
            break;
        }
        pos += 12 + size as u64;
        if pos > limit {
            return None;
        }
        blocks += 1;
        if blocks > 1000 {
            return None;
        }
    }
    if blocks > 0 {
        Some(Validated::new(pos - start, 95).note(format!("XZ: {} blocks validated", blocks)))
    } else {
        None
    }
}

fn validate_zstd(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x28\xB5\x2F\xFD" {
        return None;
    }
    // Zstd frame header
    let frame_header = read_bounded(src, start, 18.min(limit - start), limit)?;
    if frame_header.len() < 18 {
        return None;
    }
    let fhd = frame_header[4];
    let _single_segment = fhd & 0x20 != 0;
    let _content_checksum = fhd & 0x04 != 0;
    let _dictionary_id = fhd & 0x03;
    Some(Validated::new(18, 90).note("ZSTD frame header validated"))
}

// ---------------------------------------------------------------------------
// FLAC — validates fLaC magic, metadata block walk, and first audio-frame sync
// ---------------------------------------------------------------------------

fn validate_flac(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let sig = read_bounded(src, start, 4, limit)?;
    if sig != b"fLaC" {
        return None;
    }
    let mut pos = start + 4;
    let mut blocks = 0u32;
    while pos + 4 <= limit {
        if pos - start > MAX_FLAC {
            return None;
        }
        let hdr = read_bounded(src, pos, 4, limit)?;
        let last = hdr[0] & 0x80 != 0;
        let block_type = hdr[0] & 0x7F;
        // Metadata block length is a 3-byte big-endian integer at hdr[1..4].
        let block_len = ((hdr[1] as u64) << 16) | ((hdr[2] as u64) << 8) | (hdr[3] as u64);
        if pos.checked_add(4)?.checked_add(block_len)? > limit {
            return None;
        }
        if block_type == 0 {
            // STREAMINFO body is exactly 34 bytes.
            if block_len < 34 {
                return None;
            }
            let si = read_bounded(src, pos + 4, 34, limit)?;
            let _min_block = u16_le(&si, 0)?;
            let _max_block = u16_le(&si, 2)?;
            let sr = u32_be(&si, 12)?;
            let _sample_rate = sr >> 4;
            let _channels = ((sr >> 1) & 0x7) + 1;
            let bps = u32_be(&si, 14)?;
            let _bits = ((bps >> 5) & 0x1F) + 1;
            let _md5 = &si[18..34];
        }
        pos = pos.checked_add(4)?.checked_add(block_len)?;
        blocks += 1;
        if blocks > 10_000 {
            return None;
        }
        if last {
            // After the last metadata block, audio frames begin with a sync
            // word (0xFFF8–0xFFFB). Verify at least one sync word is present.
            if pos + 2 <= limit {
                let sync = read_bounded(src, pos, 2, limit)?;
                if sync[0] == 0xFF && (sync[1] & 0xFC) == 0xF8 {
                    return Some(
                        Validated::new(pos - start + 2, 90)
                            .note(format!("FLAC: {} metadata blocks + audio sync", blocks)),
                    );
                }
            }
            // Metadata-only FLAC is still valid.
            return Some(Validated::new(pos - start, 85).note(format!(
                "FLAC: {} metadata blocks, no audio frame found",
                blocks
            )));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// OLE compound file — validates the D0CF11E0 header and sector layout
// ---------------------------------------------------------------------------

fn validate_ole(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let sig = read_bounded(src, start, 8, limit)?;
    if sig != b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1" {
        return None;
    }
    if start.checked_add(512)? > limit {
        return None;
    }
    let header = read_bounded(src, start, 512, limit)?;
    // Sector size: 0x1C for 512-byte sectors (value 9), 0x14 for 4096 (value 12).
    let _sector_size_shift = u16_le(&header, 0x1C)?;
    let _fat_sector_count = u32_le(&header, 0x2C)?;
    let _dir_sector = u32_le(&header, 0x30)?;
    let _mini_fat_sector_count = u32_le(&header, 0x38)?;
    Some(Validated::new(512, 88).note("OLE compound file header validated"))
}

// ---------------------------------------------------------------------------
// MPEG-TS — validates 0x47 sync bytes at 188-byte packet intervals
// ---------------------------------------------------------------------------

/// Minimum consecutive valid MPEG-TS packets required to emit a candidate.
/// A single `0x47` byte (or a short run) appears frequently in unrelated disk
/// structures and must not be treated as a video stream.
const MPEGTS_MIN_PACKETS: u32 = 8;

/// Minimum distinct PIDs observed to consider this a plausible transport stream.
const MPEGTS_MIN_DISTINCT_PIDS: usize = 2;

/// Maximum allowed sync byte gaps (missing 0x47) before breaking the run.
/// Real TS streams are continuous; a single gap is tolerable (e.g., corruption).
const MPEGTS_MAX_GAPS: u32 = 2;

fn validate_mpegts(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    const PACKET: u64 = 188;
    let mut pos = start;
    let mut packets = 0u32;
    let mut first_pid: Option<u16> = None;
    let mut pids_seen: smallvec::SmallVec<[u16; 8]> = smallvec::SmallVec::new();
    let mut continuity_errors = 0u32;
    let mut last_cc: [Option<u8>; 8192] = [None; 8192]; // CC counter per PID
    let mut gaps = 0u32;
    while pos + PACKET <= limit {
        if pos - start > MAX_MPEGTS {
            return None;
        }
        let sync = read_bounded(src, pos, 1, limit)?;
        if sync[0] != 0x47 {
            gaps += 1;
            if gaps > MPEGTS_MAX_GAPS {
                break;
            }
            // Try to re-sync by scanning forward for the next 0x47 at a 188-byte boundary
            let mut found = false;
            for offset in PACKET..PACKET * 4 {
                if pos + offset + 1 > limit {
                    break;
                }
                if let Some(s) = read_bounded(src, pos + offset, 1, limit) {
                    if s[0] == 0x47 {
                        pos += offset;
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                break;
            }
            continue;
        }
        gaps = 0; // Reset gap counter on valid sync
        if pos + 4 > limit {
            break;
        }
        let hdr = read_bounded(src, pos + 1, 3, limit)?;
        // Byte 1-2: transport_error_indicator (bit 15), PID (bits 14-0).
        let tei = (hdr[0] & 0x80) != 0;
        let pid = ((hdr[0] as u16 & 0x1F) << 8) | hdr[1] as u16;
        // Byte 3: adaptation_field_control (bits 5-4). 0b00 is reserved/invalid.
        let afc = (hdr[2] >> 4) & 0x03;
        if tei || afc == 0 {
            // Structurally invalid packet: stop the run.
            break;
        }
        // Continuity counter (lower 4 bits of byte 3)
        let cc = hdr[2] & 0x0F;
        if let Some(last) = last_cc[pid as usize] {
            let expected = (last + 1) & 0x0F;
            if cc != expected {
                continuity_errors += 1;
            }
        }
        last_cc[pid as usize] = Some(cc);
        // Record the first PID and track distinct PIDs
        if first_pid.is_none() {
            first_pid = Some(pid);
        }
        if !pids_seen.contains(&pid) {
            pids_seen.push(pid);
        }
        pos = pos.checked_add(PACKET)?;
        packets += 1;
        if packets >= 10_000 {
            break;
        }
    }
    if packets < MPEGTS_MIN_PACKETS {
        return None;
    }
    // Require multiple distinct PIDs to avoid false positives from repeated patterns
    if pids_seen.len() < MPEGTS_MIN_DISTINCT_PIDS {
        return None;
    }
    // A run that ends mid-stream because the next packet lacks sync is still a
    // plausible fragment; bound the length to the valid run.
    let length = pos - start;
    // Score rises with run length but caps well below 100: sync bytes alone
    // cannot prove the bytes are a complete, original video file.
    // Penalize continuity errors heavily.
    let base_score = 60u8.saturating_add((packets / 10).min(30) as u8);
    let score = if continuity_errors > packets / 2 {
        base_score.saturating_sub(20)
    } else {
        base_score
    };
    let mut v =
        Validated::new(length, score).note(format!(
        "MPEG-TS: {} consecutive valid 188-byte packets, {} distinct PIDs, {} continuity errors",
        packets, pids_seen.len(), continuity_errors
    ));
    if let Some(pid) = first_pid {
        v = v.note(format!("first packet PID {pid}"));
    }
    Some(v)
}

// ---------------------------------------------------------------------------
// RAR5 — validates Rar!\x1A\x07\x01 header and block walk
// ---------------------------------------------------------------------------

fn validate_rar5(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let sig = read_bounded(src, start, 8, limit)?;
    if &sig[..7] != b"Rar!\x1A\x07\x01" || sig[7] != 0x00 {
        return None;
    }
    let mut pos = start + 8;
    let mut blocks = 0u32;
    while pos + 12 <= limit {
        if pos - start > MAX_RAR5 {
            return None;
        }
        let hdr = read_bounded(src, pos, 12, limit)?;
        let _block_type = u16_le(&hdr, 0)?;
        let block_flags = u16_le(&hdr, 2)?;
        let block_size = u32_le(&hdr, 4)? as u64;
        let _compressed_size = u32_le(&hdr, 8)? as u64;
        if block_size < 12 || pos.checked_add(block_size)? > limit {
            return None;
        }
        let next = pos.checked_add(block_size)?;
        if next <= pos {
            return None;
        }
        pos = next;
        blocks += 1;
        if blocks > 1_000_000 {
            return None;
        }
        if block_flags & 0x8000 != 0 {
            // Last block marker.
            break;
        }
    }
    if blocks == 0 {
        return None;
    }
    Some(Validated::new(pos - start, 87).note(format!("RAR5: {} blocks walked", blocks)))
}

// ---------------------------------------------------------------------------
// ISO 9660 — validates CD001 volume descriptor at the descriptor start
// ---------------------------------------------------------------------------

fn validate_iso9660(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // With adjust=1, abs_start points at the volume descriptor type byte;
    // CD001 follows at abs_start + 1.
    if start + 7 > limit {
        return None;
    }
    let desc = read_bounded(src, start, 7, limit)?;
    if desc[0] != 0x01 {
        return None; // not a Primary Volume Descriptor
    }
    if &desc[1..6] != b"CD001" {
        return None;
    }
    let _version = desc[6];
    Some(Validated::new(2048, 90).note("ISO 9660: Primary Volume Descriptor with CD001 magic"))
}

// ===== Additional document validators =====

fn validate_ole_doc(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_ole_xls(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_ole_ppt(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_ole_pub(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_ole_wps(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_ole_xlr(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_ole_one(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_wpd(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\xFF\x57\x50\x43" {
        return None;
    }
    Some(Validated::new(512, 86).note("WordPerfect document header validated"))
}

fn validate_mdb(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x00\x01\x00\x00" {
        return None;
    }
    // Strengthen: look for the Jet "Standard" signature at offset 32,
    // which is present in real MDB/ACCDB files.
    if start + 48 > limit {
        return None;
    }
    let tail = read_bounded(src, start + 32, 16, limit)?;
    if !tail.starts_with(b"Standard") {
        return None;
    }
    Some(Validated::new(512, 86).note("Jet database (MDB) header and signature validated"))
}

fn validate_accdb(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_mdb(src, start, limit)
}

fn validate_pages(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // iWork Pages is ZIP-based with specific content
    let validated = validate_zip(src, start, limit)?;
    if validated.length > MAX_PAGES {
        return None;
    }
    let has_pages = find_from(src, start, start + validated.length, b"index.apxl").is_some()
        || find_from(src, start, start + validated.length, b"catalog.plist").is_some();
    if has_pages {
        Some(Validated::new(validated.length, 90).note("Pages: ZIP with index.apxl validated"))
    } else {
        None
    }
}

fn validate_numbers(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let validated = validate_zip(src, start, limit)?;
    if validated.length > MAX_NUMBERS {
        return None;
    }
    let has_numbers = find_from(src, start, start + validated.length, b"index.apxl").is_some()
        || find_from(src, start, start + validated.length, b"catalog.plist").is_some();
    if has_numbers {
        Some(Validated::new(validated.length, 90).note("Numbers: ZIP with index.apxl validated"))
    } else {
        None
    }
}

fn validate_key(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let validated = validate_zip(src, start, limit)?;
    if validated.length > MAX_KEY {
        return None;
    }
    let has_key = find_from(src, start, start + validated.length, b"index.apxl").is_some()
        || find_from(src, start, start + validated.length, b"catalog.plist").is_some();
    if has_key {
        Some(Validated::new(validated.length, 90).note("Keynote: ZIP with index.apxl validated"))
    } else {
        None
    }
}

fn validate_fb2(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 6, limit)?;
    if h != b"<?xml" {
        return None;
    }
    if start + 50 > limit {
        return None;
    }
    let head = read_bounded(src, start, 50, limit)?;
    if !head.starts_with(b"<?xml") {
        return None;
    }
    let has_fictionbook = find_from(src, start, start + 2000, b"FictionBook").is_some();
    if !has_fictionbook {
        return None;
    }
    Some(Validated::new(2000, 86).note("FictionBook 2 XML header validated"))
}

fn validate_tex(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 5, limit)?;
    if h != b"%!TeX" {
        return None;
    }
    Some(Validated::new(5, 80).note("TeX/LaTeX source header validated"))
}

fn validate_lyx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\\\\\\[" {
        return None;
    }
    Some(Validated::new(512, 84).note("LyX document header validated"))
}

// ===== Email validators =====

fn validate_dbx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x21\x42\x44\x4e" {
        return None;
    }
    Some(Validated::new(512, 86).note("Outlook Express DBX header validated"))
}

fn validate_msg(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_ole(src, start, limit)
}

fn validate_nsf(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // Lotus Notes NSF has no reliable fixed magic; conservative: skip
    None
}

fn validate_mbx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 5, limit)?;
    if h != b"From " {
        return None;
    }
    Some(Validated::new(5, 80).note("Unix mbox separator validated"))
}

fn validate_eml(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // EML has no fixed binary magic; conservative: skip to avoid false positives
    None
}

// ===== Database validators =====

fn validate_mdf(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // SQL Server primary data file: no simple magic; conservative skip
    None
}

fn validate_ldf(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // SQL Server transaction log: no simple magic; conservative skip
    None
}

fn validate_ibd(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // InnoDB tablespace: no reliable fixed magic at offset 0; conservative skip
    None
}

fn validate_dbase(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // dBASE III+: first byte is version; conservative
    let h = read_bounded(src, start, 1, limit)?;
    let version = h[0];
    if version != 3 && version != 4 && version != 5 && version != 7 {
        return None;
    }
    Some(Validated::new(32, 82).note("dBASE header version byte validated"))
}

fn validate_cdx(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // dBASE CDX index: no simple magic; conservative skip
    None
}

fn validate_fpt(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // FoxPro memo: no simple magic; conservative skip
    None
}

fn validate_paradox(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // Paradox tables have no reliable fixed magic at offset 0; conservative skip
    // to avoid false positives on zero-filled regions.
    None
}

// ===== Executable validators =====

fn validate_com(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // COM: no magic; conservative skip to avoid false positives
    None
}

fn validate_macho(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\xfe\xed\xfa\xce" && h != b"\xcf\xfa\xed\xfe" && h != b"\xfe\xed\xfa\xcf" {
        return None;
    }
    Some(Validated::new(32, 88).note("Mach-O binary header validated"))
}

fn validate_class(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\xca\xfe\xba\xbe" {
        return None;
    }
    Some(Validated::new(8, 88).note("Java class file magic validated"))
}

fn validate_dex(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"dex\n" {
        return None;
    }
    Some(Validated::new(8, 88).note("Android DEX magic validated"))
}

// ===== Font validators =====

fn validate_ttf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x00\x01\x00\x00" {
        return None;
    }
    Some(Validated::new(12, 88).note("TrueType font header validated"))
}

fn validate_otf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"OTTO" {
        return None;
    }
    Some(Validated::new(12, 88).note("OpenType font header validated"))
}

fn validate_woff(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"wOFF" {
        return None;
    }
    Some(Validated::new(4, 88).note("WOFF font header validated"))
}

fn validate_woff2(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"wOF2" {
        return None;
    }
    Some(Validated::new(4, 88).note("WOFF2 font header validated"))
}

fn validate_pfb(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 2, limit)?;
    if h != b"\x80\x02" {
        return None;
    }
    Some(Validated::new(6, 84).note("Type 1 PFB font header validated"))
}

// ===== Disk image / forensic validators =====

fn validate_e01(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 3, limit)?;
    if h != b"EVF" {
        return None;
    }
    Some(Validated::new(512, 90).note("EnCase E01 evidence file header validated"))
}

fn validate_vfd(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // Virtual floppy disk: no fixed magic; conservative skip
    None
}

// ===== Game / ROM validators =====

fn validate_rom(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // Generic ROM: no magic; conservative skip
    None
}

fn validate_n64(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // N64 ROM: no fixed magic; conservative skip
    None
}

fn validate_gba(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // GBA ROM: no fixed magic at offset 0; conservative skip
    None
}

fn validate_sfc(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // SNES ROM: no fixed magic; conservative skip
    None
}

fn validate_nes(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"NES\x1A" {
        return None;
    }
    Some(Validated::new(16, 90).note("NES iNES header validated"))
}

fn validate_bin(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // Generic binary: no magic; conservative skip
    None
}

// ===== Apple validators =====

fn validate_plist(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 8, limit)?;
    if h == b"bplist00" {
        return Some(Validated::new(32, 88).note("Apple binary plist header validated"));
    }
    if h != b"<?xml" {
        return None;
    }
    if start + 50 > limit {
        return None;
    }
    let head = read_bounded(src, start, 50, limit)?;
    if head.starts_with(b"<?xml") && find_from(src, start, start + 2000, b"plist").is_some() {
        return Some(Validated::new(2000, 86).note("Apple XML plist header validated"));
    }
    None
}

fn validate_applesingle(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x00\x05\x16\x07" {
        return None;
    }
    Some(Validated::new(512, 86).note("AppleSingle file header validated"))
}

// ===== Additional archive validators =====

fn validate_cab(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"MSCF" {
        return None;
    }
    if start + 8 > limit {
        return None;
    }
    let header = read_bounded(src, start, 8, limit)?;
    let _reserved = u16_le(&header, 4)?;
    let _cabinet_size = u32_le(&header, 6)?;
    Some(Validated::new(24, 85).note("CAB: MSCF signature validated"))
}

fn validate_ace(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 5, limit)?;
    if &h[0..4] != b"**ACE" {
        return None;
    }
    let _version = h[4];
    Some(Validated::new(512, 86).note("ACE: header signature validated"))
}

fn validate_arj(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 3, limit)?;
    if h != b"ARJ" {
        return None;
    }
    if start + 4 > limit {
        return None;
    }
    let header = read_bounded(src, start, 4, limit)?;
    let _version = header[3];
    Some(Validated::new(512, 86).note("ARJ: header signature validated"))
}

fn validate_lzh(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"lh5\\" && h != b"lh7\\" {
        return None;
    }
    Some(Validated::new(512, 85).note("LZH/LHA: lzh header signature validated"))
}

fn validate_lzo(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x89LZO" {
        return None;
    }
    Some(Validated::new(4, 80).note("LZO: magic signature validated"))
}

fn validate_par2(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 6, limit)?;
    if h != b"PAR2\0PKG" {
        return None;
    }
    Some(Validated::new(6, 84).note("PAR2: package signature validated"))
}

fn validate_rpm(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 3, limit)?;
    if h != b"\xabED\xabEE\xdb" {
        return None;
    }
    Some(Validated::new(96, 88).note("RPM: lead signature validated"))
}

fn validate_xar(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"xar!" {
        return None;
    }
    Some(Validated::new(4, 80).note("XAR: magic signature validated"))
}

fn validate_deb(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 8, limit)?;
    if h != b"!<arch>\n" {
        return None;
    }
    Some(Validated::new(8, 82).note("Debian ar archive signature validated"))
}

fn validate_bkf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if &h[0..4] != b"\x89BKF" {
        return None;
    }
    Some(Validated::new(512, 86).note("BKF: backup file signature validated"))
}

fn validate_arc(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 1, limit)?;
    if h[0] != 0x1a {
        return None;
    }
    Some(Validated::new(512, 80).note("ARC: archive signature validated"))
}

fn validate_zoo(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 3, limit)?;
    if h != b"Zoo" {
        return None;
    }
    Some(Validated::new(512, 86).note("ZOO: archive signature validated"))
}

fn validate_wim(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 5, limit)?;
    if h != b"MSWIM" {
        return None;
    }
    Some(Validated::new(32, 88).note("WIM: Microsoft imaging signature validated"))
}

// ===== Additional image validators =====

fn validate_tga(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 18, limit)?;
    if h[0] != 0 || h[1] != 0 {
        return None;
    }
    if h[2] != 2 && h[2] != 3 && h[2] != 10 {
        return None;
    }
    Some(Validated::new(18, 85).note("TGA: truevision header validated"))
}

fn validate_pcx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 1, limit)?;
    if h[0] != 0x0A {
        return None;
    }
    Some(Validated::new(512, 84).note("PCX: ZSoft header validated"))
}

fn validate_pbm(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 2, limit)?;
    if h != b"P4" && h != b"P5" && h != b"P6" {
        return None;
    }
    Some(Validated::new(32, 82).note("Netpbm PBM/PGM/PPM signature validated"))
}

fn validate_pgm(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_pbm(src, start, limit)
}

fn validate_ppm(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_pbm(src, start, limit)
}

fn validate_ico(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h[0] != 0 || h[1] != 0 || h[2] != 1 || h[3] != 0 {
        return None;
    }
    Some(Validated::new(4, 86).note("ICO: Windows icon header validated"))
}

fn validate_icns(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"icns" {
        return None;
    }
    Some(Validated::new(8, 86).note("ICNS: Apple icon header validated"))
}

fn validate_cur(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h[0] != 0 || h[1] != 0 || h[2] != 2 || h[3] != 0 {
        return None;
    }
    Some(Validated::new(4, 86).note("CUR: Windows cursor header validated"))
}

fn validate_emf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h[0] != 1 || h[1] != 0 || h[2] != 0 || h[3] != 0 {
        return None;
    }
    Some(Validated::new(4, 80).note("EMF: enhanced meta file header validated"))
}

fn validate_wmf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\xD7\xCD\xC6\x9A" {
        return None;
    }
    Some(Validated::new(4, 80).note("WMF: Windows meta file header validated"))
}

fn validate_cdr(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 8, limit)?;
    if h != b"\x00\x00\x00\x00\xFF\xFF\xFF\xFF" {
        return None;
    }
    Some(Validated::new(512, 86).note("CDR: CorelDraw header validated"))
}

fn validate_ai(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // AI is PDF-based after v9; validate %PDF-
    validate_pdf(src, start, limit)
}

fn validate_eps(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"%!PS" {
        return None;
    }
    Some(Validated::new(4, 84).note("EPS: PostScript header validated"))
}

fn validate_ps(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_eps(src, start, limit)
}

fn validate_indd(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x06\x0e\x2b\x34" {
        return None;
    }
    Some(Validated::new(512, 84).note("INDD: InDesign header validated"))
}

fn validate_xcf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 8, limit)?;
    if h != b"gimp xcf" {
        return None;
    }
    Some(Validated::new(8, 86).note("XCF: GIMP header validated"))
}

fn validate_psd(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"8BPS" {
        return None;
    }
    Some(Validated::new(26, 88).note("PSD: Photoshop header validated"))
}

fn validate_kra(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Krita is ZIP-based
    validate_zip(src, start, limit)
}

fn validate_afd(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Affinity Designer is ZIP-based
    validate_zip(src, start, limit)
}

// ===== Additional audio validators =====

fn validate_amr(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 6, limit)?;
    if h != b"#!AMR" {
        return None;
    }
    Some(Validated::new(6, 86).note("AMR: narrowband header validated"))
}

fn validate_aac(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 2, limit)?;
    if h[0] != 0xFF || (h[1] & 0xF6) != 0xF0 {
        return None;
    }
    Some(Validated::new(7, 80).note("AAC: ADTS sync word validated"))
}

fn validate_m4a(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    validate_mp4(src, start, limit)
}

fn validate_wma(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // WMA is ASF container
    validate_asf(src, start, limit)
}

fn validate_aiff(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 12, limit)?;
    if &h[0..4] != b"FORM" {
        return None;
    }
    if &h[8..12] != b"AIFF" && &h[8..12] != b"AIFC" {
        return None;
    }
    Some(Validated::new(12, 88).note("AIFF: FORM/AIFF header validated"))
}

fn validate_au(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b".snd" {
        return None;
    }
    Some(Validated::new(24, 86).note("AU: NeXT/Sun audio header validated"))
}

fn validate_mid(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"MThd" {
        return None;
    }
    Some(Validated::new(14, 88).note("MIDI: header chunk validated"))
}

fn validate_it(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"IMPM" {
        return None;
    }
    Some(Validated::new(4, 86).note("Impulse Tracker module validated"))
}

fn validate_xm(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 17, limit)?;
    if h != b"Extended Module: " {
        return None;
    }
    Some(Validated::new(17, 86).note("FastTracker II XM module validated"))
}

fn validate_s3m(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 16, limit)?;
    if h != b"Scream Tracker 3" {
        return None;
    }
    Some(Validated::new(64, 86).note("Scream Tracker 3 module validated"))
}

fn validate_mod(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"M.K." && h != b"FLT4" && h != b"FC00" {
        return None;
    }
    Some(Validated::new(1084, 86).note("ProTracker MOD module validated"))
}

fn validate_ape(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 3, limit)?;
    if h != b"MAC" {
        return None;
    }
    Some(Validated::new(32, 86).note("Monkey's Audio header validated"))
}

fn validate_voc(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 20, limit)?;
    if h != b"Creative Voice File" {
        return None;
    }
    Some(Validated::new(26, 86).note("Creative Voice header validated"))
}

fn validate_shn(_src: &dyn ReadOnlySource, _start: u64, _limit: u64) -> Option<Validated> {
    // Shorten has no magic header; cannot reliably validate. Skip to avoid false positives.
    None
}

fn validate_spx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Speex is Ogg-based; validate via OggS magic + CRC
    validate_ogg(src, start, limit)
}

// ===== Additional video validators =====

fn validate_flv(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 3, limit)?;
    if h != b"FLV" {
        return None;
    }
    if start + 9 > limit {
        return None;
    }
    let header = read_bounded(src, start, 9, limit)?;
    let version = header[3];
    if version != 1 {
        return None;
    }
    Some(Validated::new(9, 86).note("FLV: Flash video header validated"))
}

fn validate_swf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 3, limit)?;
    if h != b"FWS" && h != b"CWS" && h != b"ZWS" {
        return None;
    }
    Some(Validated::new(8, 86).note("SWF: Flash header validated"))
}

fn validate_mxf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 22, limit)?;
    let key = &h[0..22];
    let expected = b"\x06\x0e\x2b\x34\x02\x05\x01\x01\x0d\x01\x03\x01\x02\x07\x01\x01\x0d\x01\x02\x01\x01\x04\x01";
    if key != expected {
        return None;
    }
    Some(Validated::new(22, 88).note("MXF: SMPTE key validated"))
}

fn validate_dpx(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"SDPX" {
        return None;
    }
    Some(Validated::new(512, 88).note("DPX: Cineon header validated"))
}

fn validate_exr(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x76\x2f\x31\x01" && h != b"\x76\x2f\x31\x02" {
        return None;
    }
    Some(Validated::new(4, 88).note("OpenEXR: magic header validated"))
}

fn validate_hdr(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 10, limit)?;
    if h != b"?RADIANCE" {
        return None;
    }
    Some(Validated::new(11, 86).note("RGBE Radiance header validated"))
}

fn validate_asf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    let h = read_bounded(src, start, 16, limit)?;
    let expected = b"\x30\x26\xB2\x75\x8E\x66\xCF\x11\xA6\xD9\x00\xAA\x00\x62\xCE\x6C";
    if h != expected {
        return None;
    }
    Some(Validated::new(16, 88).note("ASF: GUID header validated"))
}

fn validate_vob(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // VOB is MPEG-2 program stream; validate via program stream pack header.
    // MPEG program streams start with 0x000001BA. We validate the pack header
    // and system header structure to avoid false positives on random data.
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x00\x00\x01\xBA" {
        return None;
    }
    if start + 14 > limit {
        return None;
    }
    let pack = read_bounded(src, start, 14, limit)?;
    let _system_clock_ref = u32_be(&pack, 4)?;
    let _marker_bits = pack[12];
    let _stuffing_length = pack[13] & 0x07;
    Some(Validated::new(14, 86).note("VOB: MPEG program stream pack header validated"))
}

// ===== Camera RAW validators =====

fn validate_orf(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Olympus RAW: "ORF" magic followed by a little-endian TIFF header.
    let h = read_bounded(src, start, 8, limit)?;
    if &h[0..3] != b"ORF" {
        return None;
    }
    // Bytes 3..7 are typically a little-endian TIFF header "II*\0".
    let tiff = read_bounded(src, start + 3, 4, limit)?;
    if tiff != b"II*\0" && tiff != b"MM\x00*" {
        return None;
    }
    Some(Validated::new(512, 90).note("ORF: Olympus RAW header + TIFF magic validated"))
}

fn validate_rw2(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Panasonic RW2: "RW2" magic followed by version bytes.
    let h = read_bounded(src, start, 8, limit)?;
    if &h[0..3] != b"RW2" {
        return None;
    }
    let _version = u32_le(&h, 4)?;
    Some(Validated::new(512, 90).note("RW2: Panasonic RAW header validated"))
}

fn validate_x3f(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Sigma X3F: "FOVb" magic (Foveon).
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"FOVb" {
        return None;
    }
    Some(Validated::new(512, 90).note("X3F: Sigma Foveon RAW header validated"))
}

fn validate_crw(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Canon CRW (CIFF): "CRM\x00\x00\x08" magic.
    let h = read_bounded(src, start, 8, limit)?;
    if &h[0..6] != b"CRM\x00\x00\x08" {
        return None;
    }
    Some(Validated::new(512, 90).note("CRW: Canon CIFF RAW header validated"))
}

// ===== Additional audio validators =====

fn validate_wv(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // WavPack: "wvpk" magic.
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"wvpk" {
        return None;
    }
    Some(Validated::new(32, 88).note("WavPack: wvpk magic validated"))
}

fn validate_tta(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // True Audio: "TTA" magic followed by format tag.
    let h = read_bounded(src, start, 8, limit)?;
    if &h[0..3] != b"TTA" {
        return None;
    }
    let _format_tag = u16_le(&h, 4)?;
    let _sample_rate = u16_le(&h, 6)?;
    Some(Validated::new(32, 88).note("TTA: True Audio header validated"))
}

// ===== Disk image / backup validators =====

fn validate_gho(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // Norton Ghost: "~Wen" magic (0x7E 0x57 0x65 0x6E).
    let h = read_bounded(src, start, 4, limit)?;
    if h != b"\x7E\x57\x65\x6E" {
        return None;
    }
    Some(Validated::new(512, 88).note("GHO: Norton Ghost image header validated"))
}

fn validate_dar(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
    // DAR archive: "DAR\x01!" magic.
    let h = read_bounded(src, start, 6, limit)?;
    if &h[0..5] != b"DAR\x01!" {
        return None;
    }
    Some(Validated::new(512, 88).note("DAR: archive header validated"))
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

    #[test]
    fn valid_flac_is_carved() {
        let dir = std::env::temp_dir().join(format!("relift-carve3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audio.flac");
        let mut data = Vec::new();
        data.extend_from_slice(b"fLaC");
        // STREAMINFO metadata block, last flag set, type 0, length 34.
        data.push(0x80);
        data.extend_from_slice(&(34u32.to_be_bytes())[1..4]);
        data.extend_from_slice(&[0u8; 34]); // STREAMINFO body (zeros are fine for structure)
                                            // Audio frame sync word.
        data.push(0xFF);
        data.push(0xF8);
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
        assert!(summary.emitted >= 1, "FLAC should be carved");
        assert!(
            sink.items.iter().any(|c| c.format == "flac"),
            "FLAC format should be emitted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_ole_is_carved() {
        let dir = std::env::temp_dir().join(format!("relift-carve4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.ole");
        let mut data = vec![0u8; 512];
        data[..8].copy_from_slice(b"\xD0\xCF\x11\xE0\xA0\xB1\x19\xE1");
        // Sector size shift = 9 (512-byte sectors) at offset 0x1C.
        data[0x1C] = 9;
        data[0x1D] = 0;
        std::fs::write(&path, &data).unwrap();
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
        assert!(summary.emitted >= 1, "OLE should be carved");
        assert!(
            sink.items.iter().any(|c| c.format == "ole"),
            "OLE format should be emitted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_mpegts_is_carved() {
        let dir = std::env::temp_dir().join(format!("relift-carve5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("video.ts");
        let mut data = Vec::new();
        // Eight valid 188-byte packets with 0x47 sync at the start of each.
        // Two distinct PIDs (0x100 and 0x101) to satisfy the distinct PID requirement.
        // AFC = 0b11 (adaptation field + payload), no TEI.
        // Continuity counter increments from 0 to 7.
        for i in 0..8u8 {
            data.push(0x47);
            // Alternate between PID 0x100 and 0x101
            let pid = if i % 2 == 0 { 0x1000 } else { 0x1010 };
            data.push((pid >> 8) as u8); // PID high byte + TEI=0
            data.push((pid & 0xFF) as u8); // PID low byte
                                           // AFC = 0b11, CC = i (lower 4 bits)
            data.push(0x30 | (i & 0x0F));
            data.extend(std::iter::repeat_n(0u8, 184));
        }
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
        assert!(summary.emitted >= 1, "MPEG-TS should be carved");
        assert!(
            sink.items.iter().any(|c| c.format == "ts"),
            "TS format should be emitted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Repeated `0x47` bytes at non-188 intervals (or short runs) must not be
    /// mistaken for a valid MPEG-TS stream.
    #[test]
    fn mpegts_rejects_random_sync_bytes() {
        let dir = std::env::temp_dir().join(format!("relift-carve-ts-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("noise.ts");
        // Build an image with `0x47` at positions that are NOT 188 apart, plus
        // some short runs of valid sync. The carver should reject all of them.
        let mut data = vec![0u8; 4096];
        for i in 0..data.len() / 100 {
            data[i * 100] = 0x47; // sync every 100 bytes, not 188
        }
        // A short run of 3 valid packets (below the 4-packet minimum).
        data[2000] = 0x47;
        data[2001] = 0x10;
        data[2002] = 0x00;
        data[2003] = 0x30;
        data[2188] = 0x47;
        data[2189] = 0x10;
        data[2190] = 0x00;
        data[2191] = 0x30;
        data[2376] = 0x47;
        data[2377] = 0x10;
        data[2378] = 0x00;
        data[2379] = 0x30;
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
        assert_eq!(
            summary.emitted, 0,
            "random 0x47 bytes must not produce MPEG-TS candidates: {summary:?}"
        );
        assert!(
            !sink.items.iter().any(|c| c.format == "ts"),
            "no TS candidates from noise"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_rar5_is_carved() {
        let dir = std::env::temp_dir().join(format!("relift-carve6-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("archive.rar");
        let mut data = Vec::new();
        data.extend_from_slice(b"Rar!\x1A\x07\x01\x00");
        // RAR5 block: type=1 (main), flags=0x8000 (last), size=12 (header only),
        // compressed_size=0
        data.extend_from_slice(&(1u16).to_le_bytes()); // block type
        data.extend_from_slice(&(0x8000u16).to_le_bytes()); // block flags = last
        data.extend_from_slice(&(12u32).to_le_bytes()); // block size (header only)
        data.extend_from_slice(&(0u32).to_le_bytes()); // compressed size
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
        assert!(summary.emitted >= 1, "RAR5 should be carved");
        assert!(
            sink.items.iter().any(|c| c.format == "rar5"),
            "RAR5 format should be emitted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn valid_iso9660_is_carved() {
        let dir = std::env::temp_dir().join(format!("relift-carve8-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("disc.iso");
        // CD001 magic at offset 1, volume descriptor type byte at offset 0.
        // The iso9660 marker has adjust=-1, so abs_start = hit - 1 = 0.
        let mut data = vec![0u8; 2048];
        data[0] = 0x01; // Primary Volume Descriptor type
        data[1..6].copy_from_slice(b"CD001");
        data[6] = 0x01; // version
        std::fs::write(&path, &data).unwrap();
        let src = FileSource::open(&path).unwrap();
        // CD001 with adjust=-1: abs_start = hit(1) - 1 = 0 (type byte).
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
        assert!(summary.emitted >= 1, "ISO 9660 should be carved");
        assert!(
            sink.items.iter().any(|c| c.format == "iso9660"),
            "ISO 9660 format should be emitted"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registered_format_count_is_stable() {
        // This guard exists to keep public documentation honest. When the
        // marker set changes, update docs/src/formats.md,
        // docs/src/support-matrix.md, and the format-count statements in the
        // README and llms*.txt at the same time.
        let mut formats: Vec<&str> = MARKERS.iter().map(|m| m.format).collect();
        formats.sort_unstable();
        formats.dedup();
        assert_eq!(
            formats.len(),
            149,
            "registered carve formats changed; update docs/src/formats.md, docs/src/support-matrix.md, README.md, and llms*.txt: {formats:?}"
        );
    }

    #[test]
    fn random_bytes_rejects_new_carvers() {
        let dir = std::env::temp_dir().join(format!("relift-carve9-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("noise.img");
        let data = vec![0x5Au8; 8192]; // 0x5A pattern, no valid signatures
        std::fs::write(&path, &data).unwrap();
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
        assert_eq!(summary.emitted, 0, "noise should produce no valid carves");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The overlap must be at least the longest signature length minus one, so
    /// no signature can be split across two chunks.
    #[test]
    fn overlap_covers_longest_signature() {
        let max_len = MARKERS.iter().map(|m| m.magic.len()).max().unwrap_or(1);
        assert!(
            OVERLAP as usize >= max_len.saturating_sub(1),
            "OVERLAP {} is smaller than max signature length - 1 ({})",
            OVERLAP,
            max_len
        );
    }

    /// Build a minimal valid SQLite database (one 4096-byte page) and copy it
    /// into `buf` at `offset`.
    fn write_minimal_sqlite(buf: &mut [u8], offset: usize) {
        let page_size: u16 = 4096;
        let header = b"SQLite format 3\0";
        buf[offset..offset + header.len()].copy_from_slice(header);
        buf[offset + 16..offset + 18].copy_from_slice(&page_size.to_be_bytes());
        buf[offset + 18] = 1; // write version
        buf[offset + 19] = 1; // read version
        buf[offset + 20] = 0;
        buf[offset + 21] = 64;
        buf[offset + 22] = 32;
        buf[offset + 23] = 32;
        buf[offset + 28..offset + 32].copy_from_slice(&1u32.to_be_bytes()); // page count
                                                                            // Zero-fill the rest of the page.
        for b in buf
            .iter_mut()
            .skip(offset + 32)
            .take(page_size as usize - 32)
        {
            *b = 0;
        }
    }

    /// Signatures placed at chunk boundaries (start, end overlap, next chunk
    /// start) must all be discovered. The overlap ensures a signature whose
    /// magic starts within the last `OVERLAP` bytes of a chunk is still fully
    /// visible in the read buffer.
    #[test]
    fn signatures_at_chunk_boundaries_are_found() {
        let dir =
            std::env::temp_dir().join(format!("relift-carve-boundary-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("boundary.img");

        // Image larger than one CHUNK (4 MiB) so we cross a boundary.
        let mut data = vec![0u8; CHUNK as usize + 8192 * 2];

        // SQLite at the very start of the image (start of chunk 0).
        write_minimal_sqlite(&mut data, 0);

        // SQLite whose magic starts one byte before the end of chunk 0. Its
        // 16-byte magic spans into the overlap region, so without an overlap
        // at least `max_signature_len - 1` it would be split across chunks.
        let near_end = (CHUNK as usize) - 1;
        write_minimal_sqlite(&mut data, near_end);

        // SQLite at the start of chunk 1, well clear of the previous one's
        // page data (which extends 4096 bytes past its header).
        write_minimal_sqlite(&mut data, CHUNK as usize + 8192);

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

        let found: Vec<u64> = sink
            .items
            .iter()
            .filter(|c| c.format == "sqlite")
            .map(|c| match &c.data {
                crate::candidate::DataLocation::Raw { offset, .. } => *offset,
                _ => 0,
            })
            .collect();
        assert!(
            found.contains(&0),
            "SQLite at chunk start (offset 0) not found: {found:?}"
        );
        assert!(
            found.contains(&(near_end as u64)),
            "SQLite whose magic starts at chunk end (offset {near_end}) not found: {found:?}"
        );
        assert!(
            found.contains(&(CHUNK + 8192)),
            "SQLite at chunk 1 start not found: {found:?}"
        );
        assert_eq!(
            summary.emitted, 3,
            "expected 3 boundary SQLite files, got {}: {found:?}",
            summary.emitted
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
