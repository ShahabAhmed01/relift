# GUI User Guide

## Overview

Relift's GUI is a native desktop application built with Tauri 2 and Svelte 5. It provides a three-screen workflow designed for both novices and experts.

## Screen 1: Choose Source

![Source Picker](../assets/screenshots/01-source.png)

### Device cards show:
- **Name & model** (e.g., "Samsung SSD 970 EVO 1TB")
- **Size** (e.g., "1.0 TB")
- **Filesystem** (NTFS, exFAT, ext4, RAW, etc.)
- **Health verdict** (color-coded badge):
  - 🟢 **Healthy** — Good candidate for recovery
  - 🟡 **TRIM active** — SSD with TRIM; deleted data likely unrecoverable
  - 🔴 **Degraded** — SMART warnings, read errors; **image first**
- **System drive warning** — "System" badge on OS drive

### Actions:
- **Click a device** to select it
- **"Open disk image…"** — Select a raw image (`.img`, `.dd`, `.raw`, or any block file)
- **Advanced options** (collapsed by default):
  - Scan mode: Quick / Deep / Formatted / Lost partition
  - Include live files
  - Worker threads

### Primary button:
- **"Scan for deleted files"** (green) — Normal case
- **"Create image first (recommended)"** (gray) — Appears automatically for degraded drives

## Screen 2: Scanning

![Scan Progress](../assets/screenshots/02-scan.png)

### Live progress:
- **Circular/linear progress bar** with percentage
- **Throughput** (MB/s, rolling median)
- **Elapsed time** and **ETA** (stabilizes after ~10s)
- **Files found so far** counter with type breakdown (Photos, Videos, Documents, etc.)
- **Phase indicator**: Probing → Metadata → Carving → Done

### Controls:
- **Pause** — Freezes scan, preserves results
- **Stop & keep results** — Ends scan, keeps session for recovery

### Results streaming:
- Candidates appear in real-time as they're found
- You can **preview and select files while scan continues**
- No waiting for scan to finish

## Screen 3: Results

![Results Table](../assets/screenshots/03-results.png)

### Layout (three panes):
1. **Left (280px)**: Folder tree (when metadata paths available)
2. **Center (flex)**: Virtualized table — handles 500,000+ rows
3. **Right (360px)**: Preview pane

### Table columns:
- ☑ **Select** — Checkbox (shift-click for range)
- **Icon** — Type-specific (image, video, document, etc.)
- **Name** — Original filename (or `carved_<offset>.<ext>`)
- **Path** — Original folder path (metadata recovery only)
- **Size** — Human-readable (tabular numerals)
- **Type** — Extension/format
- **Confidence** — Colored badge:
  - 🟢 **Verified** (95-100) — Structure validated, clusters not reallocated
  - 🔵 **Likely** (75-94) — Structure valid, minor gaps
  - 🟡 **Partial** (40-74) — Truncated/fragmented
  - ⚪ **Unverified** (0-39) — Signature only (never emitted by default)
- **Modified** — Deletion time or original mtime

### Filter bar:
- **Search** — Name, path, format (as-you-type)
- **Type chips** — Images, Videos, Documents, Archives, Audio, Databases, Other
- **Date range** — Deleted/modified date
- **Size range** — Min/Max
- **Confidence** — Verified / Likely / Partial / All

### Actions:
- **Select all visible** / **Select Verified+Likely** / **Clear**
- **Right-click → Preview** (images, text, PDF first page, hex)
- **Right-click → Recover selected**

### Preview pane:
- **Images**: Thumbnail + metadata (EXIF, dimensions)
- **Text/Code**: First 10 KB with syntax highlighting
- **PDF**: First page rendered
- **Hex**: First 512 bytes with ASCII
- **Metadata**: All timestamps, confidence reasons, validation notes

## Screen 4: Recover

![Recovery](../assets/screenshots/04-recover.png)

### Destination picker:
- **Browse button** — Opens native folder dialog
- **Safety check**: Refuses same physical device with explanation
- **Free space** shown vs. required

### Structure options:
- **By type** (default) — `Photos/`, `Documents/`, `Videos/`, etc.
- **Original paths** — Recreates folder hierarchy (when metadata available)
- **Flat** — All files in one folder
- **By date** — `2024/01/`, `2024/02/`, etc.

### Options:
- **Verify after write** (on by default) — Re-reads and SHA-256 compares
- **Preserve timestamps** (on by default)
- **Collision handling** — Rename (`file (2).ext`) / Skip / Fail

### Recovery progress:
- Per-file rows with status icons (✓ Recovered, ⚠ Partial, ✗ Failed)
- Overall progress bar with throughput
- "Open folder" button when complete

### Summary report:
- Recovered / Partial / Failed / Skipped counts
- Total bytes written
- Link to detailed report (`recovery-report.json` + `.html`)

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `⌘/Ctrl + K` | Command palette |
| `/` | Focus search |
| `Space` | Toggle selection |
| `Enter` | Open preview |
| `⌘/Ctrl + A` | Select all visible |
| `⌘/Ctrl + R` | Recover |
| `Esc` | Close preview / Go back |
| `↑/↓` | Navigate list |
| `←/→` | Expand/collapse tree |

## Accessibility

- **WCAG 2.2 AA** contrast ratios
- **Full keyboard navigation** — every control reachable
- **Screen reader labels** — ARIA on all interactive elements
- **Focus indicators** — Visible 2px accent ring
- **Reduced motion** — Respects `prefers-reduced-motion`
- **200% zoom** — No clipping, horizontal scroll only where needed
- **High contrast** — System theme respected

## Settings

Access via gear icon (⚙) in header:

| Setting | Default | Description |
|---------|---------|-------------|
| Theme | System | Dark / Light / System |
| Language | English | en, zh-CN, es, de, pt-BR, ja |
| Default destination | `~/ReliftRecovery` | Where recovered files go |
| Worker threads | Auto (CPU cores) | Scan parallelism |
| Memory limit | 256 MB | Max RSS during scan |
| Rate limit | Unlimited | MB/s cap for failing drives |
| Verify after recovery | On | SHA-256 re-read |
| Preserve timestamps | On | Original mtime/atime |
| Check for updates | Off | Manual only |

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "No drives detected" | Run as Administrator (Windows) / root (Linux) / grant Full Disk Access (macOS) |
| Drive shows "Locked" | Close other apps using the drive; unmount if possible |
| Scan stuck at 99% | Large file carving; wait or pause |
| "Destination unsafe" | Choose folder on different physical drive |
| Preview not working | Format not supported for preview; try hex view |
| GUI won't start | Install WebView2 (Windows); check `relift doctor` |

## Command palette (⌘/Ctrl + K)

Quick actions:
- `Scan new drive`
- `Open recent session`
- `Open disk image`
- `Run doctor`
- `Toggle theme`
- `Open settings`
- `View recovery report`
- `Help / Keyboard shortcuts`