# ADR-0004: Carving policy — structurally validated carves only

## Status

Accepted.

## Context

Signature carving scans raw bytes for file magic numbers. Naive carving emits
every byte range that looks like a file header, producing many false positives
and files that do not actually parse. Relift only emits structurally validated
carves.

## Decision

A carved candidate is emitted only when all of the following hold:
- An end-marker, CRC, EOCD (ZIP central directory), box-chain (MP4/MOV), or
  format-specific validator passes.
- A format-specific size cap is respected (prevents carving gigabytes from a
  single header).
- The carved byte range is validated end-to-end by the format's own structure
  (e.g., JPEG SOI→EOI, PNG IEND with CRC, ZIP local+EOCD, SQLite header+page
  size).

Carved candidates use `RecoveryMethod::Carved`, `EntryState::Carved`, and are
marked `confidence` according to validator strength. Unsupported carving targets
(FLAC, RAR5, ZIP64) are documented gaps, never silently claimed.

## Consequences

- Fewer false positives; carved output is trustworthy.
- Some carvable formats are not supported until a validator is written.
- Carving is opt-in (`--carve`) and separate from metadata recovery.

## Related

- `crates/relift-core/src/carve.rs`
- `docs/GAP_REPORT.md`
