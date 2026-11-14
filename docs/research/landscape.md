# Research landscape

Relift is a read-only deleted-file recovery tool for disk images and drives.
This document records what was researched before architecture decisions were
finalized, per the project's "research before architecture" requirement.

## Name availability (verified)

| Name | crates.io | npm | PyPI | DFIR/data-recovery collision | Decision |
|------|-----------|-----|------|------------------------------|----------|
| Relift | free | free | free | none found | **Selected** |
| Unlost | — | — | — | tryunlost.com | Rejected |
| Vestige | — | — | — | DFIR tooling | Rejected |
| Exhume | — | — | — | academic ExtFS recovery | Rejected |
| Remanence | — | — | — | DFIR workbench | Rejected |
| Slackspace | — | — | — | forensics firm | Rejected |
| Phoenix | — | — | — | generic | Rejected |
| ApexRecover | — | — | — | generic | Rejected |
| Palimpsest | — | — | — | generic | Rejected |

## Existing open-source recovery tools (reference, not forking)

- **PhotoRec/TestDisk (CGSecurity):** Carving-focused, supports many filesystems,
  GPL. CLI-first. Reference for what "real byte-level recovery" looks like, but
  not a codebase Relift reuses.
- **scalpel (forensics):** Configurable signature carving. Reference for carving
  policy, not reused.
- **extundelete / undelete (ext):** ext-specific, largely unmaintained.
- **ntfsundeird / ntfs-3g tools:** NTFS-specific.

Relift differentiates by: read-only-by-construction, SHA-256-verified synthetic
fixtures, one shared engine for CLI/TUI/GUI, sparse-file segment preservation,
and an honest support matrix that never claims unsupported filesystems.

## Stack research

- Rust chosen for memory safety without a GC, strong FFI-free raw device access
  on Windows/Linux/macOS, and a single compiled binary.
- GNU toolchain selected over MSVC because it needs no admin install and no
  Visual Studio; verified compile/link/run at Rust 1.100.0.
- clap for CLI, serde/serde_json for sessions/reports, sha2 for verification,
  thiserror/anyhow for error handling.

## Filesystem research (first recovery targets)

| Filesystem | On-disk structures used by Relift |
|------------|-----------------------------------|
| FAT12/16/32 | BPB, FAT chain, 8.3 + LFN dir entries, deleted entry (0xE5) |
| exFAT | boot sector, FAT, entry-set checksums, stream extension, name entries |
| NTFS | boot sector, MFT records, $STANDARD_INFORMATION, $FILE_NAME, $DATA, runlists |
| ext2/3/4 | superblock, group descriptors, inode table, extent trees, indirect blocks |

Carving validators: JPEG, PNG, PDF, ZIP, GIF, BMP, TIFF, MP4, SQLite, RIFF,
MP3, Ogg, gzip, 7z, RAR4, PE, ELF.

## Honesty policy

- Unsupported filesystems are identified and reported, never silently claimed as
  recoverable.
- Only structurally validated carves are emitted.
- No simulated/mock results; every recovery is validated by synthetic fixtures
  with SHA-256 hash comparison.
- Default offline; no telemetry; no network client.
