# Carved Format Reference

## Overview

Relift's carving engine uses **structural validation** — not just magic bytes. Every candidate must pass a format-specific validator that walks the container, verifies checksums, and checks internal consistency.

## Validation policy

| Level | Description | Emitted? |
|-------|-------------|----------|
| **Verified** | Full structural validation + checksums pass | ✓ |
| **Likely** | Structure valid, minor gaps | ✓ |
| **Partial** | Truncated, fragmented, or incomplete | ✓ (marked) |
| **Unverified** | Signature only, no validator | ✗ (never) |

Unvalidated hits are **never emitted** as recovered files.

## Supported formats (validated)

### Images

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| JPEG | `.jpg`, `.jpeg` | `FF D8 FF` | SOI → segments → SOF → Huffman → EOI |
| PNG | `.png` | `89 50 4E 47 0D 0A 1A 0A` | IHDR → IDAT (CRC32) → IEND |
| GIF | `.gif` | `47 49 46 38 37 61` / `39 61` | Header → blocks → trailer (0x3B) |
| BMP | `.bmp` | `42 4D` | File size + pixel offset consistency |
| TIFF | `.tiff`, `.tif` | `49 49 2A 00` / `4D 4D 00 2A` | IFD chain + strip/tile offsets |
| WebP | `.webp` | `RIFF` + `WEBP` | VP8/VP8L/VP8X chunk validation |
| HEIC | `.heic` | `ftyp` + `heic`/`mif1` | ISO-BMFF box tree (like MP4) |
| AVIF | `.avif` | `ftyp` + `avif` | ISO-BMFF box tree |
| Camera RAW | `.cr2`, `.nef`, `.arw`, `.orf`, `.raf`, `.rw2`, `.dng` | TIFF-based | IFD + maker note validation |

### Documents

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| PDF | `.pdf` | `%PDF-` | Header → xref/trailer → `%%EOF` |
| DOCX | `.docx` | `PK 03 04` | ZIP + `[Content_Types].xml` + `word/` |
| XLSX | `.xlsx` | `PK 03 04` | ZIP + `[Content_Types].xml` + `xl/` |
| PPTX | `.pptx` | `PK 03 04` | ZIP + `[Content_Types].xml` + `ppt/` |
| ODT/ODS/ODP | `.odt`, `.ods`, `.odp` | `PK 03 04` | ZIP + `mimetype` |
| RTF | `.rtf` | `{\rtf` | Brace matching + control words |
| EPUB | `.epub` | `PK 03 04` | ZIP + `mimetype` + `META-INF/container.xml` |
| Legacy Office | `.doc`, `.xls`, `.ppt` | `D0 CF 11 E0` | OLE/CFB (not validated — not emitted) |

### Archives

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| ZIP | `.zip` | `PK 03 04` | Local headers → Central Directory → EOCD (CRC32) |
| 7z | `.7z` | `37 7A BC AF 27 1C` | Start header CRC → next header CRC |
| RAR4 | `.rar` | `52 61 72 21 1A 07 00` | Block chain → end block |
| gzip | `.gz` | `1F 8B 08` | Deflate stream + CRC32 + ISIZE |
| bzip2 | `.bz2` | `42 5A 68` | Header only (no end marker) |
| xz | `.xz` | `FD 37 7A 58 5A 00` | Stream CRC32 + blocks |
| zstd | `.zst` | `28 B5 2F FD` | Frame header + content checksum |
| TAR | `.tar` | `ustar` at offset 257 | Header checksum + block alignment |

### Video

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| MP4 | `.mp4` | `ftyp` | `ftyp` → `moov`/`moof` → `mdat` box tree |
| MOV | `.mov` | `ftyp` | Same as MP4 |
| MKV/WebM | `.mkv`, `.webm` | `1A 45 DF A3` | EBML IDs + segment structure |
| AVI | `.avi` | `RIFF` + `AVI ` | RIFF chunks + `idx1` if present |
| FLV | `.flv` | `46 4C 56` | Header + tags |
| MPEG-TS | `.ts`, `.m2ts` | `47` (sync byte) | Packet sync + PAT/PMT |

### Audio

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| MP3 | `.mp3` | `FF FB` / `FF FA` / `FF F3` / `FF F2` / `FF E3` | 3+ consecutive valid frames + Xing/VBRI |
| WAV | `.wav` | `RIFF` + `WAVE` | RIFF → `fmt ` + `data` chunks |
| FLAC | `.flac` | `fLaC` | Streaminfo + frame CRC16 |
| OGG | `.ogg` | `OggS` | Page CRC32 + serial + sequence + EOS |
| AAC | `.aac` | `FF F1` / `FF F9` | ADTS frame sync chain |
| AIFF | `.aiff` | `FORM` + `AIFF` | RIFF-like chunks |

### Databases

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| SQLite | `.sqlite`, `.db` | `53 51 4C 69 74 65 20 66 6F 72 6D 61 74 20 33 00` | Header → page size → page count → freelist |
| Access MDB | `.mdb` | `00 01 00 00 53 74 61 6E 64 61 72 64 20 4A 65 74 20 44 42` | Header signature |
| Access ACCDB | `.accdb` | `00 01 00 00 53 74 61 6E 64 61 72 64 20 41 43 45 20 44 42` | Header signature |

### System / Forensic

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| Windows Registry | `.reg` | `72 65 67 66` | `regf` header + hbin blocks |
| Event Log (EVTX) | `.evtx` | `45 6C 66 46 69 6C 65` | `ElfFile` header + chunk CRC |
| LNK (shortcut) | `.lnk` | `4C 00 00 00 01 14 02 00 00 00 00 00 C0 00 00 00 00 00 00 46` | ShellLink header + CLSID |
| PST/OST | `.pst`, `.ost` | `21 42 44 4E` / `4E 41 56 4D 45 4C 4F 4E 47` | Header + B-tree |
| Windows Prefetch | `.pf` | `53 43 43 41` | `SCCA` header |
| Hibernation file | `hiberfil.sys` | `48 49 42 52` | `HIBR` + Xpress compression |

### Executables

| Format | Extension | Magic | Validator |
|--------|-----------|-------|-----------|
| PE (Windows) | `.exe`, `.dll`, `.sys` | `4D 5A` (MZ) → `50 45 00 00` | DOS header → PE header → sections |
| ELF (Linux) | (none) | `7F 45 4C 46` | ELF header → program/section headers |

## Validator details

### JPEG
1. Verify SOI (`FF D8`)
2. Walk segments: length-prefixed, skip `FF 00` stuffing
3. Require SOF (baseline/progressive) with valid dimensions
4. Scan entropy-coded data for next marker (handles restarts `FF D0-D7`)
5. Require EOI (`FF D9`)
6. **Score**: 92 if complete, 0 if truncated

### PNG
1. Verify 8-byte signature
2. Walk chunks: length + type + data + CRC32
3. Require IHDR first (13 bytes, valid dimensions)
4. Require at least one IDAT
5. Require IEND (length 0, valid CRC)
6. **Score**: 96 if complete

### ZIP
1. Walk local file headers (`PK 03 04`)
2. Verify flags (no data descriptor, no encryption unless noted)
3. Walk central directory (`PK 01 02`)
4. Verify EOCD (`PK 05 06`): disk numbers, entry counts, offsets match
7. **Score**: 94 if full CD validated, 88 if empty ZIP

### MP4/MOV
1. Verify `ftyp` box first
2. Walk box tree: `moov`, `moof`, `mdat`, etc.
3. Validate `moov` structure (track headers, sample tables)
4. Verify `mdat` size consistency
5. **Score**: 92 if `ftyp`+`moov`/`moof`+`mdat` present

### SQLite
1. Verify 16-byte header
2. Page size power of 2 (512-65536)
2. Valid write/read version (1 or 2)
3. Page count > 0
4. Size = page_size × page_count within bounds
5. **Score**: 96

## Format-specific notes

### Fragmented file handling
- **JPEG**: Restart markers allow gap detection; embedded thumbnails excluded
- **MP4**: Fragmented MP4 (`moof`/`mdat` chains) not reassembled without reference file
- **ZIP**: Each entry carved independently; central directory ties them together
- **TAR**: Sequential headers; two zero blocks = end

### Encrypted/compressed payloads
- **ZIP encryption**: Detected via flag; bytes carved, not decrypted
- **RAR encryption**: Not validated
- **PDF encryption**: Detected via `/Encrypt` in trailer; not decrypted
- **7z encryption**: Header only; payload not validated

### Camera RAW
All TIFF-based (CR2, NEF, ARW, ORF, RAF, RW2, DNG):
- IFD chain validation
- Strip/tile offsets within bounds
- MakerNote not parsed (proprietary)

## Adding a new format

See `CONTRIBUTING.md` → "Adding a new file carver".

Required:
1. Validator function returning `Option<Validated>`
2. Entry in `MARKERS` array
3. `MAX_<FORMAT>` constant
4. Match arms in `marker_max()` and `validate()`
5. Unit test with valid + truncated + corrupted fixtures
6. Fuzz target in `fuzz/`