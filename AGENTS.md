# AGENTS.md — Relift AI Agent Integration Guide

This document describes how to use Relift from an AI agent or script. It provides the canonical command recipes, safety rules, and machine-readable interfaces.

## What Relift does

Relift recovers deleted files from disk images and raw devices (HDD, SSD, USB, SD). It uses:
- **Filesystem metadata recovery** (NTFS $MFT, FAT/exFAT directories, ext4 inodes, APFS B-trees) for original names/paths/timestamps
- **Validating signature carving** (25+ formats with structural checks: CRC, Huffman, box tree, etc.)
- **Strict read-only** on source; recovery only to a different physical volume

## Installation

```bash
# Linux/macOS
cargo install relift-cli
# or
curl -sSL https://relift.rs/install.sh | sh

# Windows
winget install relift.vestige
scoop install relift

# Anywhere with Rust
cargo install relift-cli

# Node.js
npx relift-cli scan ./image.dd --session ./session --json

# Python
pipx install relift-cli
```

## Safety rules (MUST follow)

1. **Never write to the source device.** Relift enforces this, but agents must not bypass it.
2. **Always recover to a different physical volume.** Verify with `lsblk`/`diskutil`/`Get-PhysicalDisk` before specifying `--dest`.
3. **Use `--dry-run` first.** Preview what would be recovered without writing.
4. **Ask the human** before touching any physical device. Confirm source path and destination.
5. **Stop using the drive** immediately after deletion. Continued use overwrites data.

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
relift scan /dev/sdX --session ./session --mode deep --carve
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

### 4. Image a failing drive first (ddrescue-style), then scan the image

```bash
relift image /dev/sdX --dest /mnt/backup/drive.img --map /mnt/backup/drive.map
relift scan /mnt/backup/drive.img --session ./session --carve
relift recover ./session --dest /mnt/recovery
```

### 5. Scriptable JSON workflow

```bash
# Scan with JSON output
relift scan /dev/sdX --session ./session --carve --json > scan.json

# Parse candidates
jq '.candidates[] | select(.confidence_label=="verified" and .format=="jpg") | .id' scan.json

# Recover selected IDs
relift recover ./session --dest /mnt/recovery --id 123 --id 456 --json
```

## Machine-readable interfaces

### JSON output (`--json`)

All commands support `--json` for machine-readable output.

**Scan output:**
```json
{
  "schema": "relift/v1",
  "tool_version": "0.1.0",
  "session": "./session",
  "source": { "path": "/dev/sdX", "size": 1000000000000 },
  "filesystems": [
    { "volume_index": 0, "offset": 1048576, "kind": "ntfs", "candidates": 1234 }
  ],
  "summary": {
    "candidates": 1234,
    "deleted": 1100,
    "live": 134,
    "with_content": 980,
    "metadata_only": 120
  }
}
```

**Recovery output:**
```json
{
  "destination": "/mnt/recovery",
  "considered": 100,
  "attempted": 95,
  "succeeded": 90,
  "partial": 3,
  "failed": 2,
  "skipped": 5,
  "bytes_written": 1073741824,
  "results": [
    { "id": 1, "original_path": "/DCIM/IMG_001.JPG", "output_path": "/mnt/recovery/IMG_001.JPG", "status": "recovered", "sha256": "..." }
  ]
}
```

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
```

### Shell completions

```bash
relift completions bash > /etc/bash_completion.d/relift
relift completions zsh > ~/.zfunc/_relift
relift completions fish > ~/.config/fish/completions/relift.fish
relift completions powershell > relift.ps1
```

## MCP Server (for AI agents)

Relift includes an MCP server exposing read-only recovery tools:

```bash
relift mcp
```

Tools available:
- `list_devices` — enumerate drives with health info
- `inspect_source` — probe filesystem, encryption, recoverability verdict
- `scan` — start scan, returns session ID (streams progress)
- `list_results` — filterable candidate list from session
- `preview_metadata` — get candidate details without extracting
- `recover` — **requires explicit human-approved destination; refuses source volume; supports dry-run**
- `report` — generate JSON/HTML/CSV report

Config for Claude Desktop:
```json
{
  "mcpServers": {
    "relift": {
      "command": "relift",
      "args": ["mcp"],
      "env": {}
    }
  }
}
```

## Capability manifest

Machine-readable capability description at `docs/capabilities.json`:
```json
{
  "tool": "relift",
  "version": "0.1.0",
  "capabilities": {
    "filesystems": ["ntfs", "fat32", "exfat", "ext4", "apfs", "hfs+", "btrfs", "xfs"],
    "carvers": ["jpg", "png", "pdf", "zip", "docx", "xlsx", "pptx", "mp4", "mp3", "..."],
    "max_file_size": 16384,
    "supports_resume": true,
    "supports_json": true,
    "read_only": true
  }
}
```

## Limitations agents must communicate

- **SSD TRIM:** Deleted data on TRIM-enabled SSDs is usually unrecoverable. Relift detects and warns.
- **Full format/overwrite:** Zeroed sectors cannot be recovered.
- **Encryption:** BitLocker/LUKS/FileVault require credentials; no cracking.
- **Physical damage:** Clicking/beeping drives — stop, image first, consider professional service.
- **Fragmented files:** May be partial; confidence score reflects this.

## Example agent prompt

> "I need to recover deleted photos from a USB drive. The drive is /dev/sdb1 (FAT32). I have a separate recovery drive at /mnt/recovery. Please:
> 1. Verify the source and destination are different physical devices
> 2. Run a quick scan first to see if metadata undelete works
> 3. If that finds nothing, run a deep scan with carving
> 4. Only recover files with 'verified' or 'likely' confidence
> 5. Show me a dry-run summary before writing anything"

The agent should execute the commands in order, show the human each step's output, and ask for confirmation before the final recovery.