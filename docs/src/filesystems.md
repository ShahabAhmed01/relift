# Filesystem support reference

This document summarizes what the current parsers implement. The status and test evidence are in the [support matrix](support-matrix.md). A parser being present does not establish recovery accuracy on a real volume.

## Fixture-tested parsers

### FAT12, FAT16, FAT32

The parser reads directory entries, long file names, and cluster chains. Deleted data may be incomplete when allocation metadata has been cleared or clusters have been reused. The short-name deletion marker loses its first byte.

### exFAT

The parser reads directory entry sets and uses allocation metadata and the NoFatChain flag. Recovery can be incomplete when the expected FAT chain is unavailable or the file data has been overwritten.

### NTFS

The parser reads MFT records, resident and non-resident unnamed data, runlists, and sparse runs. It does not follow ATTRIBUTE_LIST continuation records, recover named alternate data streams, decompress LZNT1, decrypt EFS, replay the USN journal or LogFile, or parse Recycle Bin metadata as a separate source.

### ext2, ext3, ext4

The parser reads inode metadata, supported block/extent mappings, directory entries, sparse extents, and unwritten extents. It does not replay the journal, recover inline data larger than 60 bytes, decrypt fscrypt data, or support bigalloc. Deletion and reuse can remove the metadata needed to locate content.

## Experimental parsers

HFS+, APFS, Btrfs, and XFS parsers are wired into scanning. Deterministic probe fixtures and end-to-end detection tests exist (`probe_identifies_hfsplus`, `probe_identifies_apfs`, `probe_identifies_btrfs`, `probe_identifies_xfs`), but metadata-based deleted-file recovery has not been validated against a fixture or real volume. Treat recovery results as experimental. Snapshot enumeration, encryption, compression, hardlink resolution, and other filesystem-specific cases are incomplete; see [limitations](limitations.md).

## Detection without metadata recovery

ReFS, F2FS, ZFS, ReiserFS, ISO 9660, and UDF may be identified by probing, but metadata recovery is not implemented.

## Scan mode

Quick mode scans supported filesystem metadata. Deep mode also runs the registered signature validators. The separate `partitions-scan` command searches for lost partition boot-sector signatures and stores its results in a catalog-only session; it does not reconstruct partition tables or recover files directly. See [CLI](CLI.md) for exact options.

## Evidence boundary

Current metadata recovery evidence is from deterministic synthetic FAT32, exFAT, NTFS, and ext4 fixtures. No real physical volume or representative real-media corpus has been validated. For details, use the [support matrix](support-matrix.md), [limitations](limitations.md), and [gap report](GAP_REPORT.md).
