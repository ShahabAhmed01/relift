# Research Landscape

**Updated:** 2026-11-15
**Status:** Initial desk research only. This is not a complete competitor study,
filesystem-specification review, or hands-on comparison.

## Existing recovery tools

- [PhotoRec file formats](https://www.cgsecurity.org/wiki/File_Formats_Recovered_By_PhotoRec): CGSecurity describes PhotoRec as a carving tool that searches for known headers and lists more than 480 extensions across about 300 file families. This is the project's own format listing, not a third-party benchmark. The breadth is one reason Relift does not claim feature-count parity.
- [TestDisk](https://www.cgsecurity.org/wiki/testDisk): CGSecurity documents partition-table and boot-sector repair as well as undelete/copy operations for FAT, exFAT, NTFS, and ext filesystems. Those repair workflows are outside Relift's read-only recovery scope.
- [Tauri frontend configuration](https://v2.tauri.app/start/frontend/): Tauri documents support for multiple frontend frameworks. Relift uses the existing Svelte 5 frontend rather than combining frameworks.

No competing tool was run against a shared labeled recovery corpus. No
head-to-head accuracy, speed, or safety comparison is available. See
[`comparison.md`](../comparison.md) for the comparison policy.

## Current project architecture

The implementation uses a Rust workspace with `relift-core`, `relift-cli`,
`relift-gui`, `relift-testkit`, and `relift-bench`. The CLI and Tauri/Svelte
GUI call the shared core. The selected license is GPL-3.0-or-later. Rust 1.90
is the declared minimum. See the [conflict register](../decisions/CONFLICT_REGISTER.md)
and architecture decision records for the repository's decisions.

## Filesystem implementation scope

The current parser modules are FAT, exFAT, NTFS, ext, HFS+, APFS, Btrfs, and
XFS. The first four groups have deterministic synthetic fixture tests; HFS+,
APFS, Btrfs, and XFS remain experimental. The implementation and its gaps are
documented in [`support-matrix.md`](../support-matrix.md) and
[`GAP_REPORT.md`](../GAP_REPORT.md).

This repository does not yet contain a sourced specification review for every
parser. In particular, APFS, HFS+, Btrfs, XFS, partitioning, encryption, RAID,
and platform raw-device behavior need further primary-source research before
their implementation can be described as production-validated.

## Evidence and limits

- Synthetic tests compare recovered bytes with fixture ground truth where
  applicable.
- No real-volume recovery study or labeled precision/recall corpus exists.
- The Criterion suite is an internal synthetic smoke benchmark; no public
  hardware performance comparison has been published.
- The GitHub v0.1.0 preview is published with unsigned assets and a checksum
  manifest. Package-manager submissions, signing, notarization, and docs-site
  deployment/indexing remain incomplete.
