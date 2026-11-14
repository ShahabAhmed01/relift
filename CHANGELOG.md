# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial project structure with workspace
- Core recovery engine (relift-core)
- CLI interface (relift-cli)
- Test fixture generator (relift-testkit)
- Tauri 2 + Svelte 5 GUI (relift-gui)
- Benchmark suite (relift-bench)

### Filesystems
- NTFS: MFT parsing, resident/non-resident data, $I30 slack, $LogFile, $UsnJrnl, $Recycle.Bin
- FAT12/16/32: Directory entries, LFN, cluster chain reconstruction
- exFAT: Entry sets, allocation bitmap, NoFatChain handling
- ext2/3/4: Inode tables, extent trees, journal, directory slack
- APFS: Container/volume parsing, snapshots, B-tree traversal
- HFS+: Catalog B-tree, extents overflow, journal
- Btrfs: Root tree, chunk tree, subvolumes, snapshots
- XFS: AG headers, inode B+trees, directories

### Carving
- 25+ formats with structural validation
- JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF
- PDF, DOCX, XLSX, PPTX, ODT, RTF, EPUB
- ZIP, 7z, RAR4, gzip, bzip2, xz, zstd, tar
- MP4, MOV, MKV, AVI, WebM, FLV
- MP3, WAV, FLAC, OGG, AAC
- SQLite, MDB, ACCDB
- PST/OST, LNK, EVTX, Registry, PE/ELF

### CLI
- scan, recover, image, devices, list, info, verify, doctor, version
- JSON output with stable schema
- Exit codes (0-5)
- Shell completions (bash, zsh, fish, powershell, elvish)
- Filter expressions (ext, mime, size, date, path, confidence, hash)
- Progress events, dry-run, resume

### GUI (Tauri 2 + Svelte 5)
- 4-screen flow: Source → Scan → Results → Recover
- Live streaming results with pause/resume
- Virtualized results table (500k+ rows)
- Preview pane (images, text, PDF, hex, metadata)
- Filter bar (search, type, date, size, confidence)
- Destination picker with safety validation
- Dark/Light themes, i18n scaffolding (en/zh-CN)
- Design tokens: Inter/JetBrains Mono, teal accent, 4px grid

### Documentation
- README with comparison table, quickstart
- CLI reference (docs/CLI.md)
- GUI guide (docs/GUI.md)
- Filesystem reference (docs/filesystems.md)
- Format reference (docs/formats.md)
- Forensic guide (docs/forensic-use.md)
- Benchmarks (docs/benchmarks.md)
- Troubleshooting (docs/troubleshooting.md)
- Comparison table (docs/comparison.md)
- FAQ (docs/FAQ.md)
- Support matrix (docs/support-matrix.md)
- Limitations (docs/limitations.md)
- Threat model (docs/threat-model.md)
- GAP report (docs/GAP_REPORT.md)
- Verification tracking (docs/VERIFICATION.md)
- ADRs (6) + Conflict register
- Competitive landscape research
- Benchmark methodology

### CI/CD
- GitHub Actions: Linux/macOS/Windows matrix
- fmt, clippy -D warnings, test, doc, deny, audit
- cargo-deny (licenses, advisories, bans)
- MSRV testing (1.82)
- Coverage upload
- Weekly benchmarks + regression alerts
- Nightly fuzzing
- Docs deployment to GitHub Pages
- Release automation with cargo-dist
- Signed artifacts + SBOM + attestations (when creds available)

### AI Discoverability
- llms.txt + llms-full.txt
- AGENTS.md with canonical recipes
- MCP server design
- JSON-LD SoftwareApplication + FAQPage + HowTo
- SEO-optimized docs structure
- Chinese translation scaffolding

### Packaging
- Windows: MSI + portable ZIP
- macOS: Universal DMG
- Linux: AppImage, .deb, .rpm, Flatpak, Snap, static musl
- Package managers: Homebrew, winget, Scoop, AUR, cargo, npm, pipx

## [0.1.0] - 2024-11-14

### Added
- Initial release