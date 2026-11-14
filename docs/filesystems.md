# Filesystem Reference

## Overview

Relift supports metadata-based deleted-file recovery for several filesystems. Each has different capabilities and limitations.

## FAT12 / FAT16 / FAT32

### How deletion works
- Directory entry first byte changed to `0xE5`
- LFN (long filename) entries marked similarly
- FAT chain **may be zeroed** (Windows) or **retained** (Linux `unlink`)
- Clusters marked free in FAT

### What Relift recovers
| Scenario | Name | Path | Content | Notes |
|----------|------|------|---------|-------|
| Normal delete (FAT retained) | ✓ LFN | ✓ | ✓ | Best case |
| Windows delete (FAT zeroed) | ✓ LFN | ✓ | ✓* | *Contiguous heuristic |
| Quick format | ✓ LFN | ✓ | ✓* | FAT zeroed, data intact |
| Partial overwrite | ✓ LFN | ✓ | Partial | Depends on reuse |

### Implementation details
- Parses both FAT copies; uses consensus
- LFN checksum validation
- Reconstructs first character of 8.3 name from sibling entries
- Handles `0x05` → `0xE5` Kanji edge case
- Deleted directory traversal

### Limitations
- No journal; no history beyond current FAT state
- Fragmented files with zeroed FAT → guessed contiguous (confidence downgraded)
- No encryption support

## exFAT

### How deletion works
- Directory entry set type `0x40` (file) / `0xC0` (directory) — deleted flag
- Allocation bitmap marks clusters free
- `NoFatChain` flag: file is contiguous (no FAT chain)

### What Relift recovers
| Scenario | Name | Path | Content | Notes |
|----------|------|------|---------|-------|
| Normal delete | ✓ | ✓ | ✓ | Bitmap + contiguous |
| Quick format | ✓ | ✓ | ✓ | Bitmap zeroed, data intact |

### Implementation details
- Parses up-case table for correct Unicode
- Handles stream extension entries (custom attributes)
- Allocation bitmap cross-check for confidence

### Limitations
- No journal
- No encryption support (BitLocker on exFAT not parsed)

## NTFS

### How deletion works
- MFT record `in-use` flag cleared (bit 0 of flags)
- `$FILE_NAME`, `$STANDARD_INFORMATION`, `$DATA` attributes remain until record reused
- `$BITMAP` marks clusters free
- `$LogFile` and `$UsnJrnl:$J` may retain history

### What Relift recovers
| Source | Name | Path | Timestamps | Content | Confidence |
|--------|------|------|------------|---------|------------|
| Deleted MFT record | ✓ | ✓ | ✓ | ✓ | Verified (95-100) |
| `$I30` INDEX slack | ✓ | ✓ | Partial | ✓ | Likely (80-90) |
| `$LogFile` redo/undo | ✓ | ✓ | ✓ | ✓ | Verified (90) |
| `$UsnJrnl:$J` | ✓ | ✓ | ✓ | — | Likely (85) |
| Resident `$DATA` | ✓ | ✓ | ✓ | ✓ | Verified (100) |
| `$Recycle.Bin` `$I`/`$R` | ✓ | ✓ | ✓ (deletion time) | ✓ | Verified (100) |

### Implementation details
- Parses `$MFT` (record 0) to get its own runlist → reads fragmented MFT
- USA fixup validation on every record
- Attribute parsing: `$STANDARD_INFORMATION` (0x10), `$FILE_NAME` (0x30), `$DATA` (0x80), `$ATTRIBUTE_LIST` (0x20), `$INDEX_ROOT` (0x90), `$INDEX_ALLOCATION` (0xA0)
- Resident data (≤ 700 bytes typical) recovered directly from MFT
- Non-resident: runlist decoding (VLC encoding)
- Path reconstruction via parent MFT reference (`$FILE_NAME` parent FRN)
- `$Bitmap` cross-check: if cluster reallocated → confidence downgraded
- `$Recycle.Bin`: parses `$I` (metadata) + `$R` (content) pairs, restores original name/path

### Limitations (see `docs/GAP_REPORT.md`)
- `$ATTRIBUTE_LIST` continuation records not followed
- Named ADS not recovered
- Compression (LZNT1) not decompressed
- EFS encrypted files detected, not decrypted
- `$UsnJrnl` not replayed
- WOF/XPRESS Huffman not decompressed

## ext2 / ext3 / ext4

### How deletion works
- Inode `i_links_count` set to 0, `i_dtime` set to deletion time
- **ext2/3**: Block pointers remain in inode (direct/indirect/double/triple)
- **ext4**: Extent tree **zeroed** in inode (major limitation)
- Block/inode bitmaps mark blocks/inodes free
- **ext3/4**: Journal (jbd2) may retain old inode copies

### What Relift recovers
| Source | Name | Path | Content | Notes |
|--------|------|------|---------|-------|
| ext2/3 inode | ✓ | ✓* | ✓ | *Via directory slack |
| ext4 journal | ✓ | ✓* | ✓* | Recent deletions |
| Directory slack | ✓ | ✓ | — | Names only |
| Orphan inode list | — | — | ✓ | No name/path |

*ext4 extent tree zeroed on delete; content recovery via journal or contiguous heuristic

### Implementation details
- Superblock + backup superblocks (64-bit, `meta_bg`, `flex_bg`, `bigalloc`)
- Inode table scan for `i_links_count == 0`
- Extent tree parsing (0xF30A magic) — but typically zeroed
- Journal (jbd2) scan for deleted inode references
- Directory entry linear scan + htree for stale entries
- Inline data (`i_block` stores data directly) up to 60 bytes
- Sparse/unwritten extents → `Segment::Zero` in output

### Limitations
- ext4 extent zeroing = metadata undelete rarely works
- Journal replay not implemented (orphan list only)
- Inline data > 60 bytes not recovered
- Encryption (fscrypt) detected, not decrypted
- `bigalloc` refused
- No `$UsnJrnl` equivalent

## APFS (Apple File System)

### How deletion works
- Copy-on-write: old B-tree nodes persist until overwritten
- **Snapshots** frequently retain deleted files
- Object map tracks allocated objects
- FileVault encryption optional

### What Relift recovers
| Source | Name | Path | Content | Notes |
|--------|------|------|---------|-------|
| Live B-tree | ✓ | ✓ | ✓ | If not overwritten |
| Snapshot B-tree | ✓ | ✓ | ✓ | Best for deleted files |
| Ephemeral objects | Partial | Partial | Partial | Short-lived |

### Implementation details
- Container superblock (NXSB) → volume superblock (APSB)
- B-tree traversal: file/directory records with `INODE_IS_UNUSED` flag
- Snapshot enumeration via snapshot metadata tree
- File extent parsing (direct + indirect)
- FileVault: prompts for credentials; no bypass

### Limitations
- Encryption without credentials = no recovery
- Sealed system volume (macOS) restricted
- Clone extent references not fully resolved
- Compression not decompressed

## HFS+ (Mac OS Extended)

### How deletion works
- Catalog B-tree: deleted nodes marked, not immediately removed
- Extents overflow B-tree retains extent descriptors
- Journal may retain history

### What Relift recovers
| Source | Name | Path | Content |
|--------|------|------|---------|
| Catalog deleted nodes | ✓ | ✓ | ✓ |
| Extents overflow | — | — | ✓ |
| Journal | Partial | Partial | ✓ |

### Implementation details
- Volume header (0x482B / 0x4858 magic)
- Catalog B-tree traversal (deleted node flag)
- Extents overflow B-tree for extent descriptors
- Journal scan (best-effort)

### Limitations
- Journal replay not implemented
- Compression not supported
- FileVault not handled

## Btrfs

### How deletion works
- Copy-on-write: old tree roots persist
- Subvolumes/snapshots retain deleted files
- Object references tracked

### What Relift recovers
| Source | Name | Path | Content |
|--------|------|------|---------|
| Current root | ✓* | ✓* | ✓* |
| Old roots / snapshots | ✓ | ✓ | ✓ |

*If not yet overwritten by CoW

### Implementation details
- Superblock (`_BHRfS_M` magic) at 64 KiB + 16 MiB
- Chunk tree → device extents → logical → physical mapping
- Root tree + old roots (generation-based)
- Subvolume/snapshot enumeration
- File/directory items with `BTRFS_INODE_ITEM_KEY`
- Extent items (`BTRFS_EXTENT_ITEM_KEY`)

### Limitations
- RAID5/6 parity not reconstructed
- Compression (zstd/lzo) not decompressed
- Encryption not supported
- Skinny metadata extents partially supported

## XFS

### How deletion works
- Inode B+tree: deleted inodes freed quickly
- Directory entries in B+tree
- Aggressive inode reuse

### What Relift recovers
| Source | Name | Path | Content |
|--------|------|------|---------|
| Deleted inode (rare) | ✓ | ✓ | ✓ |
| Carving | — | — | ✓ |

### Implementation details
- AG headers → inode B+tree → directory B+tree
- Inode core parsing (mode, timestamps, extent list)
- Directory entry parsing (shortform + block + leaf)
- Carve-first strategy recommended

### Limitations
- Inode reuse very fast; metadata undelete rarely works
- RAID not handled
- Real-time subvolume not supported

## Comparison table

| Feature | FAT32 | exFAT | NTFS | ext4 | APFS | Btrfs | XFS |
|---------|-------|-------|------|------|------|-------|-----|
| Deleted name | ✓ | ✓ | ✓ | ✓* | ✓ | ✓ | ✓ |
| Original path | ✓ | ✓ | ✓ | ✓* | ✓ | ✓ | ✓ |
| Timestamps | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Resident data | N/A | N/A | ✓ | ✓ (inline) | N/A | N/A | N/A |
| Sparse files | — | — | ✓ | ✓ | — | — | — |
| Journal history | — | — | ✓ | Partial | — | Snapshots | — |
| Recycle Bin | — | — | ✓ | — | Trash | — | — |
| Snapshots | — | — | VSS | — | ✓ | ✓ | — |
| Compression | — | — | Partial | — | — | Not decoded | — |
| Encryption | — | — | Detected | Detected | FileVault | — | — |
| RAID | — | — | LDM/Spaces | mdadm/LVM | — | Native | mdadm |

*ext4: only via journal/directory slack

## Choosing a scan mode

| Situation | Recommended mode |
|-----------|------------------|
| Recently deleted, FS intact | Quick (metadata) |
| Formatted / quick format | Deep (metadata + carve) |
| Deleted partition | Lost partition |
| SSD with TRIM | Quick (metadata only; carving unlikely) |
| Corrupted FS / RAW | Deep (carve) |
| Failing drive | Image first → Deep on image |

## Adding a new filesystem

See `CONTRIBUTING.md` → "Adding a new filesystem parser".