# Limitations

Relift is honest about what it cannot do. This document lists known gaps and
deliberate exclusions.

## Filesystem gaps

| Area | Status | Detail |
|------|--------|--------|
| NTFS `$ATTRIBUTE_LIST` | Unsupported | Multi-attribute records not followed |
| NTFS named alternate data streams | Unsupported | Only unnamed `$DATA` recovered |
| NTFS compression / EFS | Unsupported | Reported, not decoded |
| NTFS `$UsnJrnl` | Unsupported | Journal not replayed |
| ext4 inline data > inode body | Unsupported | Tiny inline data (≤60 bytes) recovered inline |
| ext4 encryption | Unsupported | Encrypted inodes reported as unverified |
| ext4 bigalloc | Unsupported | Filesystem refused as unsupported |
| ext4 journal replay | Unsupported | Orphan/deleted recovery only, no journal |
| HFS+, APFS, ReiserFS, XFS, Btrfs | Not implemented | Identified and reported honestly |

## Carving gaps

FLAC, RAR5, ZIP64 carving are not supported. All other carved formats require a
structural validator; unvalidated carves are never emitted.

## Platform gaps

- Windows raw disk enumeration requires elevation (non-elevated sees drive
  letters only, with an honest warning).
- macOS raw-device destination safety fails closed; macOS raw paths untested.
- Linux raw device access may require root.

## Validation gaps

- No real NTFS/ext4 volumes available locally; validation is via deterministic
  synthetic fixtures with SHA-256 comparison.
- Public test images or real-volume validation still needed for production
  claims about real media.

## Release gaps

- No signing certificates / notarization credentials; releases are honest/
  unsigned unless credentials are provided.
- No GUI/TUI yet; CLI is the only interface. Desktop GUI decision deferred
  (ADR-0005).

## What Relift does not claim

- No recovery of files from unsupported filesystems.
- No carving of formats without a structural validator.
- No benchmarks, screenshots, stars, reviews, or release claims that are not
  actually verified.
- No telemetry; default offline.
