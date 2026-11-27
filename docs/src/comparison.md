# Recovery Tool Comparison

Relift has not been evaluated in a reproducible, head-to-head recovery study.
The repository contains deterministic synthetic fixtures, not a representative
corpus of deleted files or measurements against other products. This page
therefore does not rank recovery tools or claim that Relift is faster, more
accurate, safer in practice, or more complete than another product.

## What has evidence in this repository

- FAT12/16/32, exFAT, NTFS, and ext2/3/4 have synthetic fixture tests.
- HFS+, APFS, Btrfs, and XFS are experimental and lack fixture validation.
- Carved candidates are emitted only after a format-specific validator accepts
  them; that does not establish recovery accuracy on real-world data.
- Recovery and imaging write to a validated destination path; raw-device
  behavior has not been tested against physical disks.
- The CLI can export JSON, HTML, and CSV reports. The GUI covers a subset of
  CLI capabilities.

See [`support-matrix.md`](support-matrix.md) and
[`VERIFICATION.md`](VERIFICATION.md) for the evidence and gaps.

## How to compare tools for a recovery case

Compare the exact filesystem, deletion/format scenario, image format, and
platform you need. Use a disposable test image with known file contents, record
which files each tool restores byte-for-byte, and publish the test image,
commands, tool versions, and system profile. Do not compare headline format
counts or scan speed unless the inputs and validation rules are equivalent.

For a failing or irreplaceable drive, stop using it and consider a professional
recovery service. Relift is not certified for forensic evidence acquisition,
partition repair, RAID reconstruction, or encrypted-volume recovery.
