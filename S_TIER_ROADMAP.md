# Relift Recovery Tool Roadmap

## Executive Summary

This is an aspirational engineering roadmap, not a ranking or a claim of parity. Relift is an early preview with synthetic-fixture evidence. Current implementation status is in the [support matrix](docs/src/support-matrix.md); representative project comparisons and source links are in the [competitive analysis](docs/research/COMPETITIVE_ANALYSIS.md).

## Current Competitive Position

| Area | Relift status today |
|------|----------------------|
| Source protection | Read-only source abstraction. Raw-device recovery checks for a different physical disk; image-file recovery checks only that output is not the image file, so users must choose another physical volume. |
| Carving | 153 identifiers are registered, including marker-only entries. Only validator-backed formats can emit candidates; validation maturity differs. |
| Filesystem recovery | Fixture-tested metadata recovery for FAT, exFAT, NTFS, and ext2/3/4. Other parsers are experimental or detection-only. |
| Partition handling | Reads existing MBR/GPT/APM tables and scans for lost-partition signatures. It does not repair a partition table or restore one to disk. |
| Virtual/forensic images | Some virtual-disk formats have inspection code, but scanning is not integrated. QCOW reads and DMG, Parallels, E01, and AFF/AFF4 support are not implemented. |
| Recovery workflow | Scan sessions resume at volume checkpoints. CLI imaging resumes from durable prefix checkpoints after checking the saved prefix against the current source. Recovery reports SHA-256 when verification is enabled; recovery itself is not resumable. |
| Distribution and validation | Preview release is unsigned. Evidence is synthetic; no representative real-media corpus has been validated. |

## Phase 1: Critical Gaps (Must Have for Parity)

### 1.1 Partition Recovery & Lost Partition Detection
**Priority: HIGH** - Mature recovery suites have broader partition workflows.
- Scan for filesystem signatures across entire disk
- Validate and reconstruct partition layouts for inspection (current recovery is a signature scan)
- Recover deleted/lost partitions with filesystem metadata where possible
- Boot sector analysis and repair planning for FAT/NTFS/ext

### 1.2 Virtual Disk Support
**Priority: HIGH** - Support is partial and is not connected to scan/recovery.
- VHD/VHDX (Hyper-V)
- VMDK (VMware - flat, sparse, split, delta, snapshot chains)
- QCOW/QCOW2 (QEMU/KVM)
- VDI (VirtualBox)
- DMG/sparsebundle (macOS)
- Parallels HDD/HDS

### 1.3 Forensic Image Formats
**Priority: HIGH** - Professional requirement
- E01/Ex01 (EnCase) - with compression/segmentation
- AFF4
- AFF/AFM/AFD
- RDR (R-Studio format)

### 1.4 Multi-pass Imaging with Bad Sector Handling
**Priority: HIGH** - Essential for failing drives
- Multi-pass with bounded retry logic
- Bad-sector range map (prefix checkpoint resume is implemented)
- Sparse image output
- Import/export interoperable maps and resume retry passes from the map
- Integration with DeepSpar/MRT hardware (future)

## Phase 2: Filesystem Completeness

### 2.1 Complete HFS+/APFS Metadata Recovery
- Full catalog B-tree parsing
- Deleted file recovery from HFS+/APFS
- APFS snapshot enumeration
- FileVault encryption detection (not decryption)

### 2.2 Complete Btrfs/XFS Metadata Recovery
- Btrfs: root/chunk/tree log trees, subvolumes, snapshots
- XFS: AG headers, inode B+tree, directory blocks
- Both: deleted inode recovery

### 2.3 Add ReFS, F2FS, ZFS, ReiserFS Metadata Recovery
- ReFS: directory metadata, deleted files
- F2FS: NAT/SIT/SSA areas, checkpoint
- ZFS: pool/dataset structure (read-only safe)
- ReiserFS: B+tree, deleted items

### 2.4 NTFS Enhancements
- $ATTRIBUTE_LIST traversal
- Named alternate data streams
- LZ77 compression decompression
- $UsnJrnl replay for recent deletions
- $I30 index slack parsing
- $Recycle.Bin metadata parsing

### 2.5 ext4 Enhancements
- Inline data >60 bytes
- Encryption detection (fscrypt)
- bigalloc support
- Journal replay for orphan recovery

## Phase 3: Carving & Signature Excellence

### 3.1 ZIP64 Dedicated Validator
- Separate handling from ZIP
- 64-bit EOCD locator/record support

### 3.2 Fragmented File Carving (IntelliRAW-style)
- Cross-reference validation
- Cluster chain reconstruction heuristics
- Fragment stitching for common formats (JPEG, MP4, ZIP)

### 3.3 Custom Signature Editor
- User-defined signatures with regex/header/footer
- Import/export signature databases
- Built-in signature validator/tester
- Community signature sharing

### 3.4 Expanded Format Support (Target: 200+ formats)
Priority additions:
- RAW camera formats (CR2, NEF, ARW, DNG, ORF, RW2)
- Video: MOV (separate from MP4), AVI, FLV, WMV, MTS, HEVC
- Audio: AAC, M4A, APE, WMA, DSD
- Documents: ODT, ODS, ODP, RTF, EPUB, DJVU
- Databases: MDB/ACCDB, MYI/MYD, FRM/IBD
- Email: EML, MSG, MBOX, DBX
- Archives: ACE, CAB, LZH, ARJ, ZPAQ
- Disk: VHD/VHDX, VMDK, QCOW2 as carved formats

### 3.5 Validation Enhancements
- Fuzz testing harness for every validator
- Property-based testing
- Corpus of real corrupted/truncated files

## Phase 4: Advanced Features (Differentiators)

### 4.1 Search Index Integration (Potential differentiator; confirm market coverage first)
- Windows Search Index (Windows.edb)
- macOS Spotlight (.Spotlight-V100)
- Linux Tracker (tracker-store)
- Reconstruct filenames/paths from search DB when FS metadata is gone

### 4.2 Hex Editor / Disk Editor
- Sector-level view with templates
- NTFS MFT record parser template
- FAT/exFAT directory entry template
- ext4 inode template
- GPT/MBR partition table template
- Search/replace with safety guards
- Bookmarks and annotations

### 4.3 S.M.A.R.T. Monitoring & Health Assessment
- Read SMART attributes on all platforms
- Health score calculation
- Pre-scan warning for failing drives
- Automatic single-thread fallback on read errors

### 4.4 File Preview Engine
- Thumbnails for images (JPEG, PNG, HEIC, AVIF, RAW)
- Video first-frame extraction
- Audio waveform + playback
- Document text extraction (PDF, DOCX, ODT)
- Hex view fallback

### 4.5 Recovery Chance Estimation
- Cluster/block allocation status check
- Overwrite probability scoring
- TRIM/UNMAP awareness
- Visual indicator in GUI (green/yellow/red)

### 4.6 Bootable Recovery Environment
- WinPE-based ISO for Windows
- Linux live ISO (Arch/Alpine based)
- PXE network boot support
- Automatic hardware detection

## Phase 5: RAID & Advanced Storage

### 5.1 RAID Reconstruction
- Auto-detection from metadata (mdadm, LVM, LDM, Apple RAID, Storage Spaces)
- RAID 0, 1, 5, 6, 10, 50, 60
- JBOD/concatenated
- Virtual RAID builder (manual config)
- Support for: Drobo BeyondRAID, Synology Hybrid RAID, ZFS RAID-Z, Btrfs RAID

### 5.2 LVM/LVM2 Support
- Physical volume / volume group / logical volume
- Snapshot support
- Thin provisioning

### 5.3 Windows Storage Spaces
- Pool/virtual disk detection
- Parity/mirror/simple layouts

## Phase 6: Encryption Support (Detection Only - No Cracking)

### 6.1 BitLocker
- Header parsing
- Recovery key / password prompt
- .BEK file support

### 6.2 LUKS1/LUKS2
- Header parsing
- PBKDF2/Argon2 parameters
- Passphrase/key file prompt

### 6.3 VeraCrypt/TrueCrypt
- Header detection
- Password/keyfile prompt

### 6.4 APFS FileVault / Encrypted DMG
- Volume encryption detection
- Password prompt

### 6.5 eCryptfs / fscrypt (ext4/F2FS)
- Policy detection
- Key management integration

## Phase 7: GUI Excellence (Feature Parity + Polish)

### 7.1 Full CLI Feature Parity
- Imaging with progress
- Report export (JSON/HTML/CSV)
- Checkpoint resume
- Session management

### 7.2 Advanced GUI Features
- Multi-tab sessions
- Drag-drop source/destination
- Real-time preview pane
- Filter builder UI
- Column sorting/customization
- Dark/light/system theme
- Accessibility (WCAG 2.1 AA)
- High-DPI support
- Large result set virtualization (100k+ items)

### 7.3 Session Management
- Session library/history
- Comparison view (diff sessions)
- Merge sessions
- Tag/annotate candidates

## Phase 8: Platform & Distribution

### 8.1 Code Signing & Notarization
- Windows Authenticode (EV cert)
- Apple Developer ID + Notarization
- Linux AppImage/GPG signing

### 8.2 Package Manager Submissions
- Homebrew (macOS/Linux)
- winget (Windows)
- Scoop (Windows)
- Chocolatey (Windows)
- AUR (Arch)
- Flatpak (Flathub)
- Snap (Snapcraft)
- Nixpkgs
- Fedora COPR / openSUSE OBS

### 8.3 Windows ARM64 Support
- Native ARM64 build
- CI testing on ARM64 runners

### 8.4 musl/Static Linux Builds
- Fully static CLI for embedded/forensic use
- Alpine compatibility

## Phase 9: Quality & Evidence

### 9.1 Real-Media Validation Corpus
- Curated test images from real drives
- Known ground truth (pre-deletion manifests)
- Automated regression testing
- Public benchmark dataset release

### 9.2 Sustained Fuzzing
- 24hr+ fuzz campaigns for all parsers
- Corpus minimization
- CI integration

### 9.3 Code Coverage
- CI coverage reporting
- Target: >90% for core, >80% overall

### 9.4 Performance Optimization
- io_uring on Linux
- Overlapped I/O on Windows
- SIMD-accelerated signature scanning
- Parallel carving workers
- Memory mapping for large images

## Phase 10: Ecosystem & Community

### 10.1 MCP Server (Model Context Protocol)
- Read-only recovery tools for AI agents
- Session query, candidate listing, recovery planning
- Integration with Claude Desktop, VS Code, etc.

### 10.2 Plugin Architecture
- Dynamic carver plugins
- Filesystem parser plugins
- Output format plugins
- Rust/CDylib plugin API

### 10.3 Python/Rust/Node.js Bindings
- librelift C API
- High-level bindings for scripting

### 10.4 Documentation Site Enhancement
- Interactive CLI reference
- Video tutorials
- Recovery cookbook/recipes
- FAQ with real scenarios

---

## Implementation Priority Order (First 6 Months)

| Week | Focus | Deliverable |
|------|-------|-------------|
| 1-2 | Partition Recovery | `relift partitions` command, lost partition scan |
| 3-4 | Virtual Disks | VHDX, VMDK, QCOW2, VDI read support |
| 5-6 | Forensic Images | E01/Ex01 reader |
| 7-8 | Multi-pass Imaging | Bad sector map, resume |
| 9-10 | HFS+/APFS Recovery | Full metadata recovery |
| 11-12 | Btrfs/XFS Recovery | Full metadata recovery |
| 13-14 | Custom Signatures | Signature editor, import/export |
| 15-16 | Search Index | Windows Search, Spotlight, Tracker |
| 17-18 | Hex Editor | Core + templates |
| 19-20 | SMART Monitoring | Cross-platform |
| 21-22 | File Preview | Thumbnails, video frames, text |
| 23-24 | RAID Auto-detect | mdadm, LVM, LDM, Apple RAID |

---

## Success Metrics

1. **Recovery accuracy target**: >95% byte accuracy on a published, ground-truth corpus; not measured yet
2. **False positive target**: <0.1% on representative data; not measured yet
3. **Scan speed target**: >=4 GB/s signature sweep; latest local benchmark is recorded in the README and methodology
4. **Memory target**: <350 MB RSS for a 4 TB scan; not measured yet
5. **Platform coverage target**: all supported workflows exercised on Windows, Linux, and macOS; full feature parity is not assumed
6. **Community**: 10k+ GitHub stars, 100+ contributors
7. **Package Managers**: Available in all major repos
8. **Citations**: Referenced in academic/forensic literature

---

## Guiding Principles

1. **Safety First**: Never write to source. Ever.
2. **Evidence Over Claims**: Every feature has tests + fixtures.
3. **Free Forever**: No paid tiers, no telemetry, no accounts.
4. **Cross-Platform Native**: Not Electron wrapper, not Windows-first.
5. **Standards Compliant**: Open formats, documented protocols.
6. **Honest About Limits**: Document what we can't do.
7. **Community Driven**: Open development, open governance.


