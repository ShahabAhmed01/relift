# Verification

Relift validates recovery against deterministic synthetic filesystem fixtures
with SHA-256 byte comparison. No simulated or mock results are ever presented as
recovery.

## How verification works

1. A fixture builder constructs a real, structurally valid filesystem image
   (FAT32, exFAT, NTFS, ext4) byte by byte.
2. Each fixture file has an independently computed SHA-256 digest.
3. Relift scans the image, recovers deleted files, and the test reads the output
   bytes back.
4. The recovered bytes are compared byte-for-byte and by SHA-256 against the
   fixture's independent expectation.

## What is verified

| Filesystem | Fixture | Live files | Deleted files | Sparse/holes | Unwritten extents | Fragmented | Orphan inodes |
|------------|---------|------------|---------------|--------------|-------------------|------------|---------------|
| FAT32 | `fat32_image()` | Yes | Yes | — | — | — | — |
| exFAT | `exfat_image()` | Yes | Yes | — | — | — | — |
| NTFS | `ntfs_image()` | Yes | Yes | Yes | — | Yes | — |
| ext4 | `ext4_image()` | Yes | Yes | Yes | Yes | Yes | Yes |

Sparse files (NTFS `sparse.bin`, ext4 `sparse.bin`) are recovered with an
explicit `Segment::Zero` hole. Unwritten extents (ext4 `unwritten.bin`) are
emitted as zeroes even though the physical block contains nonzero garbage.

## Running the tests

```
cargo test -p relift-core
```

Includes:
- 43 unit tests (parsers, safety, hashing, sessions, carving, partitions).
- 6 end-to-end tests (scan → recover → SHA-256 verify for FAT32, exFAT, NTFS,
  ext4; destination separation; output confinement).

## Honesty policy

- No simulated/mock results.
- No faked benchmarks, screenshots, stars, reviews, or release claims.
- Publish only what is actually verified.
- No real-volume validation claimed for NTFS/ext until public images prove it.
