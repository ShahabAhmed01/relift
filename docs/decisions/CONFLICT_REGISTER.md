# Conflict Register

This register records the governing choices made from the mutually exclusive
options in the master prompt. The existing Relift repository and the user's
current safety instructions take precedence over aspirational alternatives.

## Canonical decisions

| Topic | Selected decision | Deferred or rejected alternatives | Evidence / revisit condition |
|---|---|---|---|
| Product name | Relift; binary `relift` | Vestige, Unlost, Phoenix, Exhume, ApexRecover and other names from prompt variants | Existing repository identity and GitHub URL; revisit only if the owner requests a rename |
| Core implementation | Rust workspace; minimum Rust 1.90 | Parallel C/C++ recovery engines | `Cargo.toml`, shared `relift-core` API |
| Windows toolchain | MSVC target for release artifacts | GNU-only distribution | `.github/workflows/release.yml` builds `x86_64-pc-windows-msvc`; CI compiles workspace on Windows |
| Desktop UI | Tauri 2 + Svelte 5 | Electron, Slint, other frontend stacks | `crates/relift-gui`; built by the release workflow |
| CLI | Native `relift-cli` using `relift-core` | Shell wrapper around another recovery tool | `crates/relift-cli`; CLI contract tests |
| TUI | Deferred; no TUI is shipped | Adding a second interface without a supported workflow | No TUI crate or command exists; revisit after a concrete user need and maintainer capacity are established |
| License | GPL-3.0-or-later | Conflicting MIT/Apache-only options in source prompt variants | Existing `LICENSE` and workspace package metadata |
| Source I/O | Open sources read-only; write only through explicit destination writers | In-place repair or source mutation | `ReadOnlySource`, imaging/recovery modules, destination safety tests |
| Recovery evidence | Mark fixture-validated and experimental support separately; publish only measured results | Claims of broad accuracy, superiority, or real-media validation without evidence | `docs/support-matrix.md`, `docs/GAP_REPORT.md`, `docs/VERIFICATION.md` |
| Sessions | Directory with JSON header and JSONL records | SQLite database and crash-resume claims | `crates/relift-core/src/session.rs`; no SQLite dependency |
| Network and telemetry | No telemetry or automatic update checks; recovery is local | Required accounts, cloud scanning, silent telemetry | No network client in the core/CLI; no update command is implemented |
| Release status | Draft release only; unsigned artifacts | Claiming a published or signed release | Current GitHub release is a draft; no signing credentials are configured |

## Filesystem and format status

| Capability | Selected status | Evidence and limitation |
|---|---|---|
| FAT12/16/32, exFAT, NTFS, ext2/3/4 | Implemented and deterministic-fixture tested | Fixture/end-to-end evidence; no real-volume accuracy corpus |
| HFS+, APFS, Btrfs, XFS | Parser exists; experimental | No deterministic fixture or end-to-end recovery evidence yet |
| Other filesystems | Detection only or unsupported | See `docs/support-matrix.md` and `docs/GAP_REPORT.md` |
| Carving | Emit only formats with structural validators | Validated formats and unimplemented formats are listed in `docs/support-matrix.md` |
| Container images | Raw images only | Convert E01/AFF4/VHD/VMDK/QCOW2/DMG to raw before scanning |
| In-progress resume | Not implemented | A completed session is reusable; there is no `scan --resume` checkpoint workflow |

## Decision records

| ADR | Status |
|---|---|
| ADR-0001 — Canonical name and stack | Accepted; details in this register |
| ADR-0002 — Read-only invariant | Accepted |
| ADR-0003 — Sparse-segment model | Accepted |
| ADR-0004 — Validated carving policy | Accepted |
| ADR-0005 — GUI/TUI decision deferred | Superseded by the implemented Tauri 2 + Svelte 5 GUI; TUI remains deferred |
| ADR-0006 — Exit-code grammar | Accepted; see `docs/CLI.md` |

## Remaining reconciliation work

- Master-prompt variants request more filesystems, formats, interfaces,
  performance targets, packaging channels, SEO work, and launch materials than
  the verified product currently provides. These are not silently treated as
  shipped; see the requirements table in `docs/VERIFICATION.md` and the gap
  report.
- Requirements that depend on physical media, representative labeled corpora,
  signing credentials, third-party registry approvals, or human publication
  remain pending until that evidence or access exists.
