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

The current support and verification boundaries are maintained in docs/support-matrix.md and docs/VERIFICATION.md. FAT, exFAT, NTFS and ext2/3/4 have synthetic fixture coverage. HFS+, APFS, Btrfs and XFS parsers are experimental. Signature carving emits only candidates accepted by registered validators. ZIP64
is not separately emitted (it shares the `PK\x03\x04` local-header magic with ZIP
and is validated by the existing ZIP validator). The registry includes
marker-only entries; validator coverage and maturity vary by format.

Only raw images are supported. Imaging can resume from durable prefix checkpoints with `relift image --resume`; it does not retry unreadable sectors. Sessions use JSON and JSONL files; a cancelled or crashed scan can be continued with `relift scan --resume` (volume-level checkpoint). The GUI supports source selection, scan progress, result filtering and recovery; imaging, report export, and resume remain CLI-only.

## Verification

The current Rust workspace has 121 tests; the full workspace suite passed on 2026-11-28 on Windows. Earlier checks on stable and Rust 1.90.0, the Svelte and TypeScript checks, the production build, and a local Windows Tauri installer build were recorded on 2026-11-15. Carving registers 153 format identifiers, including marker-only entries; see docs/src/formats.md for validator coverage details.

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
