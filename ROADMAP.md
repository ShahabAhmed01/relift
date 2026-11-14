# Roadmap

## v0.1.0 — Current (Foundation)
- ✅ Core engine: read-only, streaming, session persistence
- ✅ Filesystem parsers: NTFS, FAT12/16/32, exFAT, ext2/3/4, APFS, HFS+, Btrfs, XFS
- ✅ Carving: 25+ formats with structural validation
- ✅ CLI: scan, recover, image, devices, doctor, report
- ✅ GUI: 4-screen Tauri 2 + Svelte 5 (source, scan, results, recover)
- ✅ Tests: 43 unit + 6 e2e with SHA-256 fixture validation
- ✅ Docs: architecture, CLI, GUI, filesystems, formats, forensic, troubleshooting
- ✅ CI: Linux/macOS/Windows, fmt/clippy/test/deny/audit

## v0.2.0 — Polish & Breadth (Q1 2025)
- [ ] **Filesystems**: ReFS metadata, F2FS metadata, ZFS detection, ISO 9660/UDF
- [ ] **Carvers**: +20 formats (FLAC, RAR5, ZIP64, OLE, MPEG-TS, 3GP, AIFF, M4A, WMA, EVTX deeper, LNK deeper, PST deeper, Torrent, JSON, XML, YAML, INI, JFIF/Exif)
- [ ] **Imaging**: E01/Ex01 write, VHD/VHDX write, VMDK write, AFF4 read
- [ ] **Partition recovery**: Orphaned superblock scan → virtual mount → scan
- [ ] **RAID**: mdadm RAID 0/1/5 reconstruction, LVM2 linear/striped
- [ ] **Performance**: SIMD multi-pattern (aho-corasick), io_uring/IOCP, PGO builds
- [ ] **Benchmarks**: Published methodology + CI regression tracking
- [ ] **Packaging**: winget, Homebrew, Scoop, AUR, Flatpak, Snap, cargo, npm, pipx
- [ ] **Docs site**: mdBook with search, JSON-LD, llms.txt, sitemap
- [ ] **i18n**: zh-CN, es, de, pt-BR, ja (GUI + docs)

## v0.3.0 — Forensics & Automation (Q2 2025)
- [ ] **MCP server**: `relift mcp` for AI agents (list, scan, recover with human approval)
- [ ] **Forensic features**: DFXML export, chain-of-custody manifest, case/investigator metadata
- [ ] **Encryption support**: BitLocker (recovery key), LUKS (passphrase/keyfile), FileVault
- [ ] **Advanced NTFS**: $ATTRIBUTE_LIST, $UsnJrnl replay, $LogFile parsing, WOF/XPRESS
- [ ] **Advanced ext4**: Journal replay, inline data >60B, bigalloc, encryption
- [ ] **Fragmented reconstruction**: MP4 moof/mdat chaining, JPEG gap carving, ZIP split
- [ ] **Custom carvers**: WASM plugin API (`wasmtime`)
- [ ] **TUI**: ratatui-based `relift tui` for SSH/headless
- [ ] **Distributed scan**: Multi-node for >100 TB images (planned)

## v1.0.0 — Production Release (Q3 2025)
- [ ] All v0.x gates passed (see `docs/VERIFICATION.md`)
- [ ] Signed releases: Windows (Authenticode), macOS (notarized), Linux (minisign)
- [ ] SBOM (CycloneDX), SLSA provenance, GitHub attestations
- [ ] VirusTotal clean reports linked
- [ ] Package manager submissions accepted (Homebrew core, winget, AUR, Flathub, Snapcraft)
- [ ] Docs site live, indexed, SEO optimized
- [ ] Launch: Show HN, Reddit (r/rust, r/datarecovery, r/sysadmin), Product Hunt, Chinese track
- [ ] Post-launch: Monthly releases, quarterly benchmarks, annual fuzz audit

## v1.1+ — Post-Launch
- [ ] **Custom carver marketplace**: Community-contributed WASM carvers
- [ ] **Network scanning**: iSCSI, SMB, NFS block device access
- [ ] **Cloud integration**: S3/GCS/Azure Blob as source/destination
- [ ] **AI-assisted**: ML-based fragment classification, heuristic gap filling
- [ ] **Mobile**: Android (Termux) / iOS (notarized) CLI
- [ ] **Enterprise**: LDAP/SSO, audit logging, policy enforcement

## Milestone tracking

| Milestone | Target | Status |
|-----------|--------|--------|
| Core engine complete | 2024-11 | ✅ |
| CLI complete | 2024-11 | ✅ |
| GUI complete | 2024-11 | ✅ |
| APFS/HFS+/Btrfs/XFS | 2024-12 | 🔄 |
| 45+ carvers | 2025-01 | 🔄 |
| RAID/LVM | 2025-02 | 📋 |
| PGO benchmarks | 2025-02 | 📋 |
| Packaging complete | 2025-03 | 📋 |
| Docs site live | 2025-03 | 📋 |
| v1.0.0 release | 2025-07 | 📋 |

## Contribution focus areas

**Good first issues** (seeded at launch):
1. Add carver for FLAC (validator + test + fuzz)
2. Add carver for RAR5 (validator + test + fuzz)
3. Add carver for MPEG-TS (validator + test + fuzz)
4. Improve APFS snapshot enumeration
5. Add ReFS metadata parser (superblock + directory)
6. Add F2FS checkpoint pack parser
7. Add ISO 9660/UDF parser
8. Benchmark harness for signature sweep
9. GUI: keyboard shortcuts for all actions
10. Docs: Chinese translation of FAQ

**Help wanted** (experience needed):
- RAID 5/6 parity reconstruction
- WASM plugin API design
- E01/AFF4 write implementation
- BitLocker/LUKS/FileVault credential handling
- PGO build pipeline
- Cross-compilation for ARM64 Windows/macOS