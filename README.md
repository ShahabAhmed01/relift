# Relift

Read-only deleted-file recovery for disk images and drives.

Relift scans disk images and raw devices without writing to them, records what
it finds in a session directory, and recovers validated byte ranges only to a
separate destination. Unsupported filesystems and unvalidated carves are
reported, never silently presented as recovered.

## Canonical identity

- **Name:** Relift (binary `relift`)
- **License:** GPL-3.0-or-later
- **Stack:** Rust 1.100.0 GNU toolchain
- **Crates:** `relift-core`, `relift-cli`, `relift-testkit`

## Features

- Read-only-by-construction: `ReadOnlySource` exposes no write method.
- Real byte-level recovery, fixture-validated with SHA-256 comparison.
- FAT32, exFAT, NTFS, ext2/3/4 metadata recovery.
- Signature carving with structural validators (JPEG, PNG, PDF, ZIP, GIF, BMP,
  TIFF, MP4, SQLite, RIFF, MP3, Ogg, gzip, 7z, RAR4, PE, ELF).
- Sparse-file segment preservation (holes and unwritten extents keep byte
  offsets).
- Destination safety: output proven separate from evidence before any write.
- SHA-256 verification of every recovered file (on by default).
- Session-based workflow: scan → session → recover → report (JSON/HTML/CSV).
- `devices` and `doctor` self-tests.
- No telemetry; default offline.

## Quick start

Image a raw device, then scan the image (recommended over scanning a mounted
volume):

```sh
relift image \\.\PhysicalDrive1 --dest D:\images\evidence.dd
relift scan D:\images\evidence.dd --session D:\session
relift recover D:\session --dest D:\recovered
```

Scan an image directly:

```sh
relift scan image.dd --session ./session --live --carve
relift recover ./session --dest ./out
```

List devices:

```sh
relift devices
```

Run self-tests:

```sh
relift doctor
```

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | OK |
| 1 | RUNTIME |
| 2 | USAGE |
| 3 | UNSAFE (destination safety refusal) |
| 4 | MISMATCH (source != session) |
| 5 | PARTIAL |

See `docs/CLI.md` for the full command reference.

## Documentation

- `docs/CLI.md` — command and exit-code reference
- `docs/architecture.md` — system design
- `docs/support-matrix.md` — tested/untested filesystem and platform matrix
- `docs/threat-model.md` — threats and mitigations
- `docs/limitations.md` — known gaps and exclusions
- `docs/GAP_REPORT.md` — tracked gaps
- `docs/VERIFICATION.md` — how recovery is validated
- `docs/decisions/CONFLICT_REGISTER.md` — reconciled names/stacks/licenses
- `docs/decisions/ADR-0001…0006` — architecture decision records
- `docs/research/landscape.md` — pre-architecture research

## Building

Requires Rust 1.100.0 (GNU toolchain) via rustup:

```sh
cargo build --release
```

The release binary is `target/release/relift.exe`.

## Tests

```sh
cargo test -p relift-core
```

## Honesty policy

Relift publishes only what is actually verified. No simulated results, no
faked benchmarks/screenshots/stars/reviews/release claims, no telemetry.
Unsupported filesystems are identified and reported, never silently claimed.

## License

GPL-3.0-or-later. See `LICENSE`.
