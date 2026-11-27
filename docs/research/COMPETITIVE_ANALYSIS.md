# Competitive analysis — Relift vs. established recovery applications

**Research date:** 2026-11-27
**Product:** Relift (free, open-source, GPL-3.0-or-later)
**Sources:** official product documentation and repositories (CGSecurity TestDisk/PhotoRec docs, Microsoft Windows File Recovery docs, R-Studio/UFS Explorer pricing pages). All capability claims below are labeled as verified (from the source named) or assumption (reasoned from public docs, not independently tested by Relift).

## How to read this document

- **Verified** = stated by the product's official documentation, repository, or release notes; confirmed at research date.
- **Assumption** = inferred from public documentation; not independently verified by the Relift project.
- Relift claims are drawn from `docs/src/support-matrix.md`, `docs/src/VERIFICATION.md`, and the relift-core implementation, all validated against the code and test suite on 2026-11-27.

This analysis deliberately avoids SEO-style ranking claims, fabricated comparisons, and manipulated popularity figures. It states where Relift is behind, where it is comparable, and where it can realistically be preferred.

## Products compared

| Product | License | Cost | Platform | Interface |
|---|---|---|---|---|
| Relift | GPL-3.0-or-later | Free | Windows, Linux, macOS | CLI + Tauri GUI |
| TestDisk / PhotoRec | GPL v2+ | Free | Windows, Linux, macOS | CLI/TUI (PhotoRec has QPhotoRec) |
| Windows File Recovery | Proprietary (Microsoft) | Free | Windows | CLI (`winfr`) |
| Recuva | Proprietary | Free tier; paid Pro | Windows | GUI |
| DMDE | Proprietary | Free tier (limited); paid | Windows | GUI |
| R-Studio | Proprietary | $49.99–$79.99 one-time (per Capterra, 2026) | Windows, Linux, macOS | GUI |
| UFS Explorer | Proprietary | $69.95–$699.95 (per vendor site, 2026) | Windows | GUI |

Pricing and tier details for proprietary products come from public vendor/reseller pages at research date and may have changed. Relift has no paid tier.

## Capability matrix

| Capability | Relift | TestDisk / PhotoRec | Windows File Recovery | Recuva (free) | R-Studio / UFS Explorer |
|---|---|---|---|---|---|
| Cost | Free, no tier | Free | Free | Free tier, paid Pro | Paid |
| Open source | Yes | Yes | No | No | No |
| Cross-platform | Win/Linux/macOS | Win/Linux/macOS | Windows | Windows | Win/Linux/macOS |
| GUI | Yes (Tauri + Svelte) | No (CLI/TUI); QPhotoRec basic | No | Yes | Yes |
| CLI | Yes | Yes | Yes | No | Limited |
| Read-only source by construction | Yes | Yes (read-only scan) | Yes | Yes | Yes |
| Destination separation enforced | Yes | No | No | No | No |
| Source immutability guarantee | Verified | Not stated | Not stated | Not stated | Not stated |
| Deleted-file metadata recovery (FAT/exFAT/NTFS/ext4) | Verified on fixtures | Verified (FAT/exFAT/NTFS/ext2) | Verified (NTFS) | Verified (assumption) | Verified (assumption) |
| Deep scan / signature carving | Yes, 36 formats | Yes (PhotoRec, hundreds of signatures) | Yes (signature mode) | Yes | Yes |
| Structurally validated carving | Yes (every format validated) | No (magic + header, more false positives) | Partial | Assumption | Assumption |
| Lost partition detection | No | Yes | No | No | Yes |
| Raw disk image support | Yes | Yes | No | No | Yes |
| Virtual disk (VHD/VMDK/QCOW2/DMG) | No | No | No | No | Yes (assumption) |
| E01/EWF evidence images | No | No | No | No | Yes (R-Studio, assumption) |
| SHA-256 recovery verification | Yes | No | No | No | Assumption |
| Sessions and resume | Yes | No | No | No | Partial (assumption) |
| Preview | Limited (assumption) | No | No | Yes | Yes |
| Filtering/search | Yes | Minimal | Limited | Yes | Yes |
| Reporting (JSON/CSV/HTML) | JSON CLI; HTML report | No | No | No | Yes (assumption) |
| Pause/resume scan | No | No | No | No | Partial (assumption) |
| Imaging / bad-sector handling | Single-pass imaging | No | No | No | Yes (assumption) |
| SSD TRIM handling | Documents limitation | Documents limitation | Documents limitation | N/A | Documents limitation |
| Telemetry / tracking | None | None | Microsoft Store app | Unknown | Unknown |
| Account registration required | No | No | Microsoft Store | No | No |

## Per-product notes

### TestDisk / PhotoRec (CGSecurity)

**Verified:** GPL v2+, free, runs on Windows/Linux/macOS. TestDisk detects many filesystems (NTFS, FAT12/16/32, exFAT, ext2/3/4, btrfs, HFS, XFS, ReiserFS, UFS, ZFS, APFS, and more) and can undelete files from FAT, exFAT, NTFS, and ext2. PhotoRec performs signature carving across hundreds of file signatures. Official docs note PhotoRec recovers data but not original filenames in most cases, and that ext3/ext4 deleted-file data locations are generally unavailable. TestDisk's documentation reports fragmented files are not recovered reliably.

**Relift compared:** Relift matches TestDisk on the core deleted-file use case for FAT/exFAT/NTFS/ext4, adds structurally validated carving (fewer false positives than raw signature matching), a GUI, destination separation enforcement, SHA-256 verification, and resumable sessions. Relift trails TestDisk on lost-partition detection, broader filesystem coverage (btrfs, XFS, HFS, ReiserFS, UFS, ZFS partition-level), and the sheer breadth of PhotoRec's signature list. PhotoRec carves more formats but without structural validation, so it emits more false positives and does not preserve original filenames.

### Windows File Recovery (Microsoft)

**Verified:** Free Microsoft Store tool, Windows only, CLI (`winfr`). Supports NTFS, FAT, exFAT, ReFS. Modes: Regular/Extensive/Signature. Documentation states NTFS recently-deleted files use Regular mode; older deletions, formatting, and corruption use Extensive; signature mode for non-NTFS.

**Relift compared:** Relift is cross-platform and adds a GUI, whereas Windows File Recovery is Windows-only and CLI-only. Relift's destination-separation enforcement and session resume go beyond what the tool documents. Relift does not support ReFS; Windows File Recovery does.

### Recuva / DMDE (Windows, freemium)

**Assumption:** Recuva offers a free GUI with a paid Pro tier; DMDE offers a free tier with recovery limits and paid unlocks. Both are Windows-focused.

**Relift compared:** Relift is entirely free with no file-count, size, or feature tiers, and is cross-platform. Relift's read-only and destination-safety guarantees are explicit and enforced in code, which freemium GUI tools often do not document as enforceable invariants.

### R-Studio / UFS Explorer (paid, commercial)

**Verified:** Proprietary, one-time or per-seat pricing (public pages list roughly $49.99–$79.99 for R-Studio and $69.95–$699.95 for UFS Explorer tiers at research date). Marketed for RAID, virtual disks, E01/EWF evidence images, and severely damaged filesystems.

**Relift compared:** Relift cannot yet match RAID reconstruction, virtual-disk containers, E01/EWF, or deep damaged-filesystem repair. These are documented gaps in `docs/src/GAP_REPORT.md`. Relift's advantage is cost (free), openness (GPLv3+, auditable), cross-platform availability, offline-first operation with no telemetry, and explicit enforcement that the source is never written to.

## Where Relift can realistically be preferred

1. **Price and openness.** Completely free, no tiers, no paid unlocks, GPLv3+ source auditable by anyone. For users who cannot or will not pay $50–$700, Relift is the only option in this comparison that is both free and open source across Windows, Linux, and macOS.

2. **Safety guarantees.** Relift enforces read-only source access and destination separation at the code level (`ReadOnlySource` exposes no write method; `safety.rs` refuses same-device destinations and confines writes). These are testable invariants, not documentation promises.

3. **Validated carving.** Relift's 36 carved formats each pass a structural validator before a candidate is emitted. Random data produces zero false positives (tested). This contrasts with raw magic-byte carvers that emit more false positives.

4. **Recovered-output verification.** SHA-256 verification of recovered bytes against what was written back, plus resumable, inspectable sessions.

5. **Cross-platform CLI + GUI.** One tool, same engine, on Windows, Linux, and macOS, with both scriptable JSON CLI output and a desktop GUI.

## Where Relift trails today (honest gaps)

- **Lost partition detection.** TestDisk and commercial tools reconstruct lost partitions; Relift scans a single volume/image.
- **Broader filesystem coverage.** btrfs, XFS, HFS+, ReiserFS, UFS, ZFS partition-level recovery is not implemented; HFS+/APFS/Btrfs/XFS are probe-identified but metadata recovery is experimental.
- **Virtual disk and evidence containers.** VHD/VHDX, VMDK, QCOW2, DMG, E01/EWF are not supported; only raw images.
- **Damaged/RAID filesystem repair.** No boot-sector repair, RAID reconstruction, or heavily corrupted-filesystem handling.
- **Real-media validation corpus.** All Relift recovery evidence is synthetic-fixture based; no physical drive or representative real-volume corpus has been validated (see `docs/src/VERIFICATION.md`).

These gaps are tracked in `docs/src/GAP_REPORT.md` and are research targets, not claimed capabilities.

## Competitive positioning statement

Relift is not positioned as a clone of TestDisk, R-Studio, or Windows File Recovery. It is positioned as the free, open-source, cross-platform recovery tool that enforces source safety, validates every carved file, verifies recovered output, and keeps source and destination strictly separated — with no paid tier, no telemetry, and no account requirement. Users who need partition recovery, RAID reconstruction, virtual-disk containers, or evidence-image formats should use TestDisk or a commercial suite; users who want a free, transparent, safety-first tool for deleted files on disk images and drives are Relift's target audience.
