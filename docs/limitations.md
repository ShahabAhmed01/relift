# Known Limitations

This document lists the current limitations of Relift. Each is a deliberate design decision or a known gap with a tracked issue.

## Filesystem parsing

### NTFS
- **`$ATTRIBUTE_LIST`**: Multi-attribute records (large files with many fragments) are not followed. Only the base record's unnamed `$DATA` and resident data are recovered.
- **Named alternate data streams (ADS)**: Not recovered. Only the default unnamed `$DATA` stream.
- **Compression (LZNT1)**: Compressed clusters are not decompressed; reported as unsupported when encountered.
- **EFS encryption**: Encrypted files detected via `$DATA` attribute flag; reported as encrypted, not decrypted.
- **`$UsnJrnl`**: USN journal not replayed for historical rename/delete tracking.
- **`$LogFile`**: Transaction log not parsed for metadata rollback.
- **Hardlinks**: Only the first directory entry found is used for path reconstruction.
- **Reparse points / junctions / symlinks**: Not followed; target not recovered.

### FAT12/16/32
- **Zeroed FAT on format**: Quick format zeros the FAT; cluster chains lost. Relift assumes contiguous allocation for deleted files, which may produce false positives or truncated files.
- **First character of deleted name**: `0xE5` marker destroys first byte of 8.3 name. Relift reconstructs from LFN if present, otherwise uses `?` placeholder.
- **LFN checksum mismatch**: If LFN entries corrupted, long name lost.

### exFAT
- **`NoFatChain` flag**: When set, files are contiguous; Relift handles this. When clear, FAT chain followed but may be zeroed.
- **Allocation bitmap**: Used for free-space awareness during carving.

### ext2/3/4
- **Extent tree zeroed on delete**: ext4 typically zeros the extent tree in the inode when a file is deleted. Metadata recovery relies on:
  - Journal (jbd2) for recent deletions
  - Directory entry slack for names
  - Contiguous extent heuristic from block bitmap
- **Inline data > 60 bytes**: Not implemented (stored in inode body).
- **Encryption (fscrypt)**: Encrypted inodes detected; reported as unverified, not decrypted.
- **`bigalloc` (clusters > blocks)**: Filesystem refused as unsupported.
- **Journal replay**: Not performed; orphan list and deleted inodes only.
- **`inline_data` / `ea_inode` / `metadata_csum`**: Partially supported.

### APFS
- **Encryption (FileVault)**: Requires credentials. Without them, volume shows as encrypted; no bypass.
- **Snapshots**: Enumerated; deleted files may exist in older snapshots. Recovery from snapshots implemented.
- **Clone files**: `clone` extent references not fully resolved.
- **Sealed system volume**: macOS system volume is sealed; limited access.

### HFS+
- **Journal**: Not replayed; catalog B-tree only.
- **Hardlinks / aliases**: Not resolved.

### Btrfs
- **RAID5/6**: Mixed block groups detected; recovery may be incomplete.
- **Compression (zstd/lzo)**: Not decompressed; extent references preserved but data not decompressed.
- **Subvolume tree**: Scanned; snapshots enumerated.

### XFS
- **Aggressive inode reuse**: Deleted inodes quickly reused; carving primary path.
- **B+tree complexity**: Best-effort leaf scanning only.

### ReFS / F2FS / ZFS / ReiserFS
- **Identification only**: `probe()` detects superblock signatures; metadata recovery not implemented. Carving works on raw volume.

## Carving limitations

### Formats without validators (never emitted)
- **FLAC**: No structural validator implemented.
- **RAR5**: No validator.
- **ZIP64**: 64-bit extensions not handled.
- **OLE compound files** (legacy DOC/XLS/PPT): No parser.
- **ISO 9660 filesystem carving**: Not implemented.

### Fragmentation handling
- **Bifragment gap carving**: Not implemented for most formats.
- **MP4/MOV fragmented**: `moof`/`mdat` chains not reassembled across fragments without reference file.
- **JPEG after non-JPEG gap**: Heuristic only; may miss or over-extend.

### Size limits
| Format | Max size |
|--------|----------|
| JPEG | 1 GB |
| PNG | 1 GB |
| PDF | 4 GB |
| ZIP | 8 GB |
| MP4 | 16 GB |
| MKV | 64 GB |
| PST/OST | 64 GB |
| TAR | 64 GB |

Files exceeding limits are not carved.

### False positive mitigation
- **Entropy pre-filter**: High-entropy regions (encrypted/compressed) skipped unless `--carve-all`
- **Cluster alignment**: Carving prefers filesystem cluster boundaries when metadata available
- **Validator strictness**: Every format requires structural validation (CRC, Huffman, box tree)

## Platform limitations

### Windows
- **Raw physical disk**: Requires administrator elevation. Non-elevated sessions list drive letters only with warning.
- **System drive (C:)**: Refused unless `--i-know-this-is-my-system-drive` passed.
- **Volume lock**: Not attempted by default; mounted filesystems may change during scan.
- **WebView2**: GUI requires WebView2 Runtime (Windows 10 1809+ includes it).

### macOS
- **Raw device paths**: `/dev/rdiskX` used for performance; `/dev/diskX` blocked by kernel.
- **Full Disk Access**: Required for raw device access. GUI prompts with step-by-step guide.
- **System Integrity Protection (SIP)**: May block access to some volumes.
- **APFS sealed volume**: System volume access restricted.
- **Notarization**: Releases unsigned unless credentials provided; Gatekeeper warning expected.

### Linux
- **Root required**: Raw block device access needs `CAP_SYS_RAWIO` or root.
- **io_uring**: Used when available (kernel 5.1+); falls back to `preadv2`.
- **Device names**: `/dev/sdX`, `/dev/nvmeXnY`, `/dev/mmcblkX` supported.
- **LVM/RAID**: Device-mapper and mdadm members enumerated; logical volumes scanned.

## Recovery limitations

### Destination safety
- **Same physical device**: Refused. Detection by device ID (serial/WWN + partition offset), not path.
- **Network mounts**: Allowed but not recommended; no guarantee of atomicity.
- **FUSE mounts**: Allowed; verification re-read may be slow.

### Verification
- **SHA-256 by default**: Every recovered file hashed; `--no-verify` to disable.
- **Partial files**: Marked `Partial` in report; hash covers only recovered bytes.
- **Sparse holes**: `Segment::Zero` written as zeroes; hash includes zeroes.

### Session resume
- **Kill -9 safe**: SQLite WAL + periodic header flush ensures resume consistency.
- **Source identity**: Session records source device serial, size, partition offset. Resume refuses if mismatch.
- **Cross-machine**: Session directory portable; copy to another machine to resume.

## Performance

### Memory
- **Hard cap**: 350 MB RSS target for 4 TB scan (configurable via `--max-mem`)
- **No full buffering**: Candidates streamed to SQLite; only current chunk in memory.

### Throughput
- **HDD**: Sequential read bound; single reader optimal. `rayon` threads may hurt due to seeks.
- **NVMe**: Up to `num_cpus()` threads; SIMD signature sweep ~4 GB/s/core.
- **SMR drives**: Random reads extremely slow; not optimized for.

## Unsupported scenarios

| Scenario | Recoverable? | Notes |
|----------|--------------|-------|
| SSD with TRIM enabled | Usually **no** | Controller erases blocks; metadata may survive |
| Full format (Windows `format /p`, `dd if=/dev/zero`) | **No** | Sectors zeroed |
| Secure erase / crypto erase | **No** | Hardware-level erasure |
| Overwritten clusters | **No** | Physics |
| Encryption without key | **No** | Math |
| RAID with missing members | Sometimes | Degraded RAID5/6 may reconstruct |
| Physically damaged drive | **No** | Stop; professional service |
| Files deleted months ago on active FS | Unlikely | Metadata reused, clusters reallocated |

## Honest reporting

Relift **never**:
- Claims "100% recovery"
- Shows fake progress bars
- Emits unvalidated carves as recovered
- Hides unsupported filesystems (reports them with warnings)
- Pretends carving recovers original filenames (it doesn't)

Every candidate has a confidence label and reasons. The UI distinguishes:
- **Metadata-recovered** (original name/path/timestamps)
- **Carved-validated** (content verified, name synthetic)
- **Partial/Unverified** (incomplete or uncertain)

## Future work (tracked)

See `docs/ROADMAP.md` and GitHub Issues for:
- RAID 0/1/5 reconstruction
- E01/AFF4 write support
- Custom carver plugins (WASM)
- Encrypted volume recovery (BitLocker/LUKS with credentials)
- Network/remote scanning (iSCSI, SMB)
- Distributed scan for >100 TB images