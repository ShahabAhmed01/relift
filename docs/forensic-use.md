# Forensic Use Guide

## Disclaimer

**Relift is not a certified forensic tool.** It is designed for data recovery, not forensic evidence acquisition. For legally defensible forensics, use hardware write blockers, validated forensic suites (EnCase, FTK, X-Ways), and follow your jurisdiction's rules of evidence.

That said, Relift's read-only architecture and session persistence support chain-of-custody workflows. This guide explains how to use Relift in a forensically sound manner.

## Core forensic principles

### 1. Never write to the source
Relift enforces this at the type level (`ReadOnlySource` trait has no write methods). Verify independently:
- Use a **hardware write blocker** (Tableau, WiebeTech, CRU)
- Or verify OS-level read-only mount: `mount -o ro` (Linux), `diskutil mount readOnly` (macOS)

### 2. Image first, analyze the image
```
relift image /dev/sdX --dest /evidence/drive.img --map /evidence/drive.map --passes 3 --verify
relift scan /evidence/drive.img --session /evidence/session --carve
```
Benefits:
- Original drive preserved
- Scan can be repeated, resumed, shared
- Map file documents bad sectors (ddrescue-compatible)

### 3. Document everything
- Tool version: `relift version --json`
- Command line used (logged in session)
- Source device identity (serial, size, hash)
- Environment: OS, hardware, write blocker model
- Date/time (UTC)

Relift records all this in the session and recovery report.

## Forensic mode

Enable with `--forensic` flag (CLI) or Settings → Forensic mode (GUI):

```bash
relift scan /evidence/drive.img --session /evidence/session --carve --forensic --case-id "CASE-2024-001" --investigator "J. Doe"
```

Effects:
- SHA-256 of source ranges recorded in session
- DFXML export available (`relift list --format dfxml`)
- Timestamps in UTC (RFC3339)
- Deterministic candidate ordering (by offset)
- Extra warnings for forensic relevance (encryption, TRIM, bad sectors)

## Chain of custody

### Session file (`.relift-session`)
- SQLite database with WAL mode
- Contains: source identity, scan parameters, every candidate with provenance
- Portable: copy to another machine to resume/analyze
- Tamper-evident: schema version + tool version recorded

### Recovery manifest
Generated after every recovery:
```json
{
  "tool": "relift",
  "version": "0.1.0",
  "schema": 1,
  "case_id": "CASE-2024-001",
  "investigator": "J. Doe",
  "source": { "device_id": "...", "sha256": "..." },
  "destination": "/evidence/recovered",
  "started": "2024-01-15T10:30:00Z",
  "finished": "2024-01-15T11:45:00Z",
  "results": [
    { "id": 1, "original_path": "/DCIM/IMG_001.JPG", "output_path": "/evidence/recovered/IMG_001.JPG", "status": "recovered", "sha256": "a1b2c3...", "confidence": "verified", "source_extents": [...] }
  ]
}
```
Also exported as HTML and CSV.

## Verification procedures

### 1. Verify source integrity (pre-scan)
```bash
# Hash the source image
sha256sum /evidence/drive.img > drive.sha256
# Record in case notes
```

### 2. Verify Relift's read-only behavior
```bash
# Hash before
sha256sum /dev/sdX > before.sha256
relift scan /dev/sdX --session /tmp/test --mode quick
# Hash after
sha256sum /dev/sdX > after.sha256
# Compare
diff before.sha256 after.sha256  # Should be identical
```

### 3. Verify recovered files
```bash
# Automatic (default)
relift recover /evidence/session --dest /evidence/recovered --verify

# Manual spot-check
sha256sum /evidence/recovered/IMG_001.JPG
# Compare with session's recorded hash
```

### 4. Verify destination isolation
```bash
# Relift refuses same physical device
# Manual check:
lsblk -o NAME,SERIAL,SIZE /dev/sdX
lsblk -o NAME,SERIAL,SIZE /dev/sdY  # destination
# Serials must differ
```

## Reporting

### DFXML export
```bash
relift list /evidence/session --format dfxml --out /evidence/report.dfxml
```
DFXML (Digital Forensics XML) is the standard interchange format for forensic tools. Compatible with:
- `fiwalk` / `dfxml` Python tools
- Autopsy / The Sleuth Kit
- Custom analysis pipelines

### HTML report
```bash
relift list /evidence/session --format html --out /evidence/report.html
```
Human-readable with sorting, filtering, confidence badges.

### CSV for spreadsheet analysis
```bash
relift list /evidence/session --format csv --out /evidence/report.csv
```

## Common forensic scenarios

### Scenario 1: Deleted files on NTFS (Windows)
```bash
# 1. Image with hardware write blocker
relift image \\.\PhysicalDrive1 --dest D:\evidence\drive.img --map D:\evidence\drive.map --passes 3 --verify

# 2. Quick scan for metadata (fast, preserves names)
relift scan D:\evidence\drive.img --session D:\evidence\session --mode quick --forensic --case-id "CASE-001"

# 3. Review results
relift list D:\evidence\session --format html --out D:\evidence\report.html

# 4. Recover verified files
relift recover D:\evidence\session --dest D:\evidence\recovered --min-confidence verified --verify
```

### Scenario 2: Formatted SD card (camera)
```bash
# 1. Image
relift image /dev/mmcblk0 --dest /evidence/sd.img --verify

# 2. Deep scan (carving essential for formatted FAT/exFAT)
relift scan /evidence/sd.img --session /evidence/session --mode deep --carve

# 3. Filter for photos
relift recover /evidence/session --dest /evidence/photos --filter 'ext:jpg,raw,cr2,nef' --min-confidence likely
```

### Scenario 3: Failing drive (SMART errors)
```bash
# 1. Image with conservative settings
relift image /dev/sdX --dest /evidence/drive.img --map /evidence/drive.map --passes 3 --rate-limit 10MB/s

# 2. Scan image (not the dying drive)
relift scan /evidence/drive.img --session /evidence/session --carve

# 3. Recover to healthy drive
relift recover /evidence/session --dest /mnt/healthy/recovery
```

### Scenario 4: Encrypted volume (BitLocker/LUKS/FileVault)
```bash
# Relift detects encryption and refuses to carve noise
# With credentials:
relift scan /dev/sdX --session /evidence/session --mode quick
# Enter recovery key/passphrase when prompted
# (GUI: dialog; CLI: --bitlocker-key / --luks-passphrase)
```

## Limitations for forensic use

| Limitation | Impact |
|------------|--------|
| Not NIST SP 800-86 validated | May not meet court admissibility standards alone |
| No hardware write blocker control | Must use external blocker |
| No hash verification of source device (only ranges) | Hash full device separately |
| ext4 metadata undelete limited | Journal not fully replayed |
| No RAM analysis | Disk-only |
| No mobile forensics | Android/iOS not supported |

## Best practices checklist

- [ ] Hardware write blocker used
- [ ] Source hashed before and after
- [ ] Imaging with verification (`--verify`)
- [ ] Forensic mode enabled (`--forensic`)
- [ ] Case ID and investigator recorded
- [ ] Recovery to separate physical device
- [ ] Post-recovery verification (`--verify`)
- [ ] DFXML + HTML reports saved
- [ ] Session directory preserved (chain of custody)
- [ ] Tool version and command line documented

## Legal considerations

- **Jurisdiction matters**: Some countries restrict forensic tool use to licensed examiners
- **Evidence handling**: Follow your organization's SOP for digital evidence
- **Expert testimony**: If testifying, be prepared to explain Relift's validation methodology
- **Tool validation**: Document that you tested Relift on known-good media before case use

## References

- NIST SP 800-86: Guide to Integrating Forensic Techniques into Incident Response
- NIST SP 800-101: Guidelines on Mobile Device Forensics
- SWGDE Best Practices for Computer Forensics
- DFXML Specification: https://www.forensicswiki.org/wiki/DFXML