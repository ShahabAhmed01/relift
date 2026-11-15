# Support Matrix

This matrix separates what is implemented and fixture-tested from what is only
identified or experimental. It is intentionally conservative: a checkmark
means there is code plus at least a deterministic fixture or end-to-end test.

## Filesystem support

| Filesystem | Metadata undelete | Status | Notes |
|------------|-------------------|--------|-------|
| FAT12/16/32 | ✓ | Fixture-validated | Directory entries, LFN, cluster chains |
| exFAT | ✓ | Fixture-validated | Entry sets, allocation bitmap, NoFatChain |
| NTFS | ✓ | Fixture-validated | $MFT records, resident/non-resident data, sparse runs |
| ext2/3/4 | ✓ | Fixture-validated | Inodes, direct/indirect blocks, extent trees, sparse/unwritten runs |
| HFS+ | ✓ | Experimental | Catalog B-tree and extent descriptors; no fixture yet |
| APFS | ✓ | Experimental | Container/volume headers and B-tree inode scan; no fixture yet |
| Btrfs | ✓ | Experimental | Root/chunk tree probing and inode items; no fixture yet |
| XFS | ✓ | Experimental | AG headers and inode B+tree scan; no fixture yet |
| ReFS, F2FS, ZFS, ReiserFS, ISO 9660, UDF | ✗ | Identified only | Detected by `probe()`; metadata recovery not implemented |

**Legend**

- **Fixture-validated** — exercised by the deterministic testkit and end-to-end
  scan → recover → SHA-256 tests.
- **Experimental** — parser exists and is wired into scan, but has not been
  validated against a fixture or real volume.
- **Identified only** — detected and reported; no metadata recovery.

## Platform support

| Platform | Build status | Runtime status | Notes |
|----------|--------------|----------------|-------|
| Windows 10/11 x64 (MSVC) | Built and tested locally | Exercised on synthetic fixtures | Raw devices require elevation |
| Windows ARM64 | Untested | Untested | No prebuilt artifact yet |
| macOS Intel / Apple Silicon | CI build target | Not exercised | Raw-device destination safety fails closed |
| Linux x86_64 | CI build target | Not exercised locally | Raw devices may require root |
| Linux ARM64 | Cross-build target | Not exercised | CLI archive built by the release workflow |
| Linux musl | Not built | Not built | Planned |

Raw-device access is implemented with a read-only handle on each platform:

- **Windows:** `CreateFileW` with `GENERIC_READ` only.
- **Linux:** `open(..., O_RDONLY)`.
- **macOS:** `open(..., O_RDONLY)`; destination checks fail closed.

There is no io_uring, `O_DIRECT`, or OS-specific direct-I/O path.

## Image format support

| Format | Read | Write | Notes |
|--------|------|-------|-------|
| Raw image (`.img`, `.dd`, `.001`, any block file) | ✓ | ✓ | Imaging writes a raw image plus SHA-256 manifest |
| E01/Ex01, AFF4, VHD/VHDX, VMDK, QCOW2, DMG/UDIF | ✗ | ✗ | Not implemented; convert to raw first |

## Carved format support

Every emitted carve must pass a structural validator. The current validators are:

| Category | Formats |
|----------|---------|
| Images | JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF |
| Documents | PDF, DOCX, XLSX, PPTX |
| Archives | ZIP, 7z, RAR4, gzip, bzip2, xz, zstd, tar |
| Video | MP4/MOV, MKV (WebM) |
| Audio | MP3, WAV/RIFF, FLAC, Ogg |
| Databases | SQLite |
| System | PST/OST, LNK, EVTX |
| Executables | PE (EXE/DLL), ELF |

**Not implemented as validated carvers:** RAR5, ZIP64, OLE compound files
(legacy DOC/XLS/PPT), MPEG-TS, ISO 9660.

## Feature matrix

| Feature | CLI | GUI | Notes |
|---------|-----|-----|-------|
| Device enumeration | ✓ | ✓ | `relift devices` |
| Quick scan (metadata) | ✓ | ✓ | `--mode quick` |
| Deep scan (carve) | ✓ | ✓ | `--mode deep` / `--carve` |
| Pause/resume scan | ✓ | Partial | Session-based; GUI exposes stop |
| Live file listing | ✓ | ✓ | `--live` |
| Filter by extension/confidence | ✓ | ✓ | Filter expressions |
| Preview (text/hex/metadata) | ✗ | ✓ | Results pane |
| SHA-256 verification | ✓ | ✓ | Default on |
| Destination safety | ✓ | ✓ | Blocks the same physical device |
| Disk imaging | ✓ | ✗ | CLI only |
| ddrescue-style map/resume | ✓ | ✗ | CLI only |
| JSON output | ✓ | ✗ | `--json` |
| JSON/CSV/HTML/Markdown reports | ✓ | ✗ | `relift report` |
| Shell completions | ✓ | ✗ | `relift completions` |
| Self-tests (`doctor`) | ✓ | ✓ | Permission and device checks |

## Known limitations

See `docs/GAP_REPORT.md` and `docs/limitations.md` for the full list.
