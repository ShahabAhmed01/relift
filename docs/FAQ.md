# Frequently Asked Questions

## General

### What is Relift?
Relift is a free, open-source data recovery tool for Windows, macOS, and Linux. It recovers deleted files from hard drives, SSDs, USB flash drives, SD cards, and disk images. It uses filesystem metadata (NTFS $MFT, FAT directories, ext4 inodes, etc.) to recover original filenames and paths, and falls back to validating signature carving for formatted or corrupted volumes.

### Is Relift really free?
Yes. No paid tiers, no file-size limits, no "recover 500 MB then pay" nonsense. GPL-3.0-or-later license. Free forever.

### Does Relift write to my drive?
**Never.** The recovery engine is built around a `ReadOnlySource` trait that exposes no write methods. The CLI and GUI physically cannot write to the source device. Recovery always goes to a separate destination, which Relift verifies is on a different physical drive.

### Can Relift recover files from an SSD?
**Metadata (names, paths, timestamps): often yes.** The filesystem structures (MFT, directories, inodes) may still exist even after TRIM.

**Content: usually no.** Modern SSDs with TRIM enabled erase deleted blocks within seconds. Relift detects TRIM and warns you honestly. If the drive is old or TRIM was disabled, content recovery may work.

### Can Relift recover files after a full format?
**No.** A full format (Windows `format /p`, `dd if=/dev/zero`, `blkdiscard`, secure erase) overwrites every sector. The data is physically gone. Relift will honestly report "no recoverable content" rather than fake results.

### Can Relift recover overwritten files?
**No.** Once new data occupies a sector, the old data is gone on modern drives. Relift does not claim "overwritten recovery" — that's marketing fiction.

### What filesystems does Relift support?
**Metadata undelete (original names/paths):** NTFS, FAT12/16/32, exFAT, ext2/3/4, APFS, HFS+, Btrfs, XFS.

**Carving only (content only, synthetic names):** ReFS, F2FS, ZFS, ISO 9660, UDF, and any unrecognized volume.

See `docs/support-matrix.md` for full table.

### What file formats can Relift carve?
25+ formats with **structural validation** (not just magic bytes):
- Images: JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF, camera RAW
- Documents: PDF, DOCX, XLSX, PPTX, ODT, RTF, EPUB
- Archives: ZIP, 7z, RAR4, gzip, bzip2, xz, zstd, tar
- Video: MP4, MOV, MKV, AVI, WebM, FLV
- Audio: MP3, WAV, FLAC, OGG, AAC
- Databases: SQLite, Access MDB/ACCDB
- System: PST/OST, LNK, EVTX, Registry, PE/ELF

See `docs/formats.md` for full list with validation details.

### Does Relift support RAID?
**Not yet.** RAID detection (mdadm, Storage Spaces, LDM) is implemented; reconstruction is planned. For RAID recovery, use R-Studio, DMDE, or UFS Explorer.

### Does Relift support encryption (BitLocker, FileVault, LUKS)?
**Detection only.** Relift identifies encrypted volumes and refuses to carve noise. With credentials (recovery key, passphrase, keyfile), metadata recovery works. No cracking or bypass.

## Usage

### How do I recover deleted photos from a USB drive?
```bash
# 1. Image the drive (safer)
relift image /dev/sdb --dest /mnt/backup/usb.img

# 2. Quick scan (metadata - preserves names)
relift scan /mnt/backup/usb.img --session ./session

# 3. Review and recover
relift recover ./session --dest /mnt/recovery --filter 'ext:jpg,raw,cr2,nef'
```
Or use the GUI: Select drive → Quick Scan → Filter "Images" → Recover.

### How do I recover files from a formatted SD card?
```bash
relift scan /dev/mmcblk0 --session ./session --mode deep --carve
relift recover ./session --dest /mnt/recovery
```
Formatted FAT/exFAT loses the FAT chain; carving with validation is required.

### How do I recover files from a failing drive?
**Stop using it immediately.** Clicking/beeping = physical damage.
1. Image with conservative settings: `relift image /dev/sdX --dest /mnt/backup/drive.img --map /mnt/backup/drive.map --passes 3 --rate-limit 10MB/s`
2. Scan the **image**, not the drive: `relift scan /mnt/backup/drive.img --session ./session --carve`
3. Recover from image to healthy drive.

### Can I pause and resume a scan?
Yes. Press **Pause** in GUI or send `SIGUSR1` to CLI. Session saves automatically. Resume with:
```bash
relift scan /dev/sdX --session ./session --resume
# or GUI: "Resume previous session"
```

### How do I recover only specific files?
```bash
# By extension
relift recover ./session --dest /out --filter 'ext:jpg,png,pdf'

# By confidence
relift recover ./session --dest /out --min-confidence verified

# By size
relift recover ./session --dest /out --filter 'size:1MB-100MB'

# By path
relift recover ./session --dest /out --filter 'path:*/Photos/*'

# By specific IDs
relift recover ./session --dest /out --id 123 --id 456
```

### What does "Verified / Likely / Partial / Unverified" mean?
- **Verified** (95-100): Full structural validation + clusters not reallocated
- **Likely** (75-94): Structure valid, minor gaps or partial cluster reuse
- **Partial** (40-74): Truncated, fragmented, or incomplete validation
- **Unverified** (0-39): Signature only — **never emitted by Relift**

Only "Verified" and "Likely" are recovered by default.

### How do I verify recovered files are correct?
**Automatic:** `relift recover` runs SHA-256 verification by default. Failed files marked "Failed" in report.

**Manual:**
```bash
relift verify ./session --dest /mnt/recovery
# Or check report: recovery-report.json has SHA-256 for every file
```

## Technical

### Why is Relift written in Rust?
Memory safety (no buffer overflows on malicious disk images), zero-cost abstractions, fearless concurrency, no GC pauses during long scans, single static binary, professional credibility.

### How does Relift validate carved files?
Every format has a **structural validator** that walks the container:
- JPEG: SOI → segments → SOF → Huffman → EOI
- PNG: IHDR → IDAT (CRC32) → IEND
- ZIP: Local headers → Central Directory → EOCD (CRC32)
- MP4: ftyp → moov/moof → mdat box tree
- etc.

Unvalidated hits are **never emitted**.

### How does Relift handle fragmented files?
- **NTFS**: Runlist decoding follows exact cluster chains
- **FAT/exFAT**: Cluster chain following (when FAT retained)
- **Carving**: MP4 box tree reassembly; JPEG restart markers; ZIP central directory ties entries together
- **Fragmented across gaps**: Not yet reassembled (planned)

### What is the session directory?
A portable SQLite database (`.relift-session`) containing:
- Source device identity (serial, size, partition offset)
- Scan parameters
- Every candidate with provenance, extents, confidence
- Progress checkpoints for resume
- Portable across machines

### Can I run Relift on a headless server?
Yes. CLI is fully scriptable:
```bash
relift scan /dev/sdX --session ./session --carve --json > scan.json
relift recover ./session --dest /mnt/recovery --min-confidence verified --json
```
No display server required.

### Does Relift work in Docker?
Yes. For raw device access:
```bash
docker run --rm --privileged -v /dev:/dev -v /mnt:/mnt \
  ghcr.io/shahabahmed01/relift \
  scan /dev/sdb --session /mnt/session --carve
```
Requires `--privileged` for raw block device access.

## Safety & Privacy

### Does Relift collect telemetry?
**No.** Zero network calls at runtime. No analytics, no crash reporter, no auto-update checks. `relift self-update` is opt-in and off by default.

### Can Relift recover malware?
Yes, it recovers **all** files including malware. **Always scan recovered files with antivirus before opening.** Relift writes files with no execute bit.

### Is Relift safe for forensic use?
Relift is **not a certified forensic tool**. For forensics:
- Use hardware write blocker
- Image first (`relift image --verify`)
- Enable `--forensic` flag for DFXML + SHA-256 ranges
- Document tool version, command line, environment

See `docs/forensic-use.md`.

## Troubleshooting

### "No devices detected"
Run as Administrator (Windows) / root (Linux) / grant Full Disk Access (macOS). Run `relift doctor`.

### "Destination unsafe"
You're trying to recover to the same physical drive. Choose a different physical drive.

### Scan stuck at 99%
Large file carving or bad sectors. Try `--rate-limit` or image first.

### "No files found"
- SSD with TRIM? Content likely gone.
- Full format? Data gone.
- Encrypted? Need credentials.
- Try `--mode deep --carve`.

### Recovered files corrupted
Check confidence — only recover "Verified"/"Likely". Fragmented files may be partial.

## Installation

### Windows
```bash
winget install relift.vestige
# or
scoop install relift
# or download .msi from GitHub Releases
```

### macOS
```bash
brew install relift
# or download .dmg from GitHub Releases
```

### Linux
```bash
# AppImage (recommended)
wget https://github.com/ShahabAhmed01/relift/releases/latest/download/relift-x86_64.AppImage
chmod +x relift-x86_64.AppImage
./relift-x86_64.AppImage

# Or package managers
# Arch: yay -S relift-bin
# Flatpak: flatpak install flathub com.relift.Relift
# Snap: snap install relift
```

### From source
```bash
git clone https://github.com/ShahabAhmed01/relift
cd relift
cargo build --release
# Binary at target/release/relift
```

## Contributing

See `CONTRIBUTING.md` for:
- Development setup
- Adding a carver (30-line walkthrough)
- Adding a filesystem parser
- Test fixture workflow
- PR checklist

## License

GPL-3.0-or-later. See `LICENSE`. Contributions dual-licensed under same terms.

## Citation

```bibtex
@software{relift,
  title = {Relift: Read-only deleted-file recovery},
  author = {Shahab Ahmed and contributors},
  year = {2024},
  url = {https://github.com/ShahabAhmed01/relift},
  version = {0.1.0}
}
```
See `CITATION.cff`.