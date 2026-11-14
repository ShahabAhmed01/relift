# Relift - Implementation Plan to Complete Master Prompt Requirements

## Current State (as of inspection)
- **Project Name**: Relift (canonical, per CONFLICT_REGISTER.md)
- **Core Engine**: Complete with read-only invariant, 4 FS parsers, 17 carvers
- **CLI**: Complete with 7 subcommands, JSON output, exit codes, session workflow
- **Tests**: 43 unit + 6 e2e tests with SHA-256 fixture validation
- **Docs**: Architecture, CLI reference, gap report, verification, threat model, ADRs
- **CI**: Basic (fmt, clippy, test, build on Ubuntu/Windows)

## Phase 1: Core Engine Enhancements (Week 1-2)

### 1.1 Additional Filesystem Parsers
- [ ] **APFS** parser (container superblock, volume superblock, B-tree, snapshots)
- [ ] **HFS+** parser (catalog B-tree, extents overflow, journal)
- [ ] **Btrfs** parser (chunk tree, root tree, subvolumes, snapshots)
- [ ] **XFS** parser (AG headers, inode B+trees, directories)
- [ ] **ReFS** parser (basic detection + best-effort)
- [ ] **ISO9660/UDF** parser (optical media)

### 1.2 Additional Carving Validators (target 80+ formats)
**Images**: HEIC, AVIF, CR2, NEF, ARW, ORF, RW2, RAF, DNG, PSD, WebP
**Documents**: DOCX, XLSX, PPTX, ODT, ODS, ODP, EPUB, RTF, HTML, CSV
**Archives**: RAR5, ZIP64, TAR, BZIP2, XZ, ZST
**Audio**: FLAC, AAC, M4A, AIFF, WMA
**Video**: MOV, MKV, WMV, FLV, WEBM, MPEG-TS, 3GP, AVI
**Databases**: MDB, ACCDB, DBF, Berkeley DB
**Email**: PST/OST, EML, MSG, MBOX
**System**: LNK, Windows Registry hives, EVT/EVTX, prefetch, hibernation
**Misc**: Torrent, JSON, XML, YAML, INI, JFIF, Exif

### 1.3 Advanced Storage Support
- [ ] **E01/Ex01** image reader (libewf or pure Rust)
- [ ] **VHD/VHDX** image reader
- [ ] **VMDK** image reader (monolithic/split/sparse)
- [ ] **QCOW2** image reader (with backing chain)
- [ ] **AFF4** image reader
- [ ] **RAID** detection (mdadm, Storage Spaces, LDM)
- [ ] **LVM2** support
- [ ] **Partition recovery** (orphaned superblock scan)

## Phase 2: GUI Implementation (Week 3-4)

### 2.1 Tauri 2 + Svelte 5 Setup
- [ ] Create `crates/relift-gui/` with Tauri 2 configuration
- [ ] SvelteKit frontend with Tailwind CSS 4
- [ ] Design system (Inter + JetBrains Mono, teal accent, 4/8/12/16/24/32/48 spacing)
- [ ] Dark/light theme with system preference detection

### 2.2 Four-Screen GUI Flow
1. **Source Picker** - Device cards with health verdicts, image file option
2. **Scan** - Mode selector (Quick/Deep/Formatted/Lost), live progress with ETA
3. **Results** - Virtualized table (TanStack), filters, preview pane, selection
4. **Recover** - Destination picker (blocks source volume), structure options, progress

### 2.3 GUI Features
- [ ] Real-time streaming results during scan
- [ ] Pause/Resume/Stop controls
- [ ] Preview (images, text, PDF first page, hex, metadata)
- [ ] Keyboard navigation, ARIA labels, WCAG AA contrast
- [ ] i18n scaffolding (en, zh-CN)
- [ ] Native integration (Windows installer, macOS DMG, Linux .desktop)

## Phase 3: TUI Implementation (Week 3-4, parallel)

### 3.1 ratatui-based TUI
- [ ] Device picker → Mode selector → Live scan → Results → Recover
- [ ] Vim-like keys, arrow keys, `/` search, `f` filter
- [ ] Works over SSH, 80x24 minimum
- [ ] No-color mode, mouse optional

## Phase 4: Performance Engineering (Week 5)

### 4.1 Benchmarks
- [ ] Criterion benches for: signature sweep, validators, runlist decode, B-tree walk
- [ ] End-to-end harness with 1GB/16GB/128GB fixtures
- [ ] Targets: ≥4 GB/s sweep, ≥1.5 GB/s deep scan, <350 MB RSS on 4TB
- [ ] PGO build pipeline

### 4.2 Optimizations
- [ ] SIMD-accelerated multi-pattern search (aho-corasick)
- [ ] Zero-copy parsing with nom
- [ ] io_uring on Linux, IOCP on Windows
- [ ] Memory pool for buffers

## Phase 5: Packaging & Distribution (Week 5-6)

### 5.1 Installers
- [ ] Windows: MSI (WiX) + portable ZIP + winget manifest + Scoop bucket
- [ ] macOS: Universal DMG + PKG + Homebrew formula + notarization
- [ ] Linux: AppImage + .deb + .rpm + Flatpak + Snap + static musl tarball
- [ ] Cross-platform: `cargo install relift-cli`, `npx relift-cli`, `pipx install relift-cli`

### 5.2 Release Automation
- [ ] cargo-dist configuration
- [ ] Signed artifacts (Windows Authenticode, macOS Developer ID, Linux minisign)
- [ ] SBOM (CycloneDX), GitHub artifact attestations, SLSA provenance
- [ ] VirusTotal submission, checksums, cosign signatures

## Phase 6: Documentation & Launch (Week 6-7)

### 6.1 Documentation Site (mdBook)
- [ ] Static site with search, dark mode, mobile-friendly
- [ ] Pages: Install, Quickstart, CLI reference, GUI guide, Filesystems, Formats, Forensic use, Benchmarks, Troubleshooting, Comparison, FAQ
- [ ] JSON-LD: SoftwareApplication, FAQPage, HowTo
- [ ] Sitemap, robots.txt, llms.txt, llms-full.txt

### 6.2 README Enhancement
- [ ] Hero GIF (15s real capture)
- [ ] Comparison table (Relift vs PhotoRec, Recuva, TestDisk, Disk Drill, EaseUS, R-Studio)
- [ ] All target keywords from §15.1
- [ ] Benchmark summary table
- [ ] Chinese translation (README.zh-CN.md)

### 6.3 Launch Assets
- [ ] Demo image (64MB downloadable .img with ground truth)
- [ ] Automated screenshot capture (Playwright)
- [ ] 90s narrated demo video + asciinema cast
- [ ] Show HN post, Reddit posts (r/rust, r/datarecovery, r/sysadmin, r/opensource)
- [ ] Product Hunt page
- [ ] Chinese track (掘金, 知乎, V2EX, Bilibili, Gitee mirror)

### 6.4 AI Visibility
- [ ] AGENTS.md at repo root
- [ ] MCP server (`relift mcp` command)
- [ ] Machine-readable capability manifest
- [ ] Weekly AI citation tracking

## Phase 7: Hardening & Final Verification (Week 7-8)

### 7.1 Security
- [ ] Fuzzing all parsers (cargo-fuzz, 1M+ iterations nightly)
- [ ] cargo-deny (licenses, advisories, bans)
- [ ] No-network test for engine
- [ ] Threat model documentation

### 7.2 Final Gates (from VERIFICATION.md)
- [ ] G1: CI green on all 3 OSes
- [ ] G2: Recall ≥99%, precision ≥99% on metadata fixtures
- [ ] G3: Carving recall ≥95%, precision ≥98%
- [ ] G4: Zero writes to source (hash before/after)
- [ ] G5: Destination-on-source refused
- [ ] G6: Throughput targets met
- [ ] G7: Quick scan 1TB NTFS < 90s
- [ ] G8: GUI cold start <500ms, 60fps at 500k rows
- [ ] G9: CLI JSON schema stable, exit codes tested
- [ ] G10: Resume after kill -9 works
- [ ] G11: Fuzzing clean for 24h
- [ ] G12: Signed releases on all platforms
- [ ] G13: Real screenshots from running app
- [ ] G14: Docs site live, indexed, JSON-LD valid
- [ ] G15: Anti-slop review passed
- [ ] G16: All package managers installable
- [ ] G17: Launch kit complete
- [ ] G18: VERIFICATION.md with real evidence

## Dependencies to Add

### Core Engine
- `aho-corasick` - multi-pattern search
- `rayon` - parallel carving
- `memmap2` - memory-mapped I/O
- `nom` - zero-copy parsing
- `parking_lot` - faster mutexes
- `smallvec` - inline small vectors
- `blake3` - fast content hashing

### GUI
- `tauri` v2
- `svelte` + `@sveltejs/kit`
- `tailwindcss` v4
- `lucide-svelte` icons
- `tanstack-table` (Svelte adapter) for virtualized results

### TUI
- `ratatui` + `crossterm`

### Image Formats
- `ewf-rs` or custom E01 reader
- Custom VHD/VHDX/QCOW2 readers

### Testing
- `proptest` - property testing
- `cargo-fuzz` - fuzzing
- `insta` - snapshot tests
- `playwright` - GUI E2E tests

## Execution Order

1. **Week 1**: Add dependencies, implement APFS/HFS+/Btrfs/XFS parsers, add 20+ carvers
2. **Week 2**: Implement RAID/LVM, E01/VHD/QCOW2 readers, partition recovery, benchmarks
3. **Week 3**: Tauri GUI setup + Svelte frontend, four screens, design system
4. **Week 4**: TUI implementation, GUI/TUI polish, i18n, accessibility
5. **Week 5**: Performance optimization, PGO builds, benchmark publication
6. **Week 6**: Packaging for all platforms, release automation, docs site
7. **Week 7**: Launch assets, README overhaul, SEO/AI visibility, Chinese track
8. **Week 8**: Final hardening, fuzzing, verification gates, v1.0.0 release

---

## Next Immediate Actions

1. Update Cargo.toml with new dependencies
2. Create `crates/relift-gui/` Tauri project structure
3. Implement APFS parser (highest impact missing FS)
4. Add 10 priority carvers (HEIC, AVIF, DOCX, XLSX, PPTX, FLAC, MKV, PST, LNK, EVTX)
4. Set up benchmark harness with criterion