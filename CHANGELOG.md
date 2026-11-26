# Changelog

Notable changes to Relift are recorded here.

## [Unreleased]

### Added

- Checkpoint-based scan resume: `relift scan --resume` continues a cancelled or
  crashed scan after revalidating source identity (size, kind, fingerprint).
  Completed volumes are skipped; the interrupted volume is rescanned from its
  start. Candidate IDs are dense and `candidates.jsonl` is byte-identical to an
  uninterrupted run.
- Fuzz harnesses for the carving validator and filesystem scanner in `fuzz/`.
- Benchmark results and methodology in `docs/benches/results/`.

### Changed

- `scan` accepts `--session` pointing to an existing interrupted session when
  combined with `--resume`.
## [0.1.0] - 2026-11-15 (preview)

### Added

- Rust workspace with a recovery core, CLI, synthetic test fixtures, Tauri GUI, and benchmark harness.
- Read-only source abstraction and destination separation checks.
- FAT, exFAT, NTFS, and ext2/3/4 metadata recovery with deterministic synthetic fixtures; experimental HFS+, APFS, Btrfs, and XFS parsers.
- Structurally validated signature carving for the formats listed in docs/support-matrix.md.
- Raw-image imaging, candidate sessions, recovery verification, JSON/CSV/HTML reports, and CLI JSON output.
- Tauri desktop flow for source selection, scanning, candidate review, and recovery.
- Cross-platform Rust CI and a desktop release workflow.
- Corrected GUI scan progress, cancellation, result filtering, and Tauri DTO field casing.
- Removed unsupported device-health, SMART, TRIM, pause/resume, and theme controls.
- Removed the incomplete FLAC validator so metadata-only matches are not emitted as audio files.
- Migrated the GUI frontend to SvelteKit 3 and Vite 8; updated the Tauri build workflow to Node.js 22.
- Added frontend CI checks and a SHA-256 manifest for release assets.
- Reconciled documentation with implemented behavior and verification evidence.

### Known limitations

- Recovery has not been validated on real physical media or a representative real-world corpus.
- HFS+, APFS, Btrfs, and XFS support remains experimental.
- Interrupted scans do not resume; imaging has no multi-pass map-file workflow.
- The desktop GUI does not provide imaging or report export.
- Release assets are unsigned and not notarized. Package-manager submissions are not accepted or verified.
- SHA256SUMS records asset hashes but does not authenticate the publisher.

See [the v0.1.0 preview release notes](docs/releases/v0.1.0.md) and [published release](https://github.com/ShahabAhmed01/relift/releases/tag/v0.1.0) for downloads, checksum verification, and safety information.
