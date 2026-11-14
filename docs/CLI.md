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

List physical disks and partitions.

```
relift devices [--json]
```

**Output (human):**
```
platform: linux
/dev/sda        disk     -            -          1.0 TB   readable
/dev/sda1       part     ESP          vfat         512 MB   readable
/dev/sda2       part     -            ext4       999 GB   readable
/dev/sdb        disk     SanDisk      exfat       32 GB   readable
```

**Output (`--json`):**
```json
{
  "platform": "linux",
  "devices": [
    { "path": "/dev/sda", "kind": "disk", "label": null, "filesystem": null, "size": 1000000000000, "accessible": true },
    { "path": "/dev/sda1", "kind": "part", "label": "ESP", "filesystem": "vfat", "size": 536870912, "accessible": true }
  ],
  "warnings": []
}
```

### `relift scan`

Scan a device or image for recoverable files.

```
relift scan <SOURCE> --session <DIR> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SOURCE>` | Disk image path or raw device (e.g., `/dev/sdb`, `image.dd`, `\\.\PhysicalDrive1`) |
| `--session <DIR>` | New or empty directory for session output (required) |
| `--mode <MODE>` | `quick` (metadata), `deep` (metadata + carving), `formatted`, `lost-partition` (default: `quick`) |
| `--live` | Include non-deleted (live) files |
| `--carve` | Enable signature carving (implied by `--mode deep`) |
| `--volume <INDEX>` | Scan only this volume index (repeatable) |
| `--offset <BYTES>` | Manual volume start offset (requires `--length`) |
| `--length <BYTES>` | Manual volume length (requires `--offset`) |
| `--max-candidates <N>` | Stop after N candidates (default: 1,000,000) |
| `--json` | Machine-readable JSON output |
| `--quiet` | Suppress progress, only final summary |
| `--dry-run` | Scan only, do not write session |

**Examples:**
```sh
# Quick metadata scan
relift scan /dev/sdb --session ./session

# Deep scan with carving
relift scan image.dd --session ./session --mode deep

# Scan specific volume
relift scan /dev/sdb --session ./session --volume 2

# Resume interrupted scan
relift scan image.dd --session ./session --mode deep --resume
```

### `relift recover`

Recover files from a session to a destination.

```
relift recover <SESSION> --dest <DIR> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SESSION>` | Session directory from `relift scan` |
| `--dest <DIR>` | Destination directory (must be on different physical device) |
| `--source <PATH>` | Re-open this source instead of session's recorded path |
| `--filter <EXPR>` | Filter candidates (repeatable) |
| `--min-confidence <LABEL>` | Skip candidates below this confidence (`verified`, `likely`, `partial`, `unverified`) |
| `--id <N>` | Recover only these candidate IDs (repeatable) |
| `--verify` | Re-read each file after write and compare SHA-256 (default: on) |
| `--no-verify` | Disable post-write verification |
| `--preserve-paths` | Preserve original directory structure (default: on when paths known) |
| `--preserve-times` | Preserve original timestamps (default: on) |
| `--on-conflict <MODE>` | `rename` (default), `skip`, `fail` |
| `--dry-run` | Show plan, recover nothing |
| `--json` | Machine-readable JSON output |

**Filter expressions:**
```
ext:jpg,png,mp4          # by extension
mime:image/*             # by MIME glob
size:1KB-100MB           # size range
date:2024-01-01..2024-12-31  # date range
path:*/Pictures/*        # path glob
conf:>=80                # confidence score
hash:abc123...           # exact blake3 hash
```

**Examples:**
```sh
# Recover all
relift recover ./session --dest /mnt/recovery

# Only verified JPEGs
relift recover ./session --dest /mnt/recovery --filter 'ext:jpg' --min-confidence verified

# Dry run
relift recover ./session --dest /mnt/recovery --dry-run

# Specific IDs
relift recover ./session --dest /mnt/recovery --id 123 --id 456
```

### `relift image`

Create a forensic disk image (ddrescue-style).

```
relift image <SOURCE> --dest <FILE> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `<SOURCE>` | Raw device or image |
| `--dest <FILE>` | Output image file (never overwrites existing) |
| `--map <FILE>` | Map file for resumable imaging (ddrescue-compatible) |
| `--passes <N>` | Number of passes (default: 2; forward + reverse) |
| `--rate-limit <R>` | Max read rate (e.g., `50MB/s`) |
| `--verify` | Hash image after creation (default: on) |
| `--no-verify` | Skip verification |

**Example:**
```sh
relift image /dev/sdb --dest /mnt/backup/drive.img --map /mnt/backup/drive.map --passes 3
```

### `relift list`

List files in a session.

```
relift list <SESSION> [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--format <FMT>` | `json`, `csv`, `html`, `md` (default: `json`) |
| `--out <FILE>` | Output file (default: `<session>/report.<fmt>`) |
| `--limit <N>` | Max candidates to list (0 = all) |

### `relift info`

Inspect a file's carver metadata.

```
relift info <FILE>
```

### `relift verify`

Verify recovered files against recorded hashes.

```
relift verify <SESSION> --dest <DIR>
```

### `relift session`

Manage scan sessions.

```
relift session list
relift session resume <SESSION_ID>
```

### `relift doctor`

Run self-tests for permissions, drive health, raw access.

```
relift doctor [--json]
```

**Checks:**
- Raw device access (permissions, elevation)
- Full Disk Access (macOS)
- TRIM detection
- SMART health
- Hashing/carving/parsing unit tests

### `relift version`

Print version and schema info.

```
relift version [--json]
```

### `relift completions`

Generate shell completions.

```
relift completions bash|zsh|fish|powershell|elvish
```

### `relift self-update`

Check for and install latest release.

```
relift self-update
```

## Exit codes

| Code | Constant | Meaning |
|------|----------|---------|
| 0 | `EXIT_OK` | Success |
| 1 | `EXIT_RUNTIME` | Runtime error |
| 2 | `EXIT_USAGE` | Usage/argument error |
| 3 | `EXIT_UNSAFE` | Destination safety refusal (same physical device) |
| 4 | `EXIT_MISMATCH` | Source mismatch (session source != actual source) |
| 5 | `EXIT_PARTIAL` | Partial (some files failed verification) |

## JSON schemas

All `--json` output uses versioned schemas. Current schema: `relift/v1`.

See `docs/schemas/` for full JSON Schema files.

## Configuration

Config file locations (layered, later overrides earlier):
1. Built-in defaults
2. `/etc/relift/config.toml` (system)
3. `~/.config/relift/config.toml` (user)
4. `--config <PATH>` (explicit)

Example `config.toml`:
```toml
[scan]
default_threads = 4
default_chunk_size = "256MB"

[recover]
verify = true
preserve_times = true

[gui]
theme = "dark"
language = "en"
```