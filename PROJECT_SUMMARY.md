# Relift — Implementation Summary

Relift is a GPL-3.0-or-later, read-only data-recovery tool written in Rust, with
a CLI and a Tauri 2 / Svelte 5 desktop GUI. This document summarizes what is in
the repository and what has actually been verified.

## Workspace

| Crate | Role |
|-------|------|
| `crates/relift-core` | Recovery engine: read-only source, filesystem parsers, validated carving, imaging, sessions, recovery, reporting, safety |
| `crates/relift-cli` | `relift` binary: scan, recover, image, devices, list, report, doctor, completions |
| `crates/relift-testkit` | Deterministic FAT32, exFAT, NTFS and ext4 image generators |
| `crates/relift-gui` | SvelteKit frontend (static adapter) |
| `crates/relift-gui/src-tauri` | Tauri 2 backend and desktop bundles |
| `crates/relift-bench` | Criterion benchmark harness |

`docs/` contains the CLI reference, architecture notes, support matrix, threat
model, limitations, gap report, verification plan, ADRs and benchmark
methodology. `AGENTS.md` documents the machine-readable/agent workflow.

## Implemented

### Engine (`relift-core`)
- `ReadOnlySource` trait with no write methods; `FileSource` opens read-only and
  `RawDeviceSource` uses an OS read-only handle.
- Filesystem metadata recovery for FAT12/16/32, exFAT, NTFS, ext2/3/4
  (fixture-validated) and HFS+, APFS, Btrfs, XFS (experimental parsers).
- Validating signature carving: JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF,
  PDF, ZIP/DOCX/XLSX/PPTX, 7z, RAR4, gzip, bzip2, xz, zstd, tar, MP4/MOV, MKV,
  MP3, WAV/RIFF, FLAC, Ogg, SQLite, PST, LNK, EVTX, PE, ELF.
- Sparse-file and multi-segment data locations; holes stay holes.
- Raw imaging with `.partial` file, SHA-256 manifest, map file for resume.
- SQLite (WAL) session store; scan and recovery state survives restarts.
- Destination safety: refuses destinations on the same physical device.
- SHA-256 verification of recovered files (default on) and `.partial`
  quarantine on write/hash failure.
- JSON/CSV/HTML/Markdown reports.
- Confidence labels (Verified/Likely/Partial/Unverified) with reasons.

### CLI (`relift`)
- Commands for scanning, recovery, imaging, device listing, session inspection,
  reporting, self-tests, shell completions and man-page generation.
- `--json` output with a stable schema and documented exit codes (0–5).
- Filter expressions, dry-run, minimum confidence, resume.

### GUI (`crates/relift-gui`)
- Tauri 2 backend in `src-tauri` with commands for devices, scan, candidates,
  recovery, imaging and doctor checks.
- SvelteKit + Svelte 5 runes frontend with Tailwind CSS 4.
- Source → Scan → Results → Recover flow, progress events, preview pane,
  filters, destination picker and cancellation.

## Verified

- `cargo build --release --workspace` succeeds.
- `cargo test --workspace --all-targets`: 51 tests pass (43 core unit tests,
  6 end-to-end fixture tests, 2 CLI integration tests).
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- `cargo fmt --all -- --check` is clean.
- `cargo +1.90 check --workspace --all-targets` passes (MSRV 1.90).
- Frontend production build succeeds and Tauri generates ICO/ICNS/PNG icons.
- CI covers Linux, Windows and macOS on stable plus MSRV, with Linux GUI system
  dependencies installed and `cargo-deny` checks.

## Not yet complete

Tracked in `docs/GAP_REPORT.md`:

- Real-volume validation (current evidence is synthetic fixtures only).
- Fixtures/end-to-end tests for HFS+, APFS, Btrfs and XFS.
- `cargo-fuzz` harness and coverage measurement.
- E01/AFF4/VHD/VMDK/QCOW2/DMG image readers.
- RAR5, ZIP64, OLE, MPEG-TS and ISO 9660 carvers.
- Code signing, notarization and package-manager submissions.
- TUI and GUI pause/resume, report export.

## Build

```sh
cargo build --release --workspace
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

GUI:

```sh
cd crates/relift-gui
npm install
npm run tauri build
```

## License

GPL-3.0-or-later. See `LICENSE`.
