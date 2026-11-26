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
| Windows x64 (MSVC) | Built locally and in CI | Synthetic fixtures run locally and in CI | Raw devices require elevation |
| Windows ARM64 | Untested | Untested | No prebuilt artifact yet |
| macOS CI runner | Built and tested in CI | Synthetic fixtures run in CI; no raw-device validation | Raw-device destination safety fails closed |
| Linux x86_64 | Built and tested in CI | Synthetic fixtures run in CI; no physical-device validation | Raw devices may require root |
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
| Video | MP4/ISO-BMFF, MKV |
| Audio | MP3, WAV/RIFF, Ogg |
| Databases | SQLite |
| System | PST/OST, LNK, EVTX |
| Executables | PE (EXE/DLL), ELF |

**Not emitted:** FLAC (the current parser did not establish audio-frame
boundaries), RAR5, ZIP64, OLE compound files (legacy DOC/XLS/PPT), MPEG-TS,
ISO 9660, and formats without a registered validator. MOV and WebM are not
separately classified from their container families.

## Feature matrix

| Feature | CLI | GUI | Notes |
|---------|-----|-----|-------|
| Device enumeration | ✓ | ✓ | `relift devices` |
| Quick scan (metadata) | ✓ | ✓ | `--mode quick` |
| Deep scan (carve) | ✓ | ✓ | `--mode deep` / `--carve` |
| Keep partial results after stopping a scan | ✓ | ✓ | Scan cancellation preserves candidates already written to the session |
| Resume an interrupted scan from a checkpoint | ✓ | ✗ | CLI `--resume`; volume-level checkpoint. Completed volumes are skipped and the interrupted volume is rescanned from its start. Source size/fingerprint must match |
| Live file listing | ✓ | ✓ | `--live` |
| Filter by extension/confidence | ✓ | ✓ | Filter expressions |
| Preview (text/hex/metadata) | ✗ | ✓ | Results pane |
| SHA-256 verification | ✓ | ✓ | Default on |
| Destination safety | ✓ | ✓ | Blocks the same physical device |
| Disk imaging | ✓ | ✗ | CLI only |
| Imaging map file and multi-pass resume | ✗ | ✗ | Not implemented; `relift image` performs a single copy pass |
| JSON output | ✓ | ✗ | `--json` |
| JSON/CSV/HTML/Markdown reports | ✓ | ✗ | `relift report` |
| Shell completions | ✓ | ✗ | `relift completions` |
| Self-tests (`doctor`) | ✓ | ✓ | Permission and device checks |

## Known limitations

See `docs/GAP_REPORT.md` and `docs/limitations.md` for the full list.
