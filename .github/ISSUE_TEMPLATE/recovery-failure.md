---
name: Recovery Failure Report
about: Report a case where Relift failed to recover expected files
title: "[Recovery Failure]: "
labels: ["recovery-failure"]
---

## Recovery Failure Report

**⚠️ PRIVACY WARNING**: Do NOT upload private recovered files or expose sensitive device identifiers (serial numbers, volume labels with personal info). Redact sensitive paths in logs.

### Required Information

**Relift Version**: `relift --version` output:

**Operating System**: 
- [ ] Windows 10/11
- [ ] macOS 12+
- [ ] Linux (distro: )

**Filesystem Type**:
- [ ] NTFS
- [ ] FAT32
- [ ] exFAT
- [ ] ext4
- [ ] APFS
- [ ] HFS+
- [ ] Btrfs
- [ ] XFS
- [ ] RAW/Unknown

**Source Device**:
- [ ] Internal HDD
- [ ] Internal SSD (SATA)
- [ ] Internal NVMe
- [ ] External USB HDD
- [ ] External USB SSD
- [ ] SD/microSD Card
- [ ] Disk Image (.img/.dd/.E01/.vhd/.vmdk)

**Scan Mode Used**:
- [ ] quick (metadata)
- [ ] deep (metadata + carve)
- [ ] formatted
- [ ] lost-partition

### What Happened

**Files Expected to Recover**:
| Filename | Size | Format | Original Path | Deletion Method |
|----------|------|--------|---------------|-----------------|
|          |      |        |               |                 |

**Relift Output**:
- Scan mode used:
- Candidates found:
- Files recovered:
- Confidence of recovered files:

**What Went Wrong**:
- [ ] No files found at all
- [ ] Files found but wrong names/paths
- [ ] Files found but corrupted/unopenable
- [ ] Files found but missing (false negative)
- [ ] Wrong confidence scores

### Diagnostic Data (Please Attach)

- [ ] `relift doctor` output
- [ ] Scan log / session directory (if small, zip and attach)
- [ ] `relift scan` command used
- [ ] `relift recover` command used
- [ ] Ground truth (if you have original files/hashes for comparison)

### Expected vs Actual

| File | Expected | Actual | Notes |
|------|----------|--------|-------|
|      |          |        |       |

### Additional Context

Any other information that might help diagnose the issue.

---

**Note**: If you have a disk image that demonstrates this failure, you can contribute it to our test corpus (see CONTRIBUTING.md for the fixture contribution process). This helps us add regression tests.