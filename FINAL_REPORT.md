# Relift — Final Verification Report

Date: 2026-11-15
Host: Windows 11 x86_64, Rust 1.100.0 (MSVC), Node.js 26.13.0

This report records what was run and what it proved. It replaces earlier
planning-era reports that described work as complete before it compiled.

## Commands run

| Command | Result |
|---------|--------|
| `cargo build --release --workspace` | PASS |
| `cargo test --workspace --all-targets` | PASS — 51 tests, 0 failures |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo +1.90 check --workspace --all-targets` | PASS (MSRV) |
| `npm run build` (`crates/relift-gui`) | PASS |
| `npm run tauri icon src-tauri/icons/icon.svg` | PASS — full icon set |
| `npm run tauri build` (`crates/relift-gui`) | PASS — desktop binary built |

## Test inventory

- 43 core unit tests: filesystem parsers, carving validators, confidence
  scoring, sessions, reporting, safety checks.
- 6 end-to-end tests: FAT32, exFAT, NTFS and ext4 scan → recover → SHA-256 byte
  comparison, destination separation and output confinement.
- 2 CLI integration tests.

## Safety properties exercised

- Sources are opened through `ReadOnlySource`, which exposes no write method.
- Recovery refuses a destination that resolves to the same physical device as
  the source (exit code 3).
- Output path confinement prevents path traversal via candidate names.
- A failed write or hash mismatch leaves a `.partial` file rather than a file
  that looks complete.

## Honest limitations

- All recovery evidence comes from deterministic synthetic fixtures. No real
  disk image or physical drive has been validated in this repository yet.
- HFS+, APFS, Btrfs and XFS parsers are experimental and have no fixture tests.
- RAR5, ZIP64, OLE, MPEG-TS and ISO 9660 carvers are not implemented.
- E01/AFF4/VHD/VMDK/QCOW2/DMG images must be converted to raw first.
- Release artifacts are unsigned; no signing/notarization credentials exist.
- A `cargo-fuzz` harness is not checked in and coverage is not measured.

## Release readiness

The codebase builds, lints and tests cleanly, and the release workflow produces
unsigned CLI archives and desktop bundles on tags. It is ready for a `v0.1.0`
technical preview, not for a 1.0 claim. The remaining release gates are tracked
in `docs/GAP_REPORT.md` and `docs/VERIFICATION.md`.
