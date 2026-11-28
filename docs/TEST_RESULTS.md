# Test Results — Relift

## Current audit run (2026-11-28)

Command: `cargo test --workspace --all-targets` (working tree with audit fixes)

| Crate | Target | Result | Count |
|-------|--------|--------|-------|
| relift-core | lib unit tests | PASS | 93 |
| relift-core | end_to_end integration | PASS | 11 |
| relift-cli | lib unit tests | PASS | 8 |
| relift-cli | contract integration | PASS | 5 |
| relift-gui | lib unit tests | PASS | 2 |
| relift-gui | main binary | PASS | 0 |
| relift-testkit | lib unit tests | PASS | 2 |
| relift-bench | main binary | PASS | 0 |

**Total: 121 tests passed, 0 failed, 0 ignored.** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo doc --workspace --no-deps` also passed. Eight synthetic imaging tests cover resume after interruption with a torn map tail, changed-source refusal, unsampled source-prefix changes, modified partial-prefix refusal, hard-link alias refusals for the source and output files, and finishing the manifest after an interrupted final rename. A CLI contract test confirms the resume option is exposed. The fixtures remain synthetic; this run does not validate recovery accuracy on real media.

`cargo check -p relift-core --all-targets --target x86_64-unknown-linux-gnu` also passed. The whole-workspace Linux cross-check could not complete on this Windows host because the Tauri dependency `libdbus-sys` requires a Linux pkg-config/sysroot.

The historical 2026-11-27 run is retained below as a dated record.

**Date:** 2026-11-27  
**Host:** Windows 11 x86_64  
**Toolchains:** Rust 1.100.0 (stable, MSVC, active), Rust 1.90.0 (MSVC)  
**Node.js:** 26.13.0  
**Branch:** `fix/ci-and-claim-reconciliation` (commit `0a75615`)  
**Validation basis:** Deterministic synthetic fixtures only. No physical devices accessed.

---

## Full workspace test suite

Command: `cargo test --workspace --all-targets`

| Crate | Target | Result | Count |
|-------|--------|--------|-------|
| relift-core | lib unit tests | PASS | 73 |
| relift-core | end_to_end integration | PASS | 11 |
| relift-cli | lib unit tests | PASS | 2 |
| relift-cli | contract integration | PASS | 4 |
| relift-gui | lib unit tests | PASS | 2 |
| relift-gui | main binary | PASS | 0 |
| relift-testkit | lib unit tests | PASS | 2 |
| relift-bench | main binary | PASS | 0 |

**Total: 94 tests passed, 0 failed, 0 ignored.**

### relift-core lib tests (73)

Covering: source opening/read-only behavior, truncated reads, FAT/exFAT/NTFS/ext4 filesystem parsing, carve validation (JPEG, PNG, PDF, ZIP, GIF, BMP, TIFF, MP4, SQLite, WAV, MP3, OGG, gzip, 7z, RAR4, PE, ELF, HEIC, AVIF, DOCX, XLSX, PPTX, MKV, PST, LNK, EVTX, WebP, TAR, bzip2, xz, zstd, FLAC, OLE, MPEG-TS, RAR5, ISO 9660), candidate filtering, hashing, session read/write/checkpoint/resume, scan, recover, report generation, safety (destination separation, path confinement), devices/partition mapping, and the new `registered_format_count_is_stable` guard.

### relift-core end_to_end tests (11)

- `probe_identifies_apfs` — APFS probe detection
- `probe_identifies_hfsplus` — HFS+ probe detection
- `probe_identifies_btrfs` — Btrfs probe detection
- `probe_identifies_xfs` — XFS probe detection
- `scan_metadata_reports_experimental_without_recovery` — experimental parsers reported, no recovery
- `recovers_exfat_fixture_bytes` — exFAT end-to-end SHA-256 verified
- `recovers_ntfs_fixture_bytes` — NTFS end-to-end SHA-256 verified
- `no_unexpected_files_are_written_outside_the_destination` — destination confinement
- `recovers_ext4_fixture_bytes` — ext4 end-to-end SHA-256 verified
- `destination_separation_is_still_enforced_end_to_end` — same-device refusal
- `recovers_fat32_fixture_bytes` — FAT32 end-to-end SHA-256 verified

### relift-cli tests (6)

- `error_exit_codes_are_stable` — exit code 2 for usage errors
- `partial_recovery_exit_code_is_stable` — exit code 5 for partial recovery
- 4 contract tests: `--json` output shape, exit codes, dry-run behavior, filter selection

### relift-gui tests (2)

- `scan_options_deserialize_snake_case_fields` — Tauri DTO contract
- `device_info_serializes_only_supported_fields_in_snake_case` — Tauri DTO contract

### relift-testkit tests (2)

- `fixture_boot_sector_is_fat32` — FAT32 fixture structure
- `deleted_file_data_is_present_in_the_image` — fixture contains deleted file data

---

## Lint and format checks

| Check | Command | Result |
|-------|---------|--------|
| Formatting | `cargo fmt --all -- --check` | PASS |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |

---

## Documentation build

| Check | Command | Result |
|-------|---------|--------|
| Rust docs | `cargo doc --workspace --no-deps` (RUSTDOCFLAGS=-D warnings) | PASS |
| mdBook site | `mdbook build docs` (0.4.35) | PASS |

---

## Dependency and license audit

| Check | Command | Result |
|-------|---------|--------|
| cargo-deny | `cargo deny check` | PASS (advisories, bans, licenses, sources all ok) |

---

## Repository guards

| Check | Command | Result |
|-------|---------|--------|
| Link check | `node scripts/check-links.mjs` | PASS (47 Markdown files, 0 broken links) |
| Encoding check | `node scripts/check-encoding.mjs` | PASS (131 text files, 0 BOM/NUL/mojibake issues) |

---

## What was NOT run

- **Fuzz smoke tests:** `cargo +nightly fuzz run` requires nightly toolchain and clang/libFuzzer; not executed in this local session. CI runs bounded fuzz (5000 runs each) on Linux schedule/workflow_dispatch.
- **Benchmark smoke test:** `cargo bench -p relift-bench -- --test` not executed locally. CI runs on schedule/workflow_dispatch.
- **Frontend checks:** `npm ci`, `npm run check`, `npm run build`, `npm audit` not executed locally (Node.js script execution policy restricted in this PowerShell session). CI runs these on ubuntu-latest.
- **Tauri GUI build:** `npm run tauri build` not executed locally. Previously verified on Windows (BUILD_STATUS.md).
- **Rust 1.90.0 MSVC test suite:** Not re-run locally on 1.90 toolchain. CI runs the full matrix (stable + 1.90 on Windows/Linux/macOS).
- **Cross-platform:** Only Windows local execution performed. Linux/macOS results come from CI history.

---

## Evidence status

| Claim | Evidence | Status |
|-------|----------|--------|
| FAT32/exFAT/NTFS/ext4 end-to-end recovery | Synthetic fixtures, SHA-256 byte comparison | Verified (11 end_to_end tests) |
| HFS+/APFS/Btrfs/XFS probe detection | Synthetic probe fixtures | Verified (4 probe tests) |
| Carving structural validation | Valid, truncated, malformed, random input tests | Verified (unit tests in carve.rs) |
| Source read-only | `ReadOnlySource` abstraction, no write methods | Code review; no write path |
| Destination separation | Same-device refusal tests | Verified (end_to_end + safety tests) |
| Session resume integrity | Checkpoint truncation, mismatch refusal, byte-identical resume | Verified (session + scan tests) |
| Real-media recovery accuracy | No physical drive or representative corpus | NOT VALIDATED |
| Signing/notarization | v0.1.0 assets unsigned | NOT CONFIGURED |
| Package-manager installation | Manifests contain placeholders | NOT SUBMITTED |
