# ADR-0003: Sparse-file segment model

## Status

Accepted.

## Context

Sparse files (NTFS sparse runs, ext holes/unwritten extents) contain logical
regions with no backing clusters. If recovery reads only mapped clusters and
truncates at the last data byte, the recovered file loses its original length and
byte offsets. The output must preserve the original sparse layout.

## Decision

`DataLocation::Segments` with `Segment::Data` / `Segment::Zero`:
- `Segment::Data { extent }` — bytes read from the source at a physical offset.
- `Segment::Zero { length }` — logically zero bytes not present in the source.

NTFS sparse runs and ext holes/unwritten extents are emitted as explicit zero
segments so recovered files retain their original length and byte offsets. The
recovery writer materializes zero segments as zero bytes.

## Consequences

- Recovered sparse files keep their original size.
- Byte offsets are preserved.
- Zero segments are never read from the source (no phantom data).

## Related

- `crates/relift-core/src/candidate.rs` (`Segment`, `DataLocation`)
- `crates/relift-core/src/recover.rs` (`write_zeros`)
- `crates/relift-core/src/fs/ntfs.rs`, `crates/relift-core/src/fs/ext.rs`
