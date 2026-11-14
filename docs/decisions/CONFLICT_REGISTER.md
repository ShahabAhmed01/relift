# Conflict Register

This document reconciles the many conflicting names, stacks, licenses, and
design goals that appeared in the user's original master prompt. It is the
single source of truth for project identity. When this register and another
document disagree, the register wins.

## Canonical identity

| Field | Decision | Rejected alternatives |
|-------|----------|----------------------|
| Name | **Relift** | `Unlost` (tryunlost.com), `Vestige` (DFIR tooling), `Exhume` (academic ExtFS recovery), `Remanence` (DFIR workbench), `Slackspace` (forensics firm), `Phoenix`, `ApexRecover`, `Palimpsest` |
| Binary | `relift` | — |
| License | **GPL-3.0-or-later** | — |
| Language | **Rust 1.100.0** (GNU toolchain) | — |
| Crates | `relift-core`, `relift-cli`, `relift-testkit` | — |

`Relift` was verified free on crates.io, npm, and PyPI; no data-recovery or
DFIR collision was found. The license text is stored at `LICENSE`.

## Stack reconciliation

The prompt mentioned Node, Python, and several GUI frameworks. The verified,
canonical stack is:

- Rust 1.100.0 `x86_64-pc-windows-gnu`, installed via rustup (no MSVC/admin).
- Dependencies: `serde`, `serde_json`, `sha2`, `memchr`, `crc32fast`, `flate2`,
  `thiserror`, `clap`, `anyhow`.
- Windows raw devices: `GENERIC_READ` + `FILE_FLAG_OVERLAPPED`.
- Telemetry: none. Default offline.

Node and Python are not part of the product. Any future GUI/TUI remains Rust
(native fallback or Tauri once a WebView2 path is verified).

## Design decisions recorded as ADRs

| ADR | Title | Status |
|-----|-------|--------|
| ADR-0001 | Canonical name and stack | Accepted |
| ADR-0002 | Read-only invariant | Accepted |
| ADR-0003 | Sparse-file segment model | Accepted |
| ADR-0004 | Carving policy (structurally validated only) | Accepted |
| ADR-0005 | Desktop GUI/TUI decision deferred | Accepted (documented fallback) |
| ADR-0006 | Exit-code grammar | Accepted (see docs/CLI.md) |

## Filesystem support decisions

| Target | Status | Notes |
|--------|--------|-------|
| FAT12/16/32 | Implemented + fixture-tested | Metadata + carving |
| exFAT | Implemented + fixture-tested | Metadata + carving |
| NTFS | Implemented + fixture-tested | MFT records, resident/non-resident/sparse |
| ext2/3/4 | Implemented + fixture-tested (ext4) | Extents + indirect blocks |
| HFS+/APFS | Identified, not implemented | Reported honestly |
| ReiserFS, XFS, Btrfs | Identified, not implemented | Reported honestly |

## Support matrix honesty

Unsupported filesystems are identified and reported, never silently claimed as
recoverable. Documented gaps: ext4 journal/$UsnJrnl, OLE, RAR5, ZIP64, FLAC
carving, ext4 inline data beyond the inode body, encryption, bigalloc, journal
replay, NTFS `$ATTRIBUTE_LIST`, named ADS.

## Open gaps (intentional)

- No real NTFS/ext4 volumes available locally; only synthetic fixtures.
- No signing certificates/notarization credentials; releases are honest/unsigned
  unless credentials are provided.
- macOS raw-device destination safety intentionally fails closed; macOS raw paths
  untested.
- Desktop GUI framework decision deferred (Tauri 2 needs a WebView2/MSVC-verified
  path not available here).
