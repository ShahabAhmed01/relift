# Relift Documentation

Welcome to the Relift documentation. Relift is a free, open-source, read-only deleted-file recovery tool for disk images and drives.

> **Preview status.** Validation is limited to deterministic synthetic fixtures. No physical drive or representative recovery corpus has been validated. Release binaries are unsigned. See [Verification](VERIFICATION.md) for the current evidence boundary.

## What Relift Does

Relift scans disk images and raw devices without writing to them, records what it finds in a session directory, and recovers validated byte ranges only to a separate destination. Unsupported filesystems and unvalidated carves are reported, never silently presented as recovered.

## Key Features

- **Read-only by construction** – `ReadOnlySource` exposes no write method; the engine cannot modify the source device
- **Byte-for-byte recovery** verified against synthetic fixtures with SHA-256
- **Filesystem metadata recovery** – FAT12/16/32, exFAT, NTFS, ext2/3/4 (fixture-validated); HFS+, APFS, Btrfs and XFS parsers are implemented but experimental
- **Signature carving** – 36 structurally validated formats, including JPEG, PNG, PDF, ZIP, MP4, MP3, SQLite, FLAC, RAR5, OLE, MPEG-TS, and ISO 9660 (full list in [Supported Formats](formats.md))
- **Session-based workflow** – scan → session → recover → report, with checkpoint resume
- **Cross-platform** – Windows, macOS, and Linux
- **No telemetry** – fully offline by default

## Quick Links

- [Installation](installation.md)
- [Quick Start](quickstart.md)
- [CLI Reference](CLI.md)
- [GUI Usage](GUI.md)
- [Filesystem Support](filesystems.md)
- [Supported Formats](formats.md)
- [Verification](VERIFICATION.md)
- [Limitations](limitations.md)
- [Gap Report](GAP_REPORT.md)
