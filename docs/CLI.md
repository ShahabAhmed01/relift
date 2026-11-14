# Relift CLI

Reference for the `relift` command-line interface: commands, options, output,
and exit codes.

## Canonical name and stack

- Tool binary: `relift`
- Core crate: `relift-core`
- Test fixtures: `relift-testkit`
- License: GPL-3.0-or-later

## Synopsis

```
relift [--json] <command>
```

`--json` (global) makes the command print machine-readable JSON to stdout
instead of human-readable text. Progress is always written to stderr.

## Commands

### `relift scan <source> --session <dir>`

Scan a disk image or raw device for deleted files and write candidates to a
session directory.

```
relift scan disk.img --session ./session
relift scan \\.\PhysicalDrive1 --session ./session --live --carve
relift scan image.dd --session ./session --offset 0x1000 --length 0x100000
```

Options:
- `<source>` — disk image path or raw device path (`\\.\PhysicalDriveN` on
  Windows, `/dev/sdX` on Linux/macOS). Raw devices are opened `GENERIC_READ`
  only.
- `--session DIR` — new or empty directory for session output (required).
- `--live` — also enumerate live (still-allocated) files; default is deleted
  files only.
- `--carve` — run signature carving in addition to metadata recovery.
- `--volume INDEX` — scan only this volume index (repeatable).
- `--offset BYTES` / `--length BYTES` — manual volume range (both required).
- `--max-candidates N` — stop after N candidates (default 1,000,000).

### `relift recover <session> --dest <dir>`

Recover candidates from a session into a destination directory. The source is
re-opened and its identity is checked against the session; a changed image is
refused.

```
relift recover ./session --dest ./recovered
relift recover ./session --dest ./recovered --source disk.img --min-confidence likely --id 3 --id 7
```

Options:
- `<session>` — session directory produced by `relift scan` (required).
- `--dest DIR` — destination directory, separate from the evidence (required).
- `--source PATH` — re-open this source instead of the session-recorded path.
- `--no-verify` — skip the post-write re-read SHA-256 verification.
- `--min-confidence LABEL` — skip candidates below `verified`, `likely`,
  `partial`, or `unverified`.
- `--id N` — recover only these candidate ids (repeatable).

### `relift image <source> --dest <file>`

Copy a source to a new image file with a SHA-256 manifest sidecar. The
destination is opened with `create_new`; existing files are never overwritten.

```
relift image \\.\PhysicalDrive1 --dest ./image.dd
relift image image.dd --dest ./image-verified.dd --no-verify
```

Options:
- `<source>` — disk image or raw device path (required).
- `--dest FILE` — new image file to create (required).
- `--no-verify` — skip re-read verification of the finished image.

### `relift devices`

List volumes and readable disks exposed by the operating system. Raw physical
disk enumeration requires elevation on Windows.

```
relift devices
```

### `relift doctor`

Run self-tests: SHA-256 known-answer test, temporary storage write, carving
validator (synthetic SQLite), raw device access, and network/telemetry
confirmation (there is none).

```
relift doctor
```

### `relift report <session>`

Render a saved session as JSON, HTML, or CSV.

```
relift report ./session --format html --out ./report.html
relift report ./session --format csv --limit 50
```

Options:
- `<session>` — session directory (required).
- `--format FORMAT` — `json`, `html`, or `csv` (default `json`).
- `--out FILE` — output file (default `<session>/report.<format>`).
- `--limit N` — maximum candidates to list (0 means all).

### `relift version`

Print version and schema information.

```
relift version
relift version --json
```

## Exit codes

These are part of the CLI contract and are stable across releases.

| Code | Name | Meaning |
|------|------|---------|
| 0 | OK | Command completed successfully. |
| 1 | RUNTIME | Generic runtime error (I/O, parse failure, etc.). |
| 2 | USAGE | Command-line usage error (bad flags, missing args). |
| 3 | UNSAFE | Destination safety refusal (destination overlaps evidence). |
| 4 | MISMATCH | Source does not match the session (re-scan required). |
| 5 | PARTIAL | Command completed but results are partial (failed/partial/cancelled). |

A command that reports `PARTIAL` (exit 5) still produced valid output, but some
candidates failed or were incomplete. Inspect the session/recovery summary for
details.

## Read-only guarantee

Relift never writes to a source. `ReadOnlySource` exposes no write method.
Images are opened with `File::open` (read-only). Raw devices are opened
`GENERIC_READ` only on Windows, `O_RDONLY` on Unix. The recovery writer and
imager write only below a destination that was proven separate from the source.

## Examples

Image a raw device, then scan the image:

```
relift image \\.\PhysicalDrive1 --dest D:\images\evidence.dd
relift scan D:\images\evidence.dd --session D:\session
relift recover D:\session --dest D:\recovered
```

Scan with carving and live files:

```
relift scan image.dd --session ./session --live --carve --max-candidates 50000
```

Recover only high-confidence deleted candidates:

```
relift recover ./session --dest ./out --min-confidence likely
```

## JSON output shape

With `--json`, every command emits a single JSON document to stdout. The exact
schema mirrors the corresponding Rust structs (`SessionHeader`, `RecoverySummary`,
`ImageResult`, `DeviceList`, `DoctorReport`, `ReportInfo`) and is versioned by
the session schema (`SCHEMA_VERSION = 1`).
