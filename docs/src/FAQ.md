# Frequently Asked Questions

## Recovery and safety

### What does Relift do?

Relift scans raw disk images and raw devices for recoverable filesystem
metadata and structurally validated file signatures. It stores findings in a
session directory, then can write selected files to a separate destination.
The main workflows are documented in [`CLI.md`](CLI.md).

### Which filesystems are supported?

FAT12/16/32, exFAT, NTFS, and ext2/3/4 have deterministic synthetic fixture
coverage. HFS+, APFS, Btrfs, and XFS parsers are experimental and have not been
validated against fixtures or real volumes. See the [support
matrix](support-matrix.md).

### What file types can it carve?

Relift emits only formats with structural validators. The current list and
known omissions are in [`support-matrix.md`](support-matrix.md). A recognized
signature alone is not enough to emit a candidate.

### Does Relift write to the source?

Scanning uses a read-only source interface. Recovery and imaging write to
separate destination paths. Always confirm that the destination is on a
different physical volume, and run recovery with `--dry-run` first. Do not let
an agent access a physical device without first confirming the source and
destination with a human.

### Can I recover deleted data from an SSD?

Sometimes, if the content has not been erased or overwritten. TRIM and garbage
collection often make deleted SSD content unrecoverable. Relift does not
determine that data is recoverable just because metadata remains.

### What happens after a format or overwrite?

If sectors were overwritten or discarded, their previous bytes cannot be
recovered by software. A quick format may leave some metadata or content; a
full overwrite generally does not.

### Does Relift support encryption or RAID reconstruction?

No. Relift does not crack or bypass encryption and does not reconstruct RAID,
LVM, or other multi-device volumes. Decrypt or assemble the source with
authorized tools first, then provide a supported raw image or device.

### Can it read E01, VHD, VMDK, QCOW2, or DMG images?

No. Convert those containers to a raw image before scanning. Only raw images
are supported directly.

### How do I handle a failing drive?

Stop using it. If it is clicking, beeping, disconnecting, or returning repeated
read errors, consider professional recovery. Relift's `image` command can
resume from durable checkpoints, but it does not provide unreadable-sector
maps, retries, or multi-pass recovery. Image to a different physical volume
before scanning when the device is stable enough to read.

### Can I pause or resume a scan?

Relift cannot pause a running scan. If a scan is cancelled or the process
crashes, `relift scan <source> --session <dir> --resume` continues it after
revalidating the source. Completed volumes are skipped and the interrupted
volume is rescanned from its start, so that volume's candidates are regenerated
rather than duplicated. The GUI can stop a scan but does not expose resume.

## Using Relift

### How do I start with an image?

```sh
relift scan image.dd --session ./session --mode quick
relift list ./session
relift recover ./session --dest /mnt/recovery --dry-run
```

Review the dry-run output, confirm the destination is on a separate physical
volume, then run recovery without `--dry-run`.

### How do I scan for carved files?

```sh
relift scan image.dd --session ./session --mode deep
```

Deep mode includes metadata scanning and validated signature carving. Carved
files may be partial, especially when the original file was fragmented or
overwritten.

### How do I select files to recover?

Use candidate IDs, extensions, confidence labels, or path/name filters. For
example:

```sh
relift recover ./session --dest /mnt/recovery \
  --filter 'ext:jpg,png,pdf' --min-confidence likely --dry-run
```

The complete filter grammar is in [`CLI.md`](CLI.md). Confidence labels are
evidence categories, not probabilities or guarantees that an application can
open the file.

### Does Relift verify recovered files?

Yes. Recovery re-reads and SHA-256 checks output files by default. The
`--no-verify` option disables that check. Relift does not verify files by
comparing them with an original copy unless you supply that comparison
separately.

### What is a session directory?

A session is a directory containing `session.json`, `candidates.jsonl`,
`recovered.jsonl`, and a short README. It is not a SQLite database. Session
fields are versioned; see [`CLI.md`](CLI.md) and the implementation overview in
[`architecture.md`](architecture.md).

### What does `relift doctor` check?

It runs local storage, hashing, and carving self-tests, checks telemetry/network
status, and attempts a read-only open and 512-byte read from a platform-specific
raw-device path. It may therefore access a physical device; do not run it on
behalf of someone else without confirming first.

## Interfaces and distribution

### Is there a GUI?

Yes. The desktop app uses Tauri 2 and Svelte 5. It covers source selection,
scanning, result review, preview, and recovery. Imaging and report format
selection are currently CLI-only. See [`GUI.md`](GUI.md).

### Is there a TUI or self-update command?

No. The supported interfaces are the CLI and desktop GUI. `relift self-update`
is not implemented.

### Does Relift send files or telemetry over the network?

The core and CLI have no network client or telemetry. No account or cloud
service is required.

### Is there a stable release or package-manager installer?

The [v0.1.0 preview](https://github.com/ShahabAhmed01/relift/releases/tag/v0.1.0)
is published with unsigned assets and a SHA256SUMS file. No stable, signed, or
notarized release is available. Homebrew, winget, Scoop, AUR, Flatpak, and Snap
submissions have not been completed. See [`GAP_REPORT.md`](GAP_REPORT.md).

## Validation limits

The checked-in tests use deterministic synthetic fixtures. No real physical
drive, real deleted volume, labeled accuracy corpus, 24-hour fuzz campaign, or
hardware performance target has been validated. Relift is not certified for
forensic evidence acquisition; see [`forensic-use.md`](forensic-use.md) and
[`VERIFICATION.md`](VERIFICATION.md).
