# Archived S-tier plan draft — superseded

> Do not use this draft as a description of the current project. Its 36-format
> count is obsolete, and its “zero false positives” and competitor-dominance
> targets were not evidence-based. The current implementation and realistic
> priorities are documented in [S_TIER_ROADMAP.md](S_TIER_ROADMAP.md), the
> [support matrix](docs/src/support-matrix.md), and the
> [competitive analysis](docs/research/COMPETITIVE_ANALYSIS.md).

## Current state (audit)
- Rust workspace: relift-core, relift-cli, relift-testkit, relift-gui (Tauri+Svelte), relift-bench
- Read-only source abstraction; destination separation enforced; SHA-256 verification; checkpoint resume; JSON output; shell completions; doctor self-test
- Filesystem metadata: FAT12/16/32, exFAT, NTFS, ext2/3/4 (fixture-validated); HFS+/APFS/Btrfs/XFS probe-only; ReFS/F2FS/ZFS/ReiserFS/UDF missing
- Carving: 36 registered formats with structural validators (vs PhotoRec's 480+ extensions / ~300 families) ← #1 competitive gap
- Image formats: raw + virtual disk (VHD/VHDX/VMDK/QCOW/VDI/DMG abstractions, partial); E01/AFF4 not implemented
- CLI: scan/list/recover/image/devices/doctor/report/completions/version/partitions/virtual-disk/forensic-image
- Missing (planned): MCP server, TUI, self-update, config files, `info`/`verify`/`session` commands, code signing, SBOM

## S-tier targets (beat every competitor)

### P0 — Format coverage (beat PhotoRec)
Expand carving from 36 → 300+ registered formats covering every PhotoRec family
(archives, multimedia, office, email, databases, executables, fonts, games, raw camera,
forensic, disk images) with reusable structural validators + zero-false-positive guarantee.

### P0 — Planned CLI commands
`relift info` (source/session inspection), `relift verify` (hash/integrity audit),
`relift session` (session management), `relift self-update`, `relift mcp` (MCP server).

### P0 — Config file
`--config` / `config.toml` for persistent defaults (scan mode, filters, confidence, dest).

### P1 — Filesystem completeness
Full metadata undelete for HFS+, APFS, Btrfs, XFS + add ReFS, F2FS, ZFS, ReiserFS, UDF.

### P1 — Image formats
Complete E01/Ex01, AFF4 read support; finish VHD/VHDX/VMDK/QCOW2/DMG readers.

### P1 — Encryption (with credentials)
BitLocker, LUKS, FileVault passphrase unlock + recovery.

### P2 — UX/performance
TUI (ratatui), parallel scan, mmap/O_DIRECT, pause/resume in GUI, GUI report export + preview.

### P3 — Release dominance
Code signing (Authenticode + Apple Dev ID), notarization, SBOM + attestations, all package managers,
musl/ARM64 builds, reproducible builds, docs site, GitHub topics/homepage.

## Execution order
1. carve.rs rewrite → data-driven registry, ~300 formats (this session)
2. info/verify/session/self-update/mcp CLI commands + config.toml
3. TUI + GUI report export
4. Filesystems (HFS+/APFS/Btrfs/XFS full, ReFS/F2FS/ZFS/ReiserFS/UDF)
5. Image formats (E01/AFF4 complete, finish virtual disk readers)
6. Encryption + performance + release polish
