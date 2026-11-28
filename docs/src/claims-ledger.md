# Relift Claims Ledger

This ledger maps every public claim about Relift to its source-code evidence and
validation status. It exists to keep documentation honest: a claim must be
retracted or corrected when the code or tests no longer support it.

## Evidence levels

| Level | Meaning |
|---|---|
| `fixture-validated` | Exercised by deterministic synthetic fixtures with SHA-256 byte comparison. |
| `probe-validated` | Detection/probe has a deterministic fixture; metadata recovery is not validated. |
| `unit-tested` | Covered by unit tests but not end-to-end with ground-truth bytes. |
| `implemented` | Code exists; not yet covered by tests or fixtures. |
| `not-implemented` | No code; claimed only in documentation (must be corrected). |
| `synthetic-only` | Validated on synthetic fixtures only; no physical media tested. |

## Safety claims

| ID | Claim | Evidence | Level | Location | Verified |
|---|---|---|---|---|---|
| S1 | `ReadOnlySource` exposes no write method | Trait has only `len`, `sector_size`, `read_at`, `identity`, `description`, `read_exact_at`, `read_vec` | fixture-validated | `crates/relift-core/src/source.rs` | Yes |
| S2 | Source is opened read-only on all platforms | `GENERIC_READ` only (Windows), `O_RDONLY` (Linux/macOS) | implemented | `crates/relift-core/src/source.rs` | Yes |
| S3 | Destination on same physical device as source is refused | `validate_destination` resolves ancestry and rejects same-disk | unit-tested | `crates/relift-core/src/safety.rs` | Yes |
| S4 | Output files created with `create_new` (no overwrite) | `CapabilityOpenOptions::create_new(true)` | unit-tested | `crates/relift-core/src/safety.rs` | Yes |
| S5 | Traversal components rejected in output paths | `sanitize_relative` rejects `..`, `.`, absolute, prefix | unit-tested | `crates/relift-core/src/safety.rs` | Yes |
| S6 | Recovery refuses a source whose size/fingerprint differs from session | `run_recovery` compares `size` and `fingerprint` | unit-tested | `crates/relift-core/src/recover.rs` | Yes |
| S7 | Scan resume refuses a changed source | `ensure_resumable` compares kind, size, fingerprint, device_id | unit-tested | `crates/relift-core/src/scan.rs` | Yes |
| S8 | No telemetry or network calls | No network dependencies in Cargo workspace | implemented | `Cargo.toml`, `Cargo.lock` | Yes |

## Recovery correctness claims

| ID | Claim | Evidence | Level | Location | Verified |
|---|---|---|---|---|---|
| R1 | FAT32 deleted files recovered byte-exact | End-to-end test: scan → recover → SHA-256 vs fixture | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| R2 | exFAT deleted files recovered byte-exact | End-to-end test: scan → recover → SHA-256 vs fixture | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| R3 | NTFS deleted files recovered byte-exact | End-to-end test: scan → recover → SHA-256 vs fixture | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| R4 | ext4 deleted files recovered byte-exact | End-to-end test: scan → recover → SHA-256 vs fixture | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| R5 | NTFS deleted non-resident file with reallocated clusters is downgraded to `Partial` | Unit test: `cluster_reuse_downgrades_to_partial` | unit-tested | `crates/relift-core/src/fs/ntfs.rs` | Yes |
| R6 | NTFS deleted non-resident file with free clusters stays `Likely` | Unit test: `cluster_free_keeps_likely` | unit-tested | `crates/relift-core/src/fs/ntfs.rs` | Yes |
| R7 | NTFS unavailable bitmap reports `unknown` | Unit test: `bitmap_unavailable_reports_unknown` | unit-tested | `crates/relift-core/src/fs/ntfs.rs` | Yes |
| R8 | Sparse file holes and unwritten extents keep byte offsets | End-to-end test: `recovers_ext4_fixture_bytes` checks `sparse.bin` | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |

## Carving claims

| ID | Claim | Evidence | Level | Location | Verified |
|---|---|---|---|---|---|
| C1 | 36 registered formats, each with a structural validator | `registered_format_count_is_stable` test asserts 36 unique formats | unit-tested | `crates/relift-core/src/carve.rs` | Yes |
| C2 | Random data produces zero false positives | Unit tests: `random_bytes_rejects_new_carvers`, `random_mz_is_rejected` | unit-tested | `crates/relift-core/src/carve.rs` | Yes |
| C3 | MPEG-TS requires >=4 consecutive valid 188-byte packets | Unit test: `mpegts_rejects_random_sync_bytes` | unit-tested | `crates/relift-core/src/carve.rs` | Yes |
| C4 | Valid MPEG-TS is still carved | Unit test: `valid_mpegts_is_carved` | unit-tested | `crates/relift-core/src/carve.rs` | Yes |
| C5 | Chunk overlap >= max signature length - 1 | Unit test: `overlap_covers_longest_signature` | unit-tested | `crates/relift-core/src/carve.rs` | Yes |
| C6 | Signatures spanning chunk boundaries are found | Unit test: `signatures_at_chunk_boundaries_are_found` | unit-tested | `crates/relift-core/src/carve.rs` | Yes |
| C7 | ZIP64 is not separately emitted | Code comment: shares `PK\x03\x04` magic with ZIP validator | implemented | `crates/relift-core/src/carve.rs` | Yes |
| C8 | Unvalidated carves are never emitted | `validate()` returns `Option`; only `Some` candidates are emitted | implemented | `crates/relift-core/src/carve.rs` | Yes |

## Filesystem detection claims

| ID | Claim | Evidence | Level | Location | Verified |
|---|---|---|---|---|---|
| F1 | HFS+ probe identifies 'H+' at offset 1024 | Unit test: `probe_identifies_hfsplus` | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| F2 | APFS probe identifies 'NXSB' at offset 0 | Unit test: `probe_identifies_apfs` | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| F3 | Btrfs probe identifies `_BHRfS_M` | Unit test: `probe_identifies_btrfs` | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| F4 | XFS probe identifies 'XFSB' | Unit test: `probe_identifies_xfs` | fixture-validated | `crates/relift-core/tests/end_to_end.rs` | Yes |
| F5 | HFS+/APFS/Btrfs/XFS metadata recovery not implemented | `scan_metadata` returns "not implemented" warning | implemented | `crates/relift-core/src/fs/mod.rs` | Yes |

## CLI claims

| ID | Claim | Evidence | Level | Location | Verified |
|---|---|---|---|---|---|
| L1 | Exit codes: 0 OK, 1 RUNTIME, 2 USAGE, 3 UNSAFE, 4 MISMATCH, 5 PARTIAL | Unit tests: `error_exit_codes_are_stable`, `partial_recovery_exit_code_is_stable` | unit-tested | `crates/relift-cli/src/main.rs` | Yes |
| L2 | `--json` produces machine-readable output | Contract test: `scan_json_is_valid_and_recovery_rejects_a_changed_source` | unit-tested | `crates/relift-cli/tests/contract.rs` | Yes |
| L3 | `--resume` continues a cancelled session | Contract test: `scan_resume_continues_a_cancelled_session_and_rejects_a_changed_source` | unit-tested | `crates/relift-cli/tests/contract.rs` | Yes |
| L4 | `--dry-run` validates without writing | Unit test: `dry_run_plans_without_writing_or_mutating_the_session` | unit-tested | `crates/relift-core/src/recover.rs` | Yes |

## Imaging claims

| ID | Claim | Evidence | Level | Location | Verified |
|---|---|---|---|---|---|
| I1 | Image is opened with `create_new` (existing file never overwritten) | Unit tests: `existing_manifest_is_never_overwritten`, `final_image_name_collision_does_not_replace_existing_file` | unit-tested | `crates/relift-core/src/imaging.rs` | Yes |
| I2 | Image SHA-256 verified by re-read | Unit test: `image_roundtrip_and_manifest` | unit-tested | `crates/relift-core/src/imaging.rs` | Yes |
| I3 | Imaging is a single copy pass (no mapfile resume) | Code: single `while at < total` loop; no mapfile support | implemented | `crates/relift-core/src/imaging.rs` | Yes |

## Not yet implemented (documented as gaps)

| ID | Claim | Status | Location |
|---|---|---|---|
| G1 | E01/AFF4/VHD/VMDK/QCOW2/DMG container support | Not implemented; convert to raw first | `docs/src/GAP_REPORT.md` |
| G2 | Persistent imaging mapfile / multi-pass resume | Not implemented; `relift image` is single-pass | `docs/src/GAP_REPORT.md` |
| G3 | TUI (ratatui) | Not implemented | `docs/src/GAP_REPORT.md` |
| G4 | RAID assembly | Not implemented | `docs/src/GAP_REPORT.md` |
| G5 | Encryption unlock (BitLocker/LUKS/VeraCrypt/FileVault) | Not implemented | `docs/src/GAP_REPORT.md` |
| G6 | Signed releases / notarization | Not implemented; releases are unsigned | `docs/src/GAP_REPORT.md` |
| G7 | Package-manager submissions (Homebrew/winget/Scoop/AUR/Flatpak/Snap) | Not submitted; templates only | `docs/src/GAP_REPORT.md` |
| G8 | Real-media validation | Not performed; synthetic fixtures only | `docs/src/GAP_REPORT.md` |

## Claims that must be corrected if code changes

If any of the following change, the corresponding documentation must be updated at the same time:

- The number of registered carve formats (36): `registered_format_count_is_stable` test, `docs/src/formats.md`, `docs/src/support-matrix.md`, README, `llms.txt`, `llms-full.txt`.
- The test count (100): README badge and text.
- Filesystem support levels: `docs/src/support-matrix.md`, `docs/src/limitations.md`, README.
- Exit codes: `docs/src/CLI.md`, README, `crates/relift-cli/src/main.rs`.
- The source read-only invariant: `crates/relift-core/src/source.rs`, `docs/decisions/ADR-0002-read-only-invariant.md`.

## Validation boundary

All validation evidence above is against deterministic synthetic fixtures. No
physical drive, deleted volume, or representative real-media recovery corpus has
been validated. Claims about real-world recovery accuracy (precision, recall,
F1) are **not made** and must not be made until a labelled ground-truth corpus
is established (see `docs/GAP_REPORT.md`).
