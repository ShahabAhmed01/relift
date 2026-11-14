# Architecture

Relift is a read-only deleted-file recovery tool with a shared engine used by
CLI (first-class), with TUI/GUI deferred as optional consumers.

## Crates

| Crate | Purpose | Builds binary? |
|-------|---------|----------------|
| `relift-core` | Recovery engine: sources, filesystems, scanning, carving, recovery writer, imaging, reports, sessions | No (library) |
| `relift-cli` | `relift` binary: `scan`, `recover`, `image`, `devices`, `doctor`, `report`, `version` | Yes (`relift`) |
| `relift-testkit` | Deterministic synthetic filesystem fixtures (FAT32, exFAT, NTFS, ext4) | No (library, dev-dep only) |

Dependency rule: `relift-core` dev-depends on `relift-testkit`; testkit must not
depend on core. `relift-cli` depends on `relift-core`.

## Read-only invariant

`ReadOnlySource` (in `relift-core/src/source.rs`) exposes no write method. All
recovery code receives `&dyn ReadOnlySource`. Images open read-only via
`File::open`; Windows raw devices open `GENERIC_READ` + `FILE_FLAG_OVERLAPPED`;
Unix raw devices open `O_RDONLY`.

## Data model

- `Candidate` — one recoverable item: name, size, format, `DataLocation`,
  `RecoveryMethod`, `Confidence`, `EntryState`, timestamps, validation notes.
- `DataLocation` — `Extents`, `Segments` (with `Segment::Data` /
  `Segment::Zero` for sparse/holes/unwritten extents), `Resident`, `Raw`,
  `None` (metadata only).
- `Segment` — preserves original file length and byte offsets for sparse files.
- `Session` — append-only JSONL candidate stream + header; streamed, not loaded
  entirely into memory.
- `SourceIdentity` — size + SHA-256 fingerprint over head/tail bytes; used to
  refuse recovery against a changed image.

## Filesystems (first recovery targets)

- FAT12/16/32 — metadata recovery + carving
- exFAT — metadata recovery + carving
- NTFS — MFT records, resident/non-resident/sparse $DATA, deleted-record detection
- ext2/3/4 — superblock, group descriptors, inode table, extent trees,
  direct/indirect blocks, deleted-inode scan

Unsupported filesystems are probed and reported, never silently claimed.

## Carving

Signature carving with structural validators (end-marker/CRC/EOCD/box-chain) and
format-specific size caps. Only validated carves are emitted.

## Destination safety

Output is written only below a destination proven separate from the source:
- Image: canonical same-file rejection.
- Raw device (Windows/Linux): physical-disk/mount mapping.
- macOS: fail-closed refusal with imaging advice.
- Every output file opened with `create_new`; sanitized and confined to the
  destination root.

## Imaging

Source copied to `<dest>.partial` with `create_new`, flushed, renamed to
`<dest>`, then re-read and SHA-256 verified against the copy stream. A
sidecar `.relift.json` manifest records source identity, bytes, digest, and
verification result.

## Sessions and reports

Sessions are append-only JSONL (`candidates.jsonl`, `recovered.jsonl`) plus a
header (`session.json`). Reports (JSON/HTML/CSV) stream from session files and
escape all filesystem-derived values.

## CLI exit codes

| Code | Meaning |
|------|---------|
| 0 | OK |
| 1 | RUNTIME |
| 2 | USAGE |
| 3 | UNSAFE (destination safety refusal) |
| 4 | MISMATCH (source != session) |
| 5 | PARTIAL |

## Honesty policy

- No simulated/mock results; synthetic fixtures with SHA-256 comparison.
- No faked benchmarks, screenshots, stars, reviews, or release claims.
- Publish only what is verified.
- Default offline; no telemetry.
