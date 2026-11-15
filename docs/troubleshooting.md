# Troubleshooting

Use the command reference in [`CLI.md`](CLI.md) for available flags. Relift does
not implement `--resume`, `--max-mem`, `--threads`, `--rate-limit`, image map
files, or a forensic mode.

## Device access

- Raw-device access may require Administrator on Windows or elevated
  permissions on Linux and macOS.
- Prefer a raw image when possible. Only raw image files are supported; convert
  E01/AFF4/VHD/VMDK/QCOW2/DMG containers before scanning.
- `relift devices` lists devices visible to the operating system.
- `relift doctor` runs local self-tests and also attempts a read-only open and
  a 512-byte read from a platform-specific raw-device path. Confirm with the
  device owner before running it on a system with physical media attached.

## Scan problems

### The scan finds no candidates

Run a quick metadata scan first. If it finds nothing and the filesystem or
source is suitable, a deep scan also runs the supported signature validators:

```sh
relift scan image.dd --session ./session --mode quick
relift scan image.dd --session ./deep-session --mode deep
```

Deleted content may already have been overwritten or erased by SSD TRIM. A
full overwrite cannot be reversed. Encrypted volumes are not decrypted by
Relift.

### The session directory is not empty

Use a new or empty directory for `--session`. A scan does not overwrite an
existing session's files.

### The scan is interrupted

Candidates already written to the session are retained. A later scan cannot
resume from the previous scan's checkpoint; start a new scan in a new session.
The GUI's stop action cancels scanning and keeps the results already recorded.

### The source changes before recovery

Relift compares source size and fingerprint with the session. If they differ,
re-scan the current image into a new session. The `--source` option can select
the intended source path, but does not bypass the identity check.

## Recovery problems

### The destination is rejected

Choose a directory on a different physical volume. Some virtual, stacked, or
multi-device destinations are refused when Relift cannot prove their physical
backing. Use an ordinary local volume or a raw image as the source.

Run recovery with `--dry-run` first. Confirm the destination is a separate
physical volume before writing.

### A recovered file is partial or will not open

Content can be incomplete if the original file was fragmented, overwritten,
truncated, or only partially validated. Confidence labels describe the
available recovery evidence; they are not probabilities or guarantees that an
application can open the file. SHA-256 verification checks that output bytes
were written consistently, not that they match an unavailable original.

### Recovery stops with an error

Inspect the JSON or human-readable recovery summary and run
`relift report <SESSION> --format json`. Linux and macOS may retain a failed
output under a collision-safe `.partial` name; Windows removes failed output.

## Imaging problems

`relift image` creates a new raw image and refuses to overwrite an existing
destination. It performs a single-pass copy; it does not retry bad sectors or
resume from a map file. For a failing or physically damaged drive, stop using
it and consider professional recovery or a dedicated imaging tool designed
for read errors.

## GUI problems

The GUI requires the platform's Tauri webview runtime and the native build
dependencies listed by Tauri. On Windows, install WebView2 if it is missing.
On Linux, install WebKitGTK and the system dependencies required by the
distribution. Release bundles are unsigned and may produce operating-system
warnings.

## Still stuck?

Check [`GAP_REPORT.md`](GAP_REPORT.md), [`limitations.md`](limitations.md), and
the current [GitHub issues](https://github.com/ShahabAhmed01/relift/issues).
Include Relift version, OS, filesystem, source type (image or device), command,
and relevant warning/error output. Do not attach images or recovered files that
contain private data.
