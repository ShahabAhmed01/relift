# Troubleshooting

## Quick diagnosis

Run `relift doctor` first. It checks:
- Raw device access permissions
- Full Disk Access (macOS)
- TRIM status on SSDs
- SMART health
- Hashing/carving/parsing unit tests

```bash
relift doctor
# For JSON output:
relift doctor --json
```

## Common issues

### "No devices detected" / "Device not accessible"

**Windows:**
- Run terminal as **Administrator**
- Check `relift doctor` → "Raw device access: FAIL" means elevation needed
- Antivirus may block raw disk access; add exception for `relift.exe`

**Linux:**
- Run with `sudo` or add user to `disk` group: `sudo usermod -aG disk $USER`
- Check `lsblk` — device must exist
- Some USB bridges hide devices; try `ls /dev/sd*` and `ls /dev/nvme*`

**macOS:**
- Grant **Full Disk Access** to Terminal / Relift:
  1. System Settings → Privacy & Security → Full Disk Access
  2. Add Terminal.app (or Relift.app)
  3. Restart terminal
- Use `/dev/rdiskX` (raw) not `/dev/diskX` (buffered)

### "Destination unsafe" / "Refusing to write to source device"

**Cause:** Recovery destination is on the same physical drive as the source.

**Solution:**
- Choose a folder on a **different physical drive**
- Verify with `relift devices` — check serial numbers differ
- External USB drive, second internal drive, or network share (not recommended)

### Scan stuck / very slow

**Symptoms:** Progress bar stops, throughput drops to < 1 MB/s.

**Causes & fixes:**
| Cause | Fix |
|-------|-----|
| Failing drive (bad sectors) | **Stop. Image first:** `relift image /dev/sdX --dest /mnt/backup/drive.img --map /mnt/backup/drive.map --passes 3 --rate-limit 10MB/s` |
| SMR drive | Use `--rate-limit 50MB/s`; single-threaded sequential read |
| USB 2.0 / slow bridge | Expected; wait or use faster connection |
| Encrypted volume | Relift detects high entropy; carving skipped. Use metadata scan with credentials. |
| Huge image on slow disk | Scan from NVMe, not HDD/USB |

### "No files found" / "0 candidates"

**Checklist:**
1. **Run `relift doctor`** — verify drive readable
2. **Try quick scan first** — `relift scan /dev/sdX --session ./s --mode quick`
3. **If quick scan finds nothing, try deep** — `--mode deep --carve`
3. **SSD with TRIM** — Deleted data likely erased by controller. Metadata (names) may survive; content usually not. See `relift doctor` → "TRIM: active".
4. **Full format / overwrite** — Data gone. See `docs/limitations.md`.
4. **Encrypted volume** — `relift doctor` shows "Encryption: detected". Need credentials.
5. **Wrong volume** — `relift devices` shows partitions; scan the right one (`--volume 2`)

### "Recovered files won't open" / "Corrupted"

**Causes:**
| Issue | Explanation |
|-------|-------------|
| Partial / truncated | Confidence was "Partial" or "Unverified". Check report. |
| Fragmented | File split across non-contiguous clusters. Carving may miss pieces. |
| Overwritten | Clusters reused by new data. Original content gone. |
| Encrypted | FileVault/BitLocker/LUKS without key. Relift detects and warns. |
| Wrong format | Carved as `.jpg` but actually `.png`. Try hex preview. |

**Solutions:**
- Check confidence in report — only recover "Verified" / "Likely"
- Use `--min-confidence verified` to filter
- For fragmented videos: provide reference file from same camera (future feature)
- Verify SHA-256 matches report

### Permission denied / "Access denied"

**Windows:**
- Run as Administrator
- Check antivirus exclusions
- Volume may be locked by OS: `chkdsk /x X:` (warn: may modify FS)

**Linux/macOS:**
- `sudo relift scan ...`
- Or add user to `disk` group: `sudo usermod -aG disk $USER && newgrp disk`

### GUI won't start

**Windows:**
- Install **WebView2 Runtime** (Windows 10 1809+ includes it)
- `relift doctor` → "WebView2: PASS/FAIL"

**Linux:**
- Install WebKitGTK: `sudo apt install libwebkit2gtk-4.1-0` (Ubuntu/Debian)
- Wayland: set `GDK_BACKEND=x11` if issues

**macOS:**
- Allow in Security & Privacy → General → "Allow anyway"
- `xattr -d com.apple.quarantine /Applications/Relift.app`

### "relift: command not found"

**Installation:**
```bash
# Cargo
cargo install relift-cli

# Binary download
curl -sSL https://relift.rs/install.sh | sh

# Package managers
brew install relift          # macOS
winget install relift.vestige # Windows
scoop install relift         # Windows
```

### Session resume fails

**Error:** "Source mismatch" / "Session source != actual source"

**Causes:**
- Different drive plugged in (same `/dev/sdX` but different serial)
- Image file moved/renamed
- Partition layout changed

**Fix:** Use `--source` to point to correct source:
```bash
relift recover ./session --dest ./out --source /correct/path.img
```

### Out of memory / OOM killed

**Symptoms:** Process killed, `dmesg` shows "Out of memory"

**Fixes:**
- Lower memory limit: `relift scan --max-mem 128MB ...`
- Reduce threads: `--threads 2`
- Scan smaller volumes separately: `--volume 1`
- Ensure swap enabled

### False positives / junk files

**Symptoms:** Thousands of tiny "recovered" files, mostly garbage.

**Causes:**
- Carving on encrypted/high-entropy volume
- Carving on formatted volume without metadata

**Fixes:**
- Use `--min-confidence verified` to filter
- Run quick scan first (metadata) before deep carve
- Relift's validators reject most false positives; remaining are usually partial fragments

### "relift doctor" shows FAIL

| Check | Fix |
|-------|-----|
| Raw device access | Run as admin/root; check Full Disk Access (macOS) |
| TRIM active | Normal for SSDs; carving unlikely to work |
| SMART warning | Drive failing; image first |
| WebView2 missing | Install WebView2 Runtime (Windows) |
| Hash test | Should pass; if not, report bug |

## Getting help

1. Run `relift doctor --json` and include output
2. Check `docs/GAP_REPORT.md` and `docs/limitations.md`
3. Search [GitHub Issues](https://github.com/ShahabAhmed01/relift/issues)
4. Open a **Bug report** with:
   - `relift doctor` output
   - OS, filesystem, source type
   - Scan mode used
   - Session directory (if small) or scan log
5. For false positives/negatives: use **Recovery failure** template with ground truth if possible