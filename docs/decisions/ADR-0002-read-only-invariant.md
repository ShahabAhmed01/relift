# ADR-0002: Read-only invariant

## Status

Accepted.

## Context

The product must never write to source media. This must be enforced
architecturally, not by convention, because recovery tools operate on evidence
where any write can destroy data.

## Decision

- `ReadOnlySource` trait exposes **no write method**. All scanner/recovery code
  receives `&dyn ReadOnlySource`.
- Images are opened with `File::open` (read-only).
- Windows raw devices opened `GENERIC_READ` + `FILE_FLAG_OVERLAPPED`.
  `FILE_SHARE_READ | FILE_SHARE_WRITE` is requested only because mounted volumes
  require it; this does **not** grant Relift write permission.
- Unix raw devices opened `O_RDONLY`.
- Recovery output and images are written only below a destination proven separate
  from the source (`safety::validate_destination`).
- Output files opened with `create_new`; never overwrite.
- Destination safety:
  - Image: same-file rejection via canonical path comparison.
  - Windows physical disk: `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` mapping.
  - Linux: `/proc/self/mountinfo` + `rdev` comparison.
  - macOS: raw-dest refusal with imaging advice (fail closed, untested).

## Consequences

- A scanner literally cannot write to the source through the public API.
- Raw-device destination safety fails closed on platforms where separation cannot
  be proven.
- Users must image a device first, then scan the image — this is the recommended
  workflow.

## Related

- `crates/relift-core/src/source.rs`
- `crates/relift-core/src/safety.rs`
