# AGENTS.md — Relift AI Agent Integration Guide

This document describes how to use Relift from an AI agent or script. It provides
the canonical command recipes, safety rules, and machine-readable interfaces.

## What Relift does

Relift recovers deleted files from disk images and raw devices (HDD, SSD, USB,
SD). It uses:

- **Filesystem metadata recovery** (NTFS $MFT, FAT/exFAT directories, ext4
  inodes, APFS B-trees) for original names/paths/timestamps
- **Validating signature carving** (153 formats with structural checks: CRC,
  Huffman, box tree, etc.)
- **Strict read-only** on source; recovery only to a different physical volume

## Installation

```bash
# From source (Rust)
cargo build --release --workspace
# Binary: target/release/relift (or relift.exe on Windows)

# GUI (Tauri)
cd crates/relift-gui
# Node.js 22.17 or newer
npm ci
npm run tauri build
```

The v0.1.0 preview is published with unsigned release artifacts and a SHA256SUMS
manifest on GitHub. Package-manager submissions (Homebrew, winget, Scoop, AUR,
Flatpak, Snap) remain templates only and have not been submitted or verified.

## Safety rules (MUST follow)

1. **Never write to the source device.** Relift enforces this, but agents must
   not bypass it.
2. **Always recover to a different physical volume.** Verify with
   `lsblk`/`diskutil`/`Get-PhysicalDisk` before specifying `--dest`.
3. **Use `--dry-run` first.** Preview what would be recovered without writing.
4. **Ask the human** before touching any physical device. Confirm source path
   and destination.
5. **Stop using the drive** immediately after deletion. Continued use overwrites
   data.

## Canonical recipes

### 1. Quick scan (metadata only, fast, preserves names/paths)

```bash
relift scan /dev/sdX --session ./session --mode quick
relift recover ./session --dest /mnt/recovery --dry-run
# If dry-run looks good:
relift recover ./session --dest /mnt/recovery
```

### 2. Deep scan (carving + metadata, thorough)

```bash
relift scan /dev/sdX --session ./session --mode deep
relift recover ./session --dest /mnt/recovery --dry-run
relift recover ./session --dest /mnt/recovery
```

### 3. Recover only specific files (by extension, confidence, path)

```bash
relift scan image.dd --session ./session --carve
relift recover ./session --dest /mnt/recovery \
  --filter 'ext:jpg,png,pdf' \
  --min-confidence verified \
  --dry-run
relift recover ./session --dest /mnt/recovery \
  --filter 'ext:jpg,png,pdf' \
  --min-confidence verified
```

### 4. Image a failing drive first, then scan the image

```bash
relift image /dev/sdX --dest /mnt/backup/drive.img
relift scan /mnt/backup/drive.img --session ./session --carve
relift recover ./session --dest /mnt/recovery
```

### 5. Scriptable JSON workflow

```bash
# Scan with JSON output
relift scan /dev/sdX --session ./session --carve --json > scan.json

# Recover selected IDs
relift recover ./session --dest /mnt/recovery --id 123 --id 456 --json
```

## Machine-readable interfaces

### JSON output (`--json`)

All commands support `--json` for machine-readable output.

**Scan output:** a `SessionHeader` JSON object with `source`, `filesystems`,
`summary`, `warnings`, `status`, and `schema` fields.

**Recovery output:** a `RecoverySummary` JSON object with `destination`,
`considered`, `attempted`, `succeeded`, `partial`, `failed`, `skipped`,
`bytes_written`, `dry_run`, `bytes_planned`, `results`, `warnings`, `started`,
and `finished` fields.

**List output:** a JSON object with `session`, `total`, `shown`, and
`candidates` (array of `Candidate` objects).

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Runtime error |
| 2 | Usage error |
| 3 | Unsafe destination (same physical device) |
| 4 | Source mismatch (session vs actual source) |
| 5 | Partial (some files failed) |

### Progress events (stderr)

Real-time progress on stderr (when not `--json`):

```
scan      45%  450000000 / 1000000000
carve     12%  120000000 / 1000000000
recover   80%  80 / 100
```

### Shell completions

```bash
relift completions bash > /etc/bash_completion.d/relift
relift completions zsh > ~/.zfunc/_relift
relift completions fish > ~/.config/fish/completions/relift.fish
relift completions powershell > relift.ps1
```

## Planned but not yet implemented

The following are tracked in `docs/GAP_REPORT.md` and are **not** part of the
current release:

- **MCP server** (`relift mcp`): no MCP server is implemented. The Claude
  Desktop config shown below does not work yet.
- **TUI**: no terminal user interface; CLI and GUI are the supported interfaces.
- **Self-update** (`relift self-update`): not implemented.
- **Image formats**: only raw images (`.img`, `.dd`) are supported. E01/AFF4/
  VHD/VMDK/QCOW2/DMG must be converted to raw first.
- **`relift info`, `relift verify`, `relift session`**: not implemented as
  separate commands.
- **Configuration files** (`--config`, `config.toml`): not implemented.
- **Code signing / notarization**: release artifacts are unsigned.

### MCP server (planned, not implemented)

Relift plans to include an MCP server exposing read-only recovery tools, but it
is not implemented yet. Do not configure `relift mcp` in Claude Desktop.

## Limitations agents must communicate

- **SSD TRIM:** Deleted data on TRIM-enabled SSDs is usually unrecoverable.
- **Full format/overwrite:** Zeroed sectors cannot be recovered.
- **Encryption:** BitLocker/LUKS/FileVault require credentials; no cracking.
- **Physical damage:** Clicking/beeping drives — stop, image first, consider
  professional service.
- **Fragmented files:** May be partial; confidence score reflects this.
- **Synthetic validation only:** All recovery evidence comes from deterministic
  synthetic fixtures. No real disk image or physical drive has been validated
  yet.

## Example agent prompt

> "I need to recover deleted photos from a USB drive. The drive is /dev/sdb1
> (FAT32). I have a separate recovery drive at /mnt/recovery. Please:
> 1. Verify the source and destination are different physical devices
> 2. Run a quick scan first to see if metadata undelete works
> 3. If that finds nothing, run a deep scan with carving
> 4. Only recover files with 'verified' or 'likely' confidence
> 5. Show me a dry-run summary before writing anything"

The agent should execute the commands in order, show the human each step's
output, and ask for confirmation before the final recovery.



