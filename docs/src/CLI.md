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
| `--config <FILE>` | Use a TOML config file; otherwise search `./relift.toml` and the user config directory. Applies to `scan`, `list`, and `recover`. |

Config values are defaults. Explicit command-line values take precedence. Unknown
keys and invalid modes/confidence labels are rejected. Supported settings:

```toml
[scan]
mode = "deep"                 # quick or deep
include_live = false
carve = false
max_candidates = 1000000

[recover]
verify = true
min_confidence = "likely"

[output]
dest = "/mnt/recovery"

[filter]
ext = ["jpg", "png"]
format = ["pdf"]
name = ["IMG_"]
path = ["DCIM"]
min_confidence = "likely"
```

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
| `--session <DIR>` | New or empty directory for session output; an existing interrupted session with `--resume` (required) |
| `--resume` | Continue an interrupted session. Uses the session's recorded options and requires an unchanged source |
| `--mode <MODE>` | Scan preset: `quick` (metadata only) or `deep` (metadata + validating carving). Default: config value or `quick`. |
| `--live` | Also list files that are still allocated |
| `--no-live` | Disable live entries, including a config default |
| `--carve` | Run signature carving in addition to metadata recovery (implied by `--mode deep`) |
| `--no-carve` | Disable carving, including config and deep-mode defaults |
| `--volume <INDEX>` | Scan only this volume index (repeatable) |
| `--offset <BYTES>` | Manual volume offset in bytes (requires `--length`) |
| `--length <BYTES>` | Manual volume length in bytes (requires `--offset`) |
| `--max-candidates <N>` | Stop after this many candidates (default: config value or 1,000,000) |
| `--json` | Machine-readable JSON output |

**Examples:**

```sh
# Quick metadata scan (never reads the whole volume)
relift scan /dev/sdb --session ./session

# Deep scan with validating signature carving
relift scan image.dd --session ./session --mode deep

# Continue a cancelled or crashed scan (the source must be unchanged)
relift scan image.dd --session ./session --resume

# Scan only a specific volume index
relift scan /dev/sdb --session ./session --volume 2

# Manual volume range
relift scan image.dd --session ./session --offset 1048576 --length 1000000000
```

**Resume semantics.** The checkpoint advances only after a whole volume has
been scanned and its candidates have been flushed to disk. `--resume` skips
volumes already recorded as complete and rescans the interrupted volume from
its start; candidates written after the last completed volume are discarded.
The session's recorded options (carving, live entries, volume selection, manual
range, candidate limit) are reused. The source is re-opened read-only and its
size and sampled fingerprint must match the session, otherwise resume is
refused with exit code 4. Live pause/resume is not supported. Interrupted image
copies can continue with `relift image --resume`; unreadable-sector retries and
multi-pass recovery are not implemented.

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
| `--dest <DIR>` | Destination directory (must be on a different physical device); can come from `[output].dest` |
| `--source <PATH>` | Re-open this source instead of the path recorded in the session |
| `--filter <EXPR>` | Filter candidates (repeatable): same expressions as `relift list` |
| `--dry-run` | Plan only: validate source and destination, list what would be written. No files are written and the session is not modified. |
| `--no-verify` | Do not re-read each output file after writing it (verification is on by default) |
| `--verify` | Re-read each output file, overriding a config setting that disables verification |
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
| `--resume` | Resume from the last durable checkpoint in the existing `.partial` image and `.relift-map.jsonl` file |
| `--no-verify` | Do not re-read the finished image to verify its hash |
| `--json` | Machine-readable JSON output |

**Example:**

```sh
relift image /dev/sdb --dest /mnt/backup/drive.img
```

If imaging is interrupted, rerun the same command with `--resume`. Relift
revalidates the source identity and checks the saved image prefix against the
current source bytes before continuing. This re-reads the checkpointed prefix.
Checkpoints are synced every 256 MiB by default; an interruption may require
recopying bytes since the last checkpoint. `--resume` only applies to a Relift
partial image and its matching map file. It does not retry unreadable sectors
or provide ddrescue-style multi-pass recovery.

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

### `relift partitions` and `relift partitions-scan`

`partitions` reads an existing MBR/GPT/APM table. `partitions-scan` checks
sector boundaries for known filesystem signatures and probes likely volume
starts; reported ranges are estimates for review, not a repaired partition
table. `--session` writes a catalog-only session and does not scan files.

```sh
relift partitions image.dd --probe
relift partitions-scan image.dd --max-bytes 500G --session ./partition-catalog
```

### `relift info`, `relift verify`, and `relift session`

`info` prints source or session identity information. `verify` streams the
recovered outputs and checks their SHA-256 values against the session log;
paths outside the recorded destination are rejected. Partial recovery records
are reported as incomplete. `session --full` includes the candidate listing.

```sh
relift info image.dd --verbose
relift verify ./session --json
relift session ./session --full
```

### `relift virtual-disk` and `relift forensic-image`

These commands inspect container metadata only. They do not make a container
scannable through `relift scan`; several formats and virtual-disk readers remain
incomplete. Convert the input to a raw image before scanning.

### `relift self-update` and `relift mcp`

These are status stubs. `self-update` does not query or install releases, and
`mcp` does not start a server.

## Exit codes

| Code | Constant | Meaning |
|------|----------|---------|
| 0 | `EXIT_OK` | Success |
| 1 | `EXIT_RUNTIME` | Runtime error |
| 2 | `EXIT_USAGE` | Usage/argument error |
| 3 | `EXIT_UNSAFE` | Destination safety refusal (same physical device) |
| 4 | `EXIT_MISMATCH` | Source mismatch (session source != actual source) |
| 5 | `EXIT_PARTIAL` | Partial (some files failed or were incomplete) |

## Not implemented

The following options and services are not part of the current CLI:

- `--mode formatted`, `--mode lost-partition`
- `--quiet`, `--dry-run` on `scan`
- `--preserve-paths`, `--preserve-times`, `--on-conflict` on `recover`
- `--map`, `--passes`, `--rate-limit` on `image`
- Filter expressions: `mime:`, `size:`, `date:`, `conf:`, `hash:`
- JSON Schema files (`docs/schemas/`)
- Bad-sector-aware image maps, retry passes, and ddrescue map import/export
