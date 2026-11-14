# Support matrix

Relift's filesystem and platform support. "Tested" means validated against a
deterministic synthetic fixture with SHA-256 byte comparison. "Untested" means
the code path exists but has not been validated against real media.

## Filesystems

| Filesystem | Metadata recovery | Carving | Status | Tested against |
|------------|-------------------|---------|--------|----------------|
| FAT12 | Identified | No | Not implemented | — |
| FAT16 | Identified | No | Not implemented | — |
| FAT32 | Yes | Yes (validators) | Implemented | Synthetic fixture |
| exFAT | Yes | Yes (validators) | Implemented | Synthetic fixture |
| NTFS | Yes | Yes (validators) | Implemented | Synthetic fixture |
| ext2 | Yes | No | Implemented | — (ext4 fixture) |
| ext3 | Yes | No | Implemented | — (ext4 fixture) |
| ext4 | Yes | No | Implemented | Synthetic fixture |
| HFS+ | Identified | No | Not implemented | — |
| APFS | Identified | No | Not implemented | — |
| ReiserFS | Identified | No | Not implemented | — |
| XFS | Identified | No | Not implemented | — |
| Btrfs | Identified | No | Not implemented | — |

Identified = probe recognizes the filesystem and reports it honestly. Not
implemented = no metadata recovery or carving code; reported as unsupported.

## Carving validators (structural validation required)

JPEG, PNG, PDF, ZIP, GIF, BMP, TIFF, MP4, SQLite, RIFF, MP3, Ogg, gzip, 7z,
RAR4, PE, ELF.

Documented carving gaps (not claimed): FLAC, RAR5, ZIP64.

## Platforms

| Platform | Scan from image | Scan from raw device | Recover to destination | Tested |
|----------|-----------------|----------------------|------------------------|--------|
| Windows (GNU Rust) | Yes | Yes (elevation for raw disks) | Yes | Yes |
| Linux | Yes | Yes (root for raw devices) | Yes | Untested |
| macOS | Yes | Untested (privileges) | Fail-closed refusal | Untested |

Raw device reads require elevation on Windows and Linux. macOS raw-device
destination safety intentionally fails closed and is untested.

## Honest gaps

- No real NTFS/ext4 volumes available locally; validation so far is synthetic
  fixtures only.
- ext4 journal / `$UsnJrnl`, NTFS `$ATTRIBUTE_LIST`, named ADS, OLE, encryption,
  bigalloc, journal replay: unsupported, documented.
- Release signing: honest/unsigned unless credentials are provided.
