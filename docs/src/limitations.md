# Known Limitations

This document lists current, verified limitations. It intentionally avoids
claiming features that are not implemented. See `docs/GAP_REPORT.md` for the
tracked work list.

## Filesystem parsing

### NTFS
- `$ATTRIBUTE_LIST` is not followed; only the base record's unnamed `$DATA`
  (resident or non-resident) is recovered.
- Named alternate data streams are not recovered.
- LZNT1 compression and EFS encryption are reported, not decoded.
- `$LogFile` and `$UsnJrnl` are not replayed.
- Hardlinks/reparse points are not resolved; only the first known name is used.
- `$I30` index slack and `$Recycle.Bin` are not parsed as separate sources.

### FAT12/16/32
- A quick format zeroes the FAT; cluster chains are lost. Deleted files are
  assumed contiguous, which can truncate or over-extend output.
- The `0xE5` delete marker destroys the first short-name byte. LFN is used when
  available; otherwise the missing byte becomes `_`.
- Corrupt LFN checksums do not invalidate the decoded name; the checksum is
  treated as informational.

### exFAT
- `NoFatChain` is handled; when a FAT chain is expected but zeroed, recovery
  falls back to contiguous allocation and may be incomplete.
- The allocation bitmap is used for probing, not as a complete free-space map.

### ext2/3/4
- Deletion commonly zeroes extent/block pointers. Recovery uses surviving
  inode fields, directory entries and contiguous-run heuristics.
- Inline data larger than the inode body (>60 bytes) is not recovered.
- fscrypt-encrypted inodes are reported as unverified, not decrypted.
- `bigalloc` is refused as unsupported.
- Journal replay is not performed.

### HFS+, APFS, Btrfs, XFS
- Parsers are implemented and probe fixtures exist, but metadata-based
  deleted-file recovery has no deterministic end-to-end test yet.
- APFS/Btrfs snapshot trees are not enumerated as separate recovery sources.
- APFS clone extents and FileVault-encrypted volumes are not decoded.
- HFS+ journal replay and hardlink/alias resolution are not implemented.
- Btrfs compressed extents (zstd/lzo) are not decompressed.
- XFS inode numbers from B+tree leaves are approximate.

### ReFS / F2FS / ZFS / ReiserFS / ISO 9660 / UDF
- Detection only. Metadata recovery is not implemented; raw carving may still
  find content on the underlying bytes.

## Carving

### Formats without a dedicated validator
- ZIP64: shares the `PK\x03\x04` local-header magic with ZIP and is validated by
  the existing ZIP validator rather than a separate carver. A dedicated ZIP64
  end-of-central-directory validator is not yet implemented.

Every other registered format carries its own structural validator, and an
unvalidated carve is never emitted.

### Fragmentation
- Bifragment gap carving is not implemented for most formats.
- Fragmented MP4/MOV (`moof`/`mdat`) chains are not reassembled.
- JPEG recovery after a large non-JPEG gap is heuristic and may truncate or
  over-extend.

### Limits
Per-format size caps are enforced by each validator; files exceeding the cap
are not carved. See the constants in `crates/relift-core/src/carve.rs`.

## Platform

### Windows
- Raw physical disk access requires administrator elevation.
- Non-elevated runs list drive letters only and warn.
- GUI requires the WebView2 runtime (included in Windows 10 1809+).

### macOS
- Raw-device destination safety fails closed; imaging is recommended.
- Full Disk Access is required for raw devices.
- Raw-device paths have not been exercised in this repository.
- No signing/notarization: Gatekeeper warnings are expected.

### Linux
- Raw block devices normally require root.
- No io_uring or `O_DIRECT` path; reads use ordinary positioned file I/O.
- LVM/RAID reconstruction is not implemented.

## Recovery

### Destination safety
- A destination on the same physical device as the source is refused.
- Network and FUSE destinations are allowed but atomicity is not guaranteed.

### Verification
- SHA-256 is computed by default; `--no-verify` disables the re-read check.
- Failed-output handling differs by platform: Windows removes a failed output;
  Linux and macOS retain a collision-safe `.partial` file when possible.
- Sparse holes are written as zeroes and included in the hash.

### Sessions
- Sessions are directories with `session.json`, `candidates.jsonl`, and
  `recovered.jsonl`; they do not use SQLite.
- Completed sessions can be listed, recovered, and reported later. A cancelled
  or crashed scan can be continued with `relift scan --resume`; the checkpoint
  is volume-level, so the interrupted volume is rescanned from its start.
- Recovery refuses a source whose size or fingerprint no longer matches the
  identity recorded in the session. Scan resume applies the same check.

## Performance

- No published benchmark results exist yet. Run `cargo bench -p relift-bench`
  on your hardware and record the environment described in
  `docs/benches/METHODOLOGY.md`.
- Carving is single-pass and bounded-memory; very large images are processed
  incrementally rather than mapped whole.
