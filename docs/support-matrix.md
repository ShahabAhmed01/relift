# Support Matrix

## Filesystem support

| Filesystem | Metadata undelete | Carving | Status | Notes |
|------------|-------------------|---------|--------|-------|
| FAT12 | ✓ | ✓ | Tested | Fixture-validated |
| FAT16 | ✓ | ✓ | Tested | Fixture-validated |
| FAT32 | ✓ | ✓ | Tested | Fixture-validated, LFN support |
| exFAT | ✓ | ✓ | Tested | Fixture-validated, allocation bitmap |
| NTFS | ✓ | ✓ | Tested | $MFT, resident/non-resident, sparse, $I30 slack |
| ext2 | ✓ | ✓ | Tested | Inode + block pointers |
| ext3 | ✓ | ✓ | Tested | Journal-assisted |
| ext4 | ✓ | ✓ | Tested | Extents, indirect, sparse, unwritten, orphans |
| HFS+ | ✓ | ✓ | Experimental | Catalog B-tree, extents overflow |
| APFS | ✓ | ✓ | Experimental | Container/volume, snapshots, B-tree |
| Btrfs | ✓ | ✓ | Experimental | Root tree, subvolumes, snapshots |
| XFS | ✓ | ✓ | Experimental | AG headers, inode B+tree |
| ReFS | ✗ | ✓ | Identified only | Metadata recovery not implemented |
| F2FS | ✗ | ✓ | Identified only | Metadata recovery not implemented |
| ISO 9660 | ✗ | ✓ | Identified only | Optical media |
| UDF | ✗ | ✓ | Identified only | Optical media |
| ZFS | ✗ | ✓ | Identified only | Not implemented |
| ReiserFS | ✗ | ✓ | Identified only | Not implemented |

**Legend:**
- ✓ = Implemented and tested (has fixture + e2e test)
- Experimental = Implemented but limited real-media validation
- Identified only = Detected by `probe()`, metadata recovery not implemented
- ✗ = Not supported for metadata recovery

## Platform support

| Platform | Raw device access | GUI | CLI | Imaging | Notes |
|----------|-------------------|-----|-----|---------|-------|
| Windows 10/11 x64 | ✓ (admin) | ✓ (WebView2) | ✓ | ✓ | Requires elevation for physical drives |
| Windows ARM64 | ✓ (admin) | ✓ | ✓ | ✓ | Untested |
| macOS 12+ (Intel) | ✓ (root/FDA) | ✓ | ✓ | ✓ | Full Disk Access required |
| macOS 12+ (Apple Silicon) | ✓ (root/FDA) | ✓ | ✓ | ✓ | Universal binary |
| Linux (glibc 2.31+) | ✓ (root) | ✓ | ✓ | ✓ | io_uring on kernel 5.1+ |
| Linux ARM64 | ✓ (root) | ✓ | ✓ | ✓ | Tested on Raspberry Pi 4 |
| Linux musl | Static binary | ✓ | ✓ | ✓ | Alpine, containers |

**Raw device access notes:**
- Windows: `CreateFile` with `GENERIC_READ | FILE_SHARE_READ | FILE_SHARE_WRITE`, `FILE_FLAG_OVERLAPPED`
- Linux: `open("/dev/sdX", O_RDONLY | O_DIRECT)` with fallback
- macOS: `open("/dev/rdiskX", O_RDONLY)` with Full Disk Access

## Image format support

| Format | Read | Write | Notes |
|--------|------|-------|-------|
| Raw / .img / .dd / .001 | ✓ | ✓ | Split images supported |
| E01 / Ex01 (EnCase) | ✓ | Planned | Via ewf-rs fork, CRC per chunk |
| AFF4 | Planned | Planned | |
| VHD / VHDX | ✓ | Planned | Fixed/dynamic/differencing |
| VMDK | ✓ | Planned | Monolithic/split/sparse |
| QCOW2 | ✓ | Planned | Backing chain support |
| DMG / UDIF | ✓ | Planned | Read-only |
| ISO 9660 / UDF | ✓ | ✗ | Optical images |

## Carved format support (validated)

| Category | Formats | Validation |
|----------|---------|------------|
| Images | JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF | Full structural (CRC, Huffman, IFD, box tree) |
| Documents | PDF, DOCX, XLSX, PPTX, ODT, ODS, ODP, RTF, EPUB | Container + content-type check |
| Archives | ZIP, 7z, RAR4, gzip, bzip2, xz, zstd, tar | Central directory / CRC / block validation |
| Video | MP4, MOV, MKV, AVI, WebM, FLV, MPEG-TS, 3GP | Box tree / RIFF / EBML validation |
| Audio | MP3, WAV, FLAC, OGG, AAC, M4A, AIFF | Frame sync / CRC / header validation |
| Databases | SQLite, MDB, ACCDB | Page header / freelist validation |
| System | PST/OST, LNK, EVTX, Registry hives | Header / block validation |
| Executables | PE (exe/dll), ELF | Header / section validation |

**Not yet validated (carved but not emitted):**
- FLAC (no structural validator)
- RAR5 (no validator)
- ZIP64 (no validator)
- OLE compound files (DOC/XLS/PPT legacy)
- ISO 9660 filesystem carving

## Feature matrix

| Feature | CLI | GUI | Notes |
|---------|-----|-----|-------|
| Device enumeration | ✓ | ✓ | `relift devices` |
| Quick scan (metadata) | ✓ | ✓ | `--mode quick` |
| Deep scan (carve) | ✓ | ✓ | `--mode deep` |
| Formatted partition scan | ✓ | ✓ | `--mode formatted` |
| Lost partition scan | ✓ | ✓ | `--mode lost-partition` |
| Pause/resume scan | ✓ | ✓ | Session-based |
| Live file listing | ✓ | ✓ | `--live` |
| Filter by extension | ✓ | ✓ | `--filter ext:jpg,png` |
| Filter by size | ✓ | ✓ | `--filter size:1KB-100MB` |
| Filter by date | ✓ | ✓ | `--filter date:2024-01-01..2024-12-31` |
| Filter by confidence | ✓ | ✓ | `--min-confidence verified` |
| Filter by path | ✓ | ✓ | `--filter path:*/Pictures/*` |
| Preview (images/text) | ✗ | ✓ | Right pane |
| Preview (hex) | ✗ | ✓ | Right pane |
| SHA-256 verification | ✓ | ✓ | Default on |
| Destination safety | ✓ | ✓ | Blocks same physical device |
| Disk imaging | ✓ | ✓ | `relift image` |
| ddrescue map file | ✓ | ✓ | Compatible format |
| JSON output | ✓ | ✗ | `--json` |
| CSV/HTML/MD report | ✓ | ✓ | `relift list` / GUI export |
| Shell completions | ✓ | ✗ | `relift completions` |
| Man page | ✓ | ✗ | `man relift` |
| Self-tests (doctor) | ✓ | ✓ | `relift doctor` |

## Known limitations

See `docs/GAP_REPORT.md` and `docs/limitations.md` for detailed gap tracking.