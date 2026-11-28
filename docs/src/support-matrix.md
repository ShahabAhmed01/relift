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
| HFS+ | ✗ | Experimental (probe-tested) | Detected by probe; deleted-file metadata recovery is not implemented |
| APFS | ✗ | Experimental (probe-tested) | Container/volume detection fixture exists; deleted-file metadata recovery is not implemented |
| Btrfs | ✗ | Experimental (probe-tested) | Detected by probe; deleted-file metadata recovery is not implemented |
| XFS | ✗ | Experimental (probe-tested) | Detected by probe; deleted-file metadata recovery is not implemented |
| ReFS, F2FS, ZFS, ReiserFS, ISO 9660, UDF | ✗ | Identified only | Detected by `probe()`; metadata recovery not implemented |

**Legend**

- **Fixture-validated** — exercised by the deterministic testkit and end-to-end
  scan → recover → SHA-256 tests.
- **Experimental (probe-tested)** — parser exists and is wired into scan; probe
  detection has a deterministic fixture, but metadata-based deleted-file
  recovery has not been validated against a fixture or real volume.
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
| E01/Ex01, AFF4, VHD/VHDX, VMDK, QCOW2, DMG/UDIF | Partial inspection only | ✗ | Some CLI metadata readers exist, but scanning is not integrated and some readers are incomplete; convert to raw first |

## Carved format support

Every emitted carve must pass a structural validator. The registry contains
153 format identifiers, including marker-only entries that cannot emit
candidates yet. This count is not the number of equally mature, fixture-tested,
or end-to-end-validated formats.

| Category | Formats |
|----------|---------|
| Images | JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF, TGA, PCX, PBM, PGM, PPM, ICO, ICNS, CUR, EMF, WMF, PSD, XCF, CDR, AI, EPS, PS, EXR, HDR, ORF, RW2, X3F, CRW, CR3, RAF, MRW, DDS |
| Documents | PDF, DOCX, XLSX, PPTX, DOC, XLS, PPT, PUB, WPD, WPS, XLR, RTF, TEX, LYX, FB2, PAGES, NUMBERS, KEY |
| Archives and containers | ZIP, 7z, RAR4, RAR5, gzip, bzip2, xz, zstd, TAR, ISO 9660, CAB, ACE, ARJ, LZH, LZO, PAR2, RPM, XAR, DEB, BKF, ARC, ZOO, WIM, APK |
| Video | MP4/ISO-BMFF, MKV, MPEG-TS, FLV, SWF, MXF, DPX, VOB, ASF |
| Audio | MP3, WAV/RIFF, Ogg, FLAC, AMR, AAC, M4A, WMA, AIFF, AU, MID, IT, XM, S3M, MOD, APE, VOC, SPX, WV, TTA |
| Databases | SQLite, MDB, ACCDB |
| System | PST/OST, LNK, EVTX, DBX, MSG, MBX, EML, NSF |
| Executables | PE (EXE/DLL/SYS), ELF, Mach-O, Java Class, DEX, COM |
| Fonts | TTF, OTF, WOFF, WOFF2, PFB |
| Disk / forensic | E01 (EnCase), GHO, DAR |
| Apple | Plist (binary and XML), AppleSingle |
| Game / ROM | NES |

**Not emitted:** Shn (Shorten), NSF (Lotus Notes), EML, VFD, ROM, N64, GBA, SFC,
BIN have no working validator in this build and are not emitted despite having
markers. ZIP64 (shares the PK\\x03\\x04 local-header magic with ZIP; handled by
the existing ZIP validator), and formats without a registered validator. MOV is
classified as MP4/ISO-BMFF; WebM is classified as MKV; AVI is classified as RIFF.

The `registered_format_count_is_stable` unit test fails when the marker set
changes, so this table cannot drift silently.

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
| Lost-partition signature scan | ✓ | ✗ | `relift partitions-scan`; estimated candidates for review, no table repair |
| Imaging map file and multi-pass resume | ✗ | ✗ | Not implemented; `relift image` performs a single copy pass |
| TOML config defaults | ✓ | ✗ | `--config` applies to scan, list, and recover; CLI values override config |
| JSON output | ✓ | ✗ | `--json` |
| JSON/CSV/HTML reports | ✓ | ✗ | `relift report` |
| Shell completions | ✓ | ✗ | `relift completions` |
| Self-tests (`doctor`) | ✓ | ✓ | Permission and device checks |
| `relift info` (source/session inspection) | ✓ | ✗ | `--json` supported |
| `relift verify` (SHA-256 audit of recovered files) | ✓ | ✗ | Re-reads outputs against stored hashes |
| `relift session` (session inspection) | ✓ | ✗ | Full session header and summary |
| `relift self-update` (update status) | ✓ | ✗ | Offline-by-default; `--check` only |
| `relift mcp` (MCP server status) | ✓ | ✗ | Planned; offline-by-default |

## Known limitations

See `docs/GAP_REPORT.md` and `docs/limitations.md` for the full list.



