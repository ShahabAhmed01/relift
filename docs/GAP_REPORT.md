# Gap report

Documented gaps in Relift's current implementation. Each gap is honest,
deliberate, and tracked.

## Filesystem parsing gaps

### NTFS
- `$ATTRIBUTE_LIST`: multi-attribute records are not followed; only unnamed
  `$DATA` and resident data are recovered.
- Named alternate data streams: not recovered.
- Compression (LZ77) and EFS: not decoded; reported as unsupported when
  encountered.
- `$UsnJrnl`: journal not replayed.

### ext2/3/4
- Inline data larger than the inode body (>60 bytes): not implemented.
- Encryption: encrypted inodes reported as unverified, not decrypted.
- bigalloc: filesystem refused as unsupported.
- Journal replay: not performed; orphan and deleted-inode recovery only.

## Carving gaps

- FLAC: no structural validator; not carved.
- RAR5: no validator; not carved.
- ZIP64: no validator; not carved.

All other carved formats (JPEG, PNG, PDF, ZIP, GIF, BMP, TIFF, MP4, SQLite,
RIFF, MP3, Ogg, gzip, 7z, RAR4, PE, ELF) require a structural validator; an
unvalidated carve is never emitted.

## Platform gaps

- Windows: raw physical disk enumeration requires elevation. Non-elevated
  sessions list drive letters only with an honest warning.
- macOS: raw-device destination safety fails closed with imaging advice; macOS
  raw paths are untested.
- Linux: raw device access may require root.

## Validation gaps

- No real NTFS/ext4 volumes available locally. All validation so far is against
  deterministic synthetic fixtures (FAT32, exFAT, NTFS, ext4) with SHA-256 byte
  comparison.
- Public test images or real-volume validation still needed before claiming
  real-media recovery.

## Release gaps

- No signing certificates / notarization credentials. Releases are honest/
  unsigned unless credentials are provided.
- No GUI/TUI yet. CLI is the only interface. Desktop GUI decision deferred
  (ADR-0005); Rust-native GUI is the documented fallback candidate.

## Deliberate exclusions (not bugs)

- No simulated or mock results; every recovery is fixture-validated.
- No faked benchmarks, screenshots, stars, reviews, or release claims.
- No telemetry; default offline.
- Unsupported filesystems are identified and reported, never silently claimed.
