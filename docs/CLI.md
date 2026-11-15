# CLI Reference

## Global options

```
relift [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--json` | Emit machine-readable JSON on stdout |
| `--help` | Show help |
| `--version` | Print version |

## Commands

### `relift devices`

List physical disks and partitions exposed by the operating system.

```
relift devices [--json]
```

### `relift scan`

Probe filesystems and write deleted-file candidates to a session directory.

```
relift scan <SOURCE> --session <DIR> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SOURCE>` | Disk image path or raw device (e.g., `image.dd`, `\\.\PhysicalDrive1`) |
| `--session <DIR>` | New or empty directory for session output (required) |
| `--mode <MODE>` | Scan preset: `quick` (metadata only) or `deep` (metadata + validating carving) (default: `quick`) |
| `--live` | Also list files that are still allocated |
| `--carve` | Run signature carving in addition to metadata recovery (implied by `--mode deep`) |
| `--volume <INDEX>` | Scan only this volume index (repeatable) |
| `--offset <BYTES>` | Manual volume offset in bytes (requires `--length`) |
| `--length <BYTES>` | Manual volume length in bytes (requires `--offset`) |
| `--max-candidates <N>` | Stop after this many candidates (default: 1,000,000) |
| `--json` | Machine-readable JSON output |

**Examples:**

```sh
# Quick metadata scan (never reads the whole volume)
relift scan /dev/sdb --session ./session

# Deep scan with validating signature carving
relift scan image.dd --session ./session --mode deep

# Scan only a specific volume index
relift scan /dev/sdb --session ./session --volume 2

# Manual volume range
relift scan image.dd --session ./session --offset 1048576 --length 1000000000
```

### `relift list`

List candidates saved in a session without writing anything.

```
relift list <SESSION> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SESSION>` | Session directory produced by `relift scan` |
| `--filter <EXPR>` | Filter candidates (repeatable): `ext:jpg,png`, `format:pdf`, `name:IMG_`, `path:DCIM` |
| `--min-confidence <LABEL>` | Hide candidates below this confidence (`verified`, `likely`, `partial`, `unverified`) |
| `--limit <N>` | Maximum rows to print (0 means all) (default: 100) |
| `--json` | Machine-readable JSON output |

**Filter expressions:**

```
ext:jpg,png,mp4          # by file extension or filename extension
format:pdf               # by recorded candidate format
name:IMG_                # case-insensitive filename substring
path:DCIM                # case-insensitive path substring
```

Multiple `--filter` expressions are ANDed; comma-separated values within one
expression are ORed.

**Examples:**

```sh
# List all candidates (first 100)
relift list ./session

# Only verified JPEGs and PNGs
relift list ./session --filter 'ext:jpg,png' --min-confidence verified

# JSON output
relift list ./session --json
```

### `relift recover`

Recover candidates from a session into a destination directory. The source's
identity must still match the session record; a changed image is refused.

```
relift recover <SESSION> --dest <DIR> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SESSION>` | Session directory produced by `relift scan` |
| `--dest <DIR>` | Destination directory (must be on a different physical device) |
| `--source <PATH>` | Re-open this source instead of the path recorded in the session |
| `--filter <EXPR>` | Filter candidates (repeatable): same expressions as `relift list` |
| `--dry-run` | Plan only: validate source and destination, list what would be written. No files are written and the session is not modified. |
| `--no-verify` | Do not re-read each output file after writing it (verification is on by default) |
| `--min-confidence <LABEL>` | Skip candidates below this confidence (`verified`, `likely`, `partial`, `unverified`) |
| `--id <N>` | Recover only these candidate IDs (repeatable) |
| `--json` | Machine-readable JSON output |

**Examples:**

```sh
# Recover all candidates
relift recover ./session --dest /mnt/recovery

# Only verified JPEGs, dry run first
relift recover ./session --dest /mnt/recovery --filter 'ext:jpg' --min-confidence verified --dry-run

# Recover specific IDs only
relift recover ./session --dest /mnt/recovery --id 123 --id 456

# Without post-write hash verification
relift recover ./session --dest /mnt/recovery --no-verify
```

### `relift image`

Copy a source to a new image file and write a SHA-256 manifest. Existing files
are never overwritten.

```
relift image <SOURCE> --dest <FILE> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SOURCE>` | Disk image or raw device path |
| `--dest <FILE>` | New image file to create (never overwrites existing) |
| `--no-verify` | Do not re-read the finished image to verify its hash |
| `--json` | Machine-readable JSON output |

**Example:**

```sh
relift image /dev/sdb --dest /mnt/backup/drive.img
```

### `relift doctor`

Run self-tests for hashing, carving, storage and raw access.

```
relift doctor [--json]
```

### `relift report`

Render a saved session as JSON, HTML or CSV.

```
relift report <SESSION> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SESSION>` | Session directory produced by `relift scan` |
| `--format <FORMAT>` | Report format: `json`, `html` or `csv` (default: `json`) |
| `--out <FILE>` | Output file (defaults to `<session>/report.<format>`) |
| `--limit <N>` | Maximum candidates to list (0 means all) (default: 0) |
| `--json` | Machine-readable JSON output |

### `relift version`

Print version and schema information.

```
relift version [--json]
```

### `relift completions`

Generate shell completions for your shell.

```
relift completions <SHELL>
```

Supported shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

**Example:**

```sh
relift completions bash > /etc/bash_completion.d/relift
relift completions powershell > relift.ps1
```

## Exit codes

| Code | Constant | Meaning |
|------|----------|---------|
| 0 | `EXIT_OK` | Success |
| 1 | `EXIT_RUNTIME` | Runtime error |
| 2 | `EXIT_USAGE` | Usage/argument error |
| 3 | `EXIT_UNSAFE` | Destination safety refusal (same physical device) |
| 4 | `EXIT_MISMATCH` | Source mismatch (session source != actual source) |
| 5 | `EXIT_PARTIAL` | Partial (some files failed or were incomplete) |

## Planned but not yet implemented

The following are documented in `AGENTS.md` as planned work and are **not**
part of the current CLI:

- `relift info`, `relift verify`, `relift session`, `relift self-update`
- `--mode formatted`, `--mode lost-partition`, `--resume` on `scan`
- `--quiet`, `--dry-run` on `scan`
- `--preserve-paths`, `--preserve-times`, `--on-conflict` on `recover`
- `--map`, `--passes`, `--rate-limit` on `image`
- Filter expressions: `mime:`, `size:`, `date:`, `conf:`, `hash:`
- Configuration files (`--config`, `config.toml`)
- JSON Schema files (`docs/schemas/`)
