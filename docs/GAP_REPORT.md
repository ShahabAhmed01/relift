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
- `$I30` directory-index slack and `$Recycle.Bin` metadata are not parsed as
  separate sources.

### ext2/3/4
- Inline data larger than the inode body (>60 bytes): not implemented.
- Encryption: encrypted inodes reported as unverified, not decrypted.
- bigalloc: filesystem refused as unsupported.
- Journal replay: not performed; orphan and deleted-inode recovery only.

### HFS+, APFS, Btrfs, XFS
- Parsers are implemented and wired into scan, but there are no deterministic
  fixtures or end-to-end recovery tests for them. Treat output as experimental.
- APFS/Btrfs snapshots are not enumerated as separate recovery sources.
- XFS inode-number reconstruction is approximate for B+tree leaf records.

## Carving gaps

- RAR5: no validator; not carved.
- ZIP64: no validator; not carved.
- OLE compound files (legacy DOC/XLS/PPT): not carved.
- MPEG-TS and ISO 9660: no validated carver.
- Every emitted carve must pass a structural validator; unvalidated carves are
  never emitted.

## Image format gaps

- Only raw images are supported. E01/Ex01, AFF4, VHD/VHDX, VMDK, QCOW2 and
  DMG/UDIF must be converted to raw before scanning.

## Platform gaps

- Windows: raw physical disk enumeration requires elevation. Non-elevated
  sessions list drive letters only with an honest warning.
- Windows ARM64: no build or runtime testing yet.
- macOS: raw-device destination safety fails closed with imaging advice; macOS
  raw paths are untested.
- Linux: raw device access may require root. Destination separation resolves
  partition, stacked-device, and NVMe namespace ancestry through sysfs; virtual
  destinations whose physical backing cannot be proven are refused. Btrfs
  destinations are refused because mount metadata does not establish every
  device in a multi-device filesystem. No io_uring or `O_DIRECT` path.
- GUI bundles are unsigned: no Authenticode certificate, Apple Developer ID or
  notarization credentials are configured.

## Validation gaps

- Recovery path checks are path-based; descriptor-relative no-follow writes are
  not implemented, so concurrent mutation of the destination tree by another
  local process is not prevented.
- No real NTFS/ext4 volumes available locally. All validation so far is against
  deterministic synthetic fixtures (FAT32, exFAT, NTFS, ext4) with SHA-256 byte
  comparison.
- Public test images or real-volume validation still needed before claiming
  real-media recovery.
- No `cargo-fuzz` harness is checked in yet; parser fuzzing is a release gate.
- Code coverage is not measured in CI.

## Interface gaps

- No TUI. The CLI and Tauri desktop GUI are the supported interfaces.
- The GUI exposes stop/cancel but not pause/resume for scans.
- The GUI does not export JSON/CSV/HTML reports; use `relift report` on the
  session directory.

## Release gaps

- No signing certificates or notarization credentials. Releases are unsigned
  unless credentials are provided.
- Homebrew, winget, Scoop, AUR, Flatpak and Snap manifests are not yet
  submitted to their upstream repositories.
- Linux ARM64 is cross-compiled for the CLI only; GUI bundles are x86_64.

## Deliberate exclusions (not bugs)

- No simulated or mock results; every fixture-validated recovery is compared
  byte-for-byte with SHA-256.
- No faked benchmarks, screenshots, stars, reviews, or release claims.
- No telemetry; default offline.
- Unsupported filesystems are identified and reported, never silently claimed.
