# Tool Comparison

## Relift vs. Major Alternatives

| Feature | Relift | PhotoRec | TestDisk | Recuva | Disk Drill (Free) | EaseUS (Free) | R-Studio |
|---------|--------|----------|----------|--------|-------------------|---------------|----------|
| **License** | GPL-3.0 | GPL-2.0 | GPL-2.0 | Proprietary | Freemium | Freemium | Proprietary |
| **Price** | Free | Free | Free | Free | 500 MB limit | 2 GB limit | $79-$299 |
| **GUI** | ✓ Native | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ |
| **CLI / JSON** | ✓ Full | ✗ | ✓ Basic | ✗ | ✗ | ✗ | ✓ Limited |
| **NTFS $MFT undelete** | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **FAT/exFAT undelete** | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ |
| **ext4 undelete** | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | Partial |
| **APFS/HFS+ undelete** | ✓ | ✗ | ✗ | ✗ | ✓ | ✓ | ✓ |
| **Btrfs/XFS undelete** | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | Partial |
| **Validating carving** | ✓ (25+) | Partial (300+) | ✗ | Partial | Partial | Partial | ✓ |
| **Fragmented file recovery** | ✓ | ✗ | ✗ | Partial | Partial | Partial | ✓ |
| **Read-only guarantee** | ✓ Type-level | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ |
| **Pause/resume** | ✓ Session | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **SHA-256 verification** | ✓ Default | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **Disk imaging** | ✓ ddrescue-style | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| **RAID/LVM** | Planned | ✗ | ✓ Basic | ✗ | ✗ | ✗ | ✓ |
| **Cross-platform** | Win/Mac/Linux | Win/Mac/Linux | Win/Mac/Linux | Win | Win/Mac | Win/Mac | Win/Mac/Linux |
| **Open source** | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ |
| **Last update** | 2024 | 2023 | 2023 | 2016 | 2024 | 2024 | 2024 |

## Detailed comparison

### Relift vs. PhotoRec / TestDisk (CGSecurity)

**Where PhotoRec/TestDisk win:**
- **Format breadth**: PhotoRec supports 300+ file signatures vs Relift's 25+ validated
- **Partition repair**: TestDisk repairs MBR/GPT, boot sectors, undeletes partitions
- **Maturity**: 20+ years of real-world use, extensive device quirks handled
- **No dependencies**: Single static binary, runs anywhere

**Where Relift wins:**
- **Metadata recovery**: NTFS $MFT, FAT directories, ext4 inodes → original names/paths/timestamps
- **Validating carving**: Every carved file passes structural validation (CRC, Huffman, box tree)
- **Honest confidence scoring**: Verified/Likely/Partial/Unverified with reasons
- **Modern UX**: Native GUI, live streaming results, pause/resume, preview
- **Read-only invariant**: Enforced at type level, not just convention
- **Session persistence**: SQLite-based, resumable after crash/kill
- **JSON/CLI automation**: Full scriptable interface with stable schema
- **Modern codebase**: Rust memory safety, fuzzed parsers, no C buffer overflows
- **Cross-platform native**: No Wine needed on Linux/macOS

### Relift vs. Recuva (Piriform/CCleaner)

**Where Recuva wins:**
- **Windows integration**: Shell extension, deep Windows knowledge
- **Simplicity**: Very simple wizard for non-technical users
- **Free version**: No artificial limits (but no updates since 2016)

**Where Relift wins:**
- **Cross-platform**: Linux, macOS, Windows
- **Open source**: Auditable, no hidden telemetry
- **ext4/APFS/Btrfs support**: Recuva is NTFS/FAT only
- **Validating carving**: Recuva's "deep scan" is header-only
- **No artificial limits**: Recuva free has no limits but Pro adds virtual drive support
- **Active development**: Recuva last updated 2016
- **Read-only guarantee**: Recuva can write to source if not careful

### Relift vs. Disk Drill / EaseUS (Freemium)

**Where Disk Drill/EaseUS win:**
- **Polish**: Very slick commercial UX
- **Marketing**: "Free" versions widely known
- **Preview**: Good built-in previews
- **Partition recovery**: Disk Drill has good partition recovery

**Where Relift wins:**
- **Truly free**: No 500 MB / 2 GB recovery limits
- **No upsells**: No "Pro" tier, no nag screens, no "recover more" buttons
- **Open source**: Auditable, community-driven
- **Validating carving**: Their free versions often skip validation
- **Cross-platform**: Disk Drill Mac/Win only; EaseUS Win/Mac
- **No artificial limits**: Recover terabytes if you want
- **Transparent**: Honest about SSD TRIM, overwritten data, encryption

### Relift vs. R-Studio / UFS Explorer / DMDE (Professional)

**Where professional tools win:**
- **RAID reconstruction**: R-Studio/DMDE excel at RAID 5/6, proprietary RAID
- **Hex editor**: Built-in low-level hex editing
- **Virtual RAID**: Assemble RAID from images
- **Advanced NTFS**: $UsnJrnl replay, $LogFile parsing, transaction rollback
- **Encryption support**: BitLocker/LUKS/VeraCrypt with keys
- **Forensic features**: Case management, hash sets, timeline analysis
- **Support**: Commercial support contracts

**Where Relift wins:**
- **Free & open source**: No license fees
- **Validating carving**: Professional tools often skip validation for speed
- **Modern stack**: Rust vs C++ (memory safety)
- **CLI automation**: JSON, scripting, CI/CD friendly
- **Community**: Extensible carver plugin system (planned)

## When to use each

| Scenario | Recommended |
|----------|-------------|
| Accidental delete on NTFS/FAT/exFAT/ext4 | **Relift** (quick scan, names preserved) |
| Formatted drive / corrupted FS | **Relift** (deep scan + validating carve) |
| SSD with TRIM enabled | **Relift** (honest about limitations) |
| Failing drive (clicking, bad sectors) | **Image first with Relift/ddrescue**, then scan image |
| Complex RAID 5/6 reconstruction | **R-Studio / DMDE / UFS Explorer** |
| Partition table repair | **TestDisk** |
| Maximum format coverage (obscure formats) | **PhotoRec** (then validate manually) |
| Forensic case requiring certified tool | **EnCase / FTK / X-Ways** (Relift as supplementary) |
| Scripted/automated recovery | **Relift CLI** (JSON, stable schema) |
| Cross-platform (Linux/macOS/Win) | **Relift** |
| Budget: $0 | **Relift / PhotoRec / TestDisk** |

## Honest assessment

Relift is **not** a drop-in replacement for professional forensic suites. It lacks:
- Advanced RAID reconstruction
- Full $UsnJrnl/$LogFile replay
- BitLocker/LUKS/VeraCrypt decryption
- Virtual RAID assembly
- Case management / reporting for court

But for **deleted file recovery on common filesystems**, it provides:
- Better validation than header-only carvers
- Original filenames/paths where metadata survives
- Honest confidence scoring
- Free, open-source, cross-platform
- Scriptable for automation

If you need professional RAID/encryption/forensic features, use the professional tools. For everything else, Relift is a strong free alternative.