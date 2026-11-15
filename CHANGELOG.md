# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned
- Real-media validation corpus and fixtures for HFS+, APFS, Btrfs, XFS.
- `cargo-fuzz` harness and coverage reporting.
- E01/AFF4/VHD/VMDK/QCOW2/DMG image readers.
- RAR5, ZIP64, OLE, MPEG-TS and ISO 9660 validated carvers.
- Code signing, notarization and package-manager submissions.
- TUI, GUI pause/resume, GUI report export.

## [0.1.0] - 2026-11-15

First technical preview. The project builds and all checked-in tests pass; see
`FINAL_REPORT.md` and `docs/GAP_REPORT.md` for the verified scope and known
gaps.

### Added

- Rust workspace: `relift-core`, `relift-cli`, `relift-testkit`, `relift-gui`
  (Tauri backend in `crates/relift-gui/src-tauri`), `relift-bench`.
- Read-only source abstraction (`ReadOnlySource`, `FileSource`,
  `RawDeviceSource`) with no write operations on evidence.
- Filesystem metadata recovery:
  - FAT12/16/32: directory entries, LFN, cluster chains.
  - exFAT: entry sets, allocation bitmap, NoFatChain.
  - NTFS: `$MFT` records, resident/non-resident data, sparse runs.
  - ext2/3/4: inodes, direct/indirect blocks, extent trees, sparse and
    unwritten runs.
  - Experimental parsers for HFS+, APFS, Btrfs and XFS.
- Validating signature carving with structural checks for JPEG, PNG, GIF, BMP,
  TIFF, WebP, HEIC, AVIF, PDF, ZIP/DOCX/XLSX/PPTX, 7z, RAR4, gzip, bzip2, xz,
  zstd, tar, MP4/MOV, MKV, MP3, WAV/RIFF, FLAC, Ogg, SQLite, PST, LNK, EVTX, PE
  and ELF.
- Sparse and multi-segment data locations.
- Raw disk imaging with `.partial` output, SHA-256 manifest and resumable map
  file.
- SQLite (WAL) session store with resumable scans and recoveries.
- Destination safety checks that refuse the source's physical device.
- SHA-256 verification of recovered files and `.partial` quarantine on failure.
- Confidence scoring (Verified/Likely/Partial/Unverified) with reasons.
- JSON/CSV/HTML/Markdown reports.
- CLI: scan, recover, image, devices, list, report, doctor, completions and man
  page; `--json` output; exit codes 0–5; filters, dry-run and resume.
- GUI: Tauri 2 + Svelte 5 + Tailwind CSS 4, four-screen flow, progress events,
  candidate filters, preview pane, destination picker and cancellation.
- Deterministic fixture generators for FAT32, exFAT, NTFS and ext4.
- 51 tests: 43 core unit, 6 end-to-end and 2 CLI integration tests.
- CI for Linux/Windows/macOS on stable and Rust 1.90 with clippy, rustfmt,
  docs and `cargo-deny` checks.
- Release workflow for CLI archives (Linux x86_64/aarch64, Windows x86_64,
  macOS x86_64/aarch64) and unsigned desktop bundles.
- Documentation: README, CLI reference, architecture, support matrix, threat
  model, limitations, gap report, verification plan, ADRs, benchmark
  methodology, `AGENTS.md` and packaging templates.

### Known limitations

- Synthetic fixtures only; no real-volume validation yet.
- Release artifacts are unsigned.
- HFS+/APFS/Btrfs/XFS support is experimental.
- No fuzzing harness, coverage measurement, TUI or image-format readers yet.
