# Relift Implementation Inventory

## Overview

This inventory replaces earlier unsupported claims of parity and dominance. Relift remains an early preview; use the [support matrix](docs/src/support-matrix.md), [verification record](docs/src/VERIFICATION.md), and [competitive analysis](docs/research/COMPETITIVE_ANALYSIS.md) for current evidence and scope.

## Phase 1: Partition and image workflows - PARTIAL

### 1.1 Partition Recovery & Lost Partition Detection
**Status: LOST-PARTITION SIGNATURE SCAN IMPLEMENTED; REPAIR NOT IMPLEMENTED**

New module: `crates/relift-core/src/partitions.rs`

Features implemented:
- **MBR parsing**: Primary partitions, extended partitions, logical drives
- **GPT parsing**: Protective MBR, primary GPT header, partition entries, backup GPT detection
- **APM parsing**: Apple Partition Map support
- **Filesystem signature scanning**: Scans entire disk for NTFS, exFAT, FAT32/16/12, ext2/3/4, HFS+, APFS, Btrfs, XFS, ReFS, ISO 9660 signatures
- **Partition size estimation**: Reads filesystem-specific size fields (NTFS $Boot, exFAT boot sector, FAT BPB, ext superblock)
- **Partition layout**: The library can assemble a virtual inspection layout; it does not repair or write a partition table

New CLI commands:
- `relift partitions <source> [--probe]` - Show partition table with filesystem probing
- `relift partitions-scan <source> [--max-bytes] [--session <dir>]` - Scan for lost partitions

### 1.2 Virtual Disk Support
**Status: PARTIAL INSPECTION ONLY; NOT INTEGRATED WITH SCANNING**

New module: `crates/relift-core/src/virtual_disk.rs`

Formats with reader/inspection code (coverage is incomplete and not equivalent to scan support):
- **VHD** (Hyper-V): Fixed and dynamic disks, Block Allocation Table (BAT) support
- **VHDX** (Hyper-V v2): Region tables, BAT, metadata regions, logical/physical sector sizes
- **VMDK** (VMware): Monolithic flat, monolithic sparse, split sparse, grain directories/tables
- **VDI** (VirtualBox): Dynamic, fixed, differencing disks, block maps
- **Raw disk images**: Direct passthrough

Not implemented:
- QCOW/QCOW2 (QEMU) - structure defined, parsing incomplete
- DMG (macOS) - placeholder
- Parallels HDD/HDS - placeholder

New CLI commands:
- `relift virtual-disk <source> [--verbose]` - Inspect virtual disk metadata
- `relift forensic-image <source> [--verbose]` - Inspect forensic images (placeholder)

### 1.3 Forensic Image Formats
**Status: NOT IMPLEMENTED**

The extension dispatcher exists, but currently returns `NotImplemented` for:
- E01/Ex01 (EnCase)
- AFF/AFF4
- Full implementation pending

## Phase 2: Filesystem Completeness - IN PROGRESS

### 2.1 Core Filesystems (Already Complete)
- FAT12/16/32 ✅
- exFAT ✅
- NTFS ✅
- ext2/3/4 ✅

### 2.2 Experimental Filesystems (Parsers Exist, Metadata Recovery Incomplete)
- HFS+ / APFS - Parser exists, metadata recovery experimental
- Btrfs / XFS - Parser exists, metadata recovery experimental
- ReFS / F2FS / ZFS / ReiserFS - Detection only

## Phase 3: Carving & Signature Excellence - EXISTING (Strong Foundation)

### 3.1 Signature registry and validators
The registry contains 153 identifiers, including marker-only entries. Only candidates accepted by implemented validators can be emitted. The tested validator set is listed in the [verification record](docs/src/VERIFICATION.md); registered identifiers are not a count of equally validated formats.

Examples of covered file families include:
- Images: JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF
- Documents: PDF, DOCX, XLSX, PPTX
- Archives: ZIP, 7z, RAR4, RAR5, gzip, bzip2, xz, zstd, TAR, ISO 9660
- Video: MP4/ISO-BMFF, MKV, MPEG-TS
- Audio: MP3, WAV/RIFF, Ogg, FLAC
- Databases: SQLite
- System: PST/OST, LNK, EVTX, OLE
- Executables: PE, ELF

### 3.2 ZIP64 Support
- Handled by existing ZIP validator (shared PK\x03\x04 magic)
- Dedicated ZIP64 validator planned

## Phase 4: Advanced Features - PLANNED

### 4.1 Search Index Integration (Unique Differentiator)
**Planned**: Windows Search Index (Windows.edb), macOS Spotlight, Linux Tracker

### 4.2 Hex Editor / Disk Editor
**Planned**: Sector-level view with templates for NTFS MFT, FAT/exFAT dir entries, ext4 inodes, GPT/MBR

### 4.3 S.M.A.R.T. Monitoring
**Planned**: Cross-platform SMART attribute reading, health scoring

### 4.4 File Preview Engine
**Planned**: Thumbnails, video frames, audio waveforms, document text extraction

### 4.5 Recovery Chance Estimation
**Planned**: Cluster allocation status, overwrite probability, TRIM awareness

## Phase 5: RAID & Advanced Storage - PLANNED

- RAID auto-detection (mdadm, LVM, LDM, Apple RAID, Storage Spaces)
- RAID 0, 1, 5, 6, 10, 50, 60
- Virtual RAID builder
- Drobo BeyondRAID, Synology Hybrid RAID, ZFS RAID-Z, Btrfs RAID

## Phase 6: Encryption Support - PLANNED (Detection Only)

- BitLocker header parsing
- LUKS1/LUKS2 header parsing
- VeraCrypt/TrueCrypt header parsing
- APFS FileVault / Encrypted DMG
- eCryptfs / fscrypt detection

## Phase 7: GUI Excellence - EXISTING (Tauri + Svelte 5)

Current GUI features:
- Source selection (devices and raw images)
- Quick/Deep scan with real-time progress
- Result filtering and preview
- Destination selection with safety validation
- Recovery with hash verification
- Cancel scan/recovery

The GUI does not expose several CLI features:
- Imaging
- Report export (JSON/HTML/CSV)
- Checkpoint resume
- Full session management

## Phase 8: Platform & Distribution - IN PROGRESS

### 8.1 Code Signing & Notarization
- Windows Authenticode (EV cert) - pending
- Apple Developer ID + Notarization - pending
- Linux AppImage/GPG signing - pending

### 8.2 Package Manager Submissions
- Homebrew (template only; not submitted)
- winget (template only; not submitted)
- Scoop (template only; not submitted)
- Chocolatey - pending
- AUR (template only; not submitted)
- Flatpak (template only; not submitted)
- Snap (template only; not submitted)
- Nixpkgs - pending
- Fedora COPR / openSUSE OBS - pending

### 8.3 Windows ARM64 Support
- Cross-compilation target defined
- CI testing needed

### 8.4 musl/Static Linux Builds
- Planned for forensic/embedded use

## Phase 9: Quality & Evidence - EXISTING (Strong)

### 9.1 Validation Corpus
- Deterministic synthetic fixtures for FAT32, exFAT, NTFS, ext4
- SHA-256 byte-for-byte verification
- Real-media validation corpus planned

### 9.2 Fuzzing
- cargo-fuzz harnesses for carver and filesystem probes
- CI runs bounded fuzz executions
- 24-hour sustained campaigns planned

### 9.3 Code Coverage
- Not yet measured in CI
- Target: >90% core, >80% overall

### 9.4 Performance
- Signature sweep: 3.74-4.00 GiB/s (target: >=4 GB/s)
- Memory: <350 MB RSS for 4 TB scan (target)

## Phase 10: Ecosystem & Community - PLANNED

### 10.1 MCP Server
- Read-only recovery tools for AI agents
- Session query, candidate listing, recovery planning

### 10.2 Plugin Architecture
- Dynamic carver plugins (Rust/CDylib)
- Filesystem parser plugins
- Output format plugins

### 10.3 Language Bindings
- librelift C API
- Python/Rust/Node.js bindings

## Testing Status

The original inventory snapshot recorded 92 tests. The current local test count is maintained in [test results](docs/TEST_RESULTS.md).

```
test result: ok. 81 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

New tests added:
- `partitions::tests::reads_mbr_with_primary_partitions`
- `partitions::tests::reads_gpt_partition_table`
- `virtual_disk::tests::vhd_fixed_disk`

## Competitive Position After Implementation

| Feature | Relift | TestDisk/PhotoRec | DMDE | R-Studio/UFS Explorer |
|---------|--------|-------------------|------|----------------------|
| Cost | Free, no tiers | Free | Freemium | Paid |
| Open Source | ✅ GPL-3.0+ | ✅ GPLv2+ | ❌ | ❌ |
| Read-only source access | Implemented; see threat model | See linked comparative research | Not assessed here | Not assessed here |
| Destination separation | Raw-device same-disk refusal; image files require user volume choice | See linked comparative research | Not assessed here | Not assessed here |
| Carving | 153 registered IDs; validator-backed subset emits candidates | See linked comparative research | Not assessed here | Not assessed here |
| SHA-256 verification | Available during recovery; may be disabled | See linked comparative research | Not assessed here | Not assessed here |
| Sessions/resume | Scan resume implemented; recovery resume not implemented | See linked comparative research | Not assessed here | Not assessed here |
| Partition recovery | Lost-signature scan; no partition table write | See linked comparative research | Not assessed here | Not assessed here |
| Virtual disks | Partial inspection; scan integration absent | See linked comparative research | Not assessed here | Not assessed here |
| Forensic images | E01/AFF/AFF4 not implemented | See linked comparative research | Not assessed here | Not assessed here |
| Hex editor | Planned | ❌ | ✅ | ✅ |
| S.M.A.R.T. monitoring | Planned | ❌ | ✅ | ✅ |
| File preview | Limited | ❌ | Limited | ✅ |
| Custom signatures | Planned | ❌ | ✅ | ✅ |
| Search index integration | Planned (Unique!) | ❌ | ❌ | ❌ (Recover has it) |
| RAID reconstruction | Planned | Basic | ✅ | ✅ |
| Encryption support | Planned (Detection) | ❌ | ❌ | ✅ |

## Unique Differentiators (Relift Only)

1. **Read-only source access** - Source access is exposed through a read-only interface
2. **Destination safety** - Raw-device same-disk recovery is refused; image-file destinations need a user-selected separate volume
3. **Validated carving** - Only validator-accepted candidates emit; validator coverage and maturity vary
4. **SHA-256 verification** - Recovery can verify output hashes; verification can be disabled
5. **Resumable scans** - Checkpoint-based scan resume is implemented; recovery resume is not
6. **Search index integration** (planned) - Recover filenames from Windows Search/Spotlight/Tracker
7. **Completely free, no tiers, no telemetry** - GPL-3.0-or-later
8. **Cross-platform native** - Windows/Linux/macOS with same engine

## Next Steps (Priority Order)

1. **Complete QCOW2 implementation** - High demand for cloud/VM forensics
2. **Implement hex editor** - Critical for forensic analysis
3. **Add S.M.A.R.T. monitoring** - Pre-scan health assessment
4. **Implement search index integration** - Unique killer feature
4. **Add RAID auto-detection** - mdadm, LVM, LDM, Storage Spaces
5. **Implement encryption detection** - BitLocker, LUKS, FileVault
6. **Complete GUI feature parity** - Imaging, reports, resume
7. **Code signing & notarization** - Trustable distribution
8. **Real-media validation corpus** - Evidence-based claims
9. **MCP server** - AI agent integration
10. **Plugin architecture** - Community extensibility

## Files Modified/Created

### New Files
- `crates/relift-core/src/partitions.rs` - Partition table parsing + lost partition scanning
- `crates/relift-core/src/virtual_disk.rs` - Virtual disk format support
- `S_TIER_ROADMAP.md` - Comprehensive roadmap
- `IMPLEMENTATION_SUMMARY.md` - This document

### Modified Files
- `crates/relift-core/src/lib.rs` - Exported new modules
- `crates/relift-core/src/error.rs` - Added `NotImplemented` error variant
- `crates/relift-core/src/scan.rs` - Fixed Volume struct initialization
- `crates/relift-core/src/session.rs` - Added Default derive to SessionOptions
- `crates/relift-core/src/partitions.rs` - Fixed test issues, improved GPT/MBR tests
- `crates/relift-core/src/virtual_disk.rs` - Fixed VHD footer parsing, VMDK grain tables, checksum verification
- `crates/relift-core/Cargo.toml` - Added tempfile dev-dependency
- `crates/relift-cli/src/main.rs` - Added new CLI commands (partitions, partitions-scan, virtual-disk, forensic-image)

## Conclusion

Relift has useful safety and workflow foundations, but broad recovery accuracy and feature parity have not been established through real-media testing. The highest-value work is validating parsers on representative media, then adding damaged-drive imaging and broader filesystem and container support.

The foundation is solid for adding the remaining differentiating features (search index integration, hex editor, RAID, encryption detection) to become the undisputed #1 free/open-source recovery tool.


