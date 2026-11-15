# Relift project summary

Relift is a Rust command-line and Tauri desktop application for scanning raw disk images and devices for recoverable files. Source access uses a read-only abstraction. Recovery writes to a destination that must pass device-separation checks.

## Workspace

| Crate | Purpose |
|---|---|
| relift-core | Scanning, filesystem metadata parsing, signature carving, imaging, recovery, reports and destination safety |
| relift-cli | CLI commands and machine-readable output |
| relift-testkit | Deterministic synthetic FAT32, exFAT, NTFS and ext4 fixtures |
| relift-gui | SvelteKit frontend |
| relift-gui/src-tauri | Tauri commands and desktop packaging |
| relift-bench | Criterion benchmark harness |

## Supported scope

The current support and verification boundaries are maintained in docs/support-matrix.md and docs/VERIFICATION.md. FAT, exFAT, NTFS and ext2/3/4 have synthetic fixture coverage. HFS+, APFS, Btrfs and XFS parsers are experimental. Signature carving emits only candidates accepted by registered validators. FLAC, RAR5, ZIP64 and OLE are not emitted.

Only raw images are supported. Imaging is a single copy pass; there is no map-file resume. Sessions use JSON and JSONL files; interrupted scans do not resume from checkpoints. The GUI supports source selection, scan progress, result filtering and recovery; imaging and report export remain CLI-only.

## Verification

The current Rust workspace has 74 tests. On 2026-11-15, tests passed on stable and Rust 1.90.0. The Svelte and TypeScript checks and production build passed. A local Windows Tauri build generated MSI and NSIS installer packages.

All recovery accuracy evidence is synthetic. No physical drive or representative real-media corpus has been validated. Open release gates are listed in docs/GAP_REPORT.md.

## Build

    cargo build --release --workspace
    cargo test --workspace --all-targets

For the GUI, install Node.js 22.17 or newer, then run:

    cd crates/relift-gui
    npm ci
    npm run check
    npm run tauri build

See README.md and the docs directory for the full references.
