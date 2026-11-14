# Relift

Read-only deleted-file recovery for disk images and drives.

Relift scans disk images and raw devices without writing to them, records what it finds in a session directory, and recovers validated byte ranges only to a separate destination. Unsupported filesystems and unvalidated carves are reported, never silently presented as recovered.

## Canonical identity

- **Name:** Relift (binary `relift`)
- **License:** GPL-3.0-or-later
- **Stack:** Rust 1.82+ (GNU toolchain on Windows)
- **Crates:** `relift-core`, `relift-cli`, `relift-testkit`, `relift-gui`, `relift-bench`

## Features

- **Read-only-by-construction:** `ReadOnlySource` exposes no write method; the engine cannot modify the source device.
- **Real byte-level recovery, fixture-validated with SHA-256 comparison.**
- **Filesystem metadata recovery:** FAT12/16/32, exFAT, NTFS, ext2/3/4, HFS+, APFS, Btrfs, XFS.
- **Signature carving with structural validators:** JPEG, PNG, GIF, BMP, TIFF, WebP, HEIC, AVIF, PDF, ZIP/DOCX/XLSX/PPTX, OLE, 7z, RAR4, gzip, bzip2, xz, zstd, tar, MP4/MOV/MKV/AVI/WEBM, MP3, WAV, FLAC, OGG, SQLite, PE/ELF, PST/OST, LNK, EVTX, and more (25+ formats).
- **Sparse-file segment preservation:** holes and unwritten extents keep byte offsets.
- **Destination safety:** output proven separate from evidence before any write.
- **SHA-256 verification** of every recovered file (on by default).
- **Session-based workflow:** scan → session → recover → report (JSON/HTML/CSV).
- **Pause/resume** scans and recover across restarts.
- **`devices` and `doctor`** self-tests for permissions and drive health.
- **No telemetry;** default offline.
- **GUI (Tauri 2 + Svelte 5):** three-screen flow, live results, preview, safe destination picker.
- **CLI:** full feature parity, JSON output, shell completions, man page.

## Quick start

Image a raw device, then scan the image (recommended over scanning a mounted volume):

```sh
relift image \\.\PhysicalDrive1 --dest D:\images\evidence.dd
relift scan D:\images\evidence.dd --session D:\session
relift recover D:\session --dest D:\recovered
```

Scan an image directly:

```sh
relift scan image.dd --session ./session --live --carve
relift recover ./session --dest ./out
```

List devices:

```sh
relift devices
```

Run self-tests:

```sh
relift doctor
```

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | OK |
| 1 | RUNTIME |
| 2 | USAGE |
| 3 | UNSAFE (destination safety refusal) |
| 4 | MISMATCH (source != session) |
| 5 | PARTIAL |

See `docs/CLI.md` for the full command reference.

## GUI

Relift includes a native desktop GUI built with Tauri 2 and Svelte 5. Three screens, one primary action each:

1. **Source picker** — drives and images with health verdicts
2. **Scan** — live progress, streaming results, pause/resume
3. **Results** — filterable table, preview pane, safe destination picker
4. **Recover** — progress, verification, summary report

Installers available for Windows (MSI), macOS (DMG), Linux (AppImage, .deb, .rpm, Flatpak, Snap).

## Comparison

| Feature | Relift | PhotoRec | TestDisk | Recuva | Disk Drill (free) | EaseUS (free) | R-Studio |
|---------|--------|----------|----------|--------|-------------------|---------------|----------|
| License | GPL-3.0 | GPL-2.0 | GPL-2.0 | Proprietary | Freemium | Freemium | Proprietary |
| Price | Free | Free | Free | Free* | 500 MB limit | 2 GB limit | $$$ |
| GUI | ✓ | ✗ | ✗ | ✓ | ✓ | ✓ | ✓ |
| CLI / JSON | ✓ | ✗ | ✓ | ✗ | ✗ | ✗ | ✓ |
| NTFS $MFT undelete | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ |
| FAT/exFAT undelete | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ | ✓ |
| ext4 undelete | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | Partial |
| APFS/HFS+ undelete | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| Btrfs/XFS undelete | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | Partial |
| Validating carving | ✓ | Partial | ✗ | Partial | Partial | Partial | ✓ |
| Fragmented file recovery | ✓ | ✗ | ✗ | Partial | Partial | Partial | ✓ |
| Read-only guarantee | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✓ |
| Pause/resume | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| SHA-256 verification | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| Disk imaging | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| RAID/LVM | Planned | ✗ | ✓ | ✗ | ✗ | ✗ | ✓ |
| Cross-platform | Win/Mac/Linux | Win/Mac/Linux | Win/Mac/Linux | Win | Win/Mac | Win/Mac | Win/Mac/Linux |

*Recuva free version has no official updates since 2016.

**Where competitors win:** PhotoRec has broader raw format coverage (300+ signatures); TestDisk repairs partition tables; R-Studio has the most complete RAID reconstruction. Relift focuses on validated recovery with honest confidence scoring.

## Benchmarks

| Metric | Target | Method |
|--------|--------|--------|
| Cold start (CLI) | < 50 ms | `hyperfine` median of 100 runs |
| GUI cold start | < 400 ms | Tauri `window_created` → first paint |
| Sequential read (NVMe) | ≥ 90% of `fio` baseline | `fio` baseline comparison |
| Deep scan sweep | ≥ 4 GB/s | 64 MB test buffer, `memchr` SIMD |
| Deep scan full | ≥ 1.5 GB/s | Real fixtures, all carvers |
| Quick scan (1 TB NTFS) | < 90 s | 1M MRT records, NVMe |
| Memory (4 TB scan) | < 350 MB RSS | Bounded streaming, no full buffering |
| Binary size (release) | < 30 MB | `strip`, LTO, single codegen unit |

See `benches/METHODOLOGY.md` for full methodology and `benches/results/` for published CSV results.

## Documentation

- `docs/CLI.md` — command and exit-code reference
- `docs/architecture.md` — system design
- `docs/support-matrix.md` — tested/untested filesystem and platform matrix
- `docs/threat-model.md` — threats and mitigations
- `docs/limitations.md` — known gaps and exclusions
- `docs/GAP_REPORT.md` — tracked gaps
- `docs/VERIFICATION.md` — how recovery is validated
- `docs/decisions/CONFLICT_REGISTER.md` — reconciled names/stacks/licenses
- `docs/decisions/ADR-0001…0006` — architecture decision records
- `docs/research/landscape.md` — pre-architecture research
- `docs/benches/METHODOLOGY.md` — benchmark methodology
- `docs/benches/results/` — published benchmark CSVs

## Building

Requires Rust 1.82+ (GNU toolchain) via rustup:

```sh
cargo build --release
```

The release binary is `target/release/relift.exe` (Windows) or `target/release/relift` (Unix).

For GUI development:

```sh
cd crates/relift-gui
npm install
npm run tauri dev
```

## Tests

```sh
cargo test -p relift-core
cargo test --workspace
```

Includes:
- 43 unit tests (parsers, safety, hashing, sessions, carving, partitions).
- 6 end-to-end tests (scan → recover → SHA-256 verify for FAT32, exFAT, NTFS, ext4; destination separation; output confinement).
- Property-based tests with `proptest`.
- Fuzzing targets for all parsers (`cargo fuzz`).

## Honesty policy

Relift publishes only what is actually verified. No simulated results, no faked benchmarks/screenshots/stars/reviews/release claims, no telemetry. Unsupported filesystems are identified and reported, never silently claimed.

## Limitations

See `docs/limitations.md` and `docs/GAP_REPORT.md` for details. Notable gaps:

- NTFS: `$ATTRIBUTE_LIST`, named ADS, compression, EFS, `$UsnJrnl` not recovered.
- ext4: inline data >60 bytes, encryption, bigalloc, journal replay not implemented.
- Carving: FLAC, RAR5, ZIP64, OLE compound files not validated.
- No real NTFS/ext4 volumes available locally; only synthetic fixtures tested.
- No signing certificates/notarization credentials; releases are honest/unsigned.
- macOS raw-device destination safety intentionally fails closed; macOS raw paths untested.
- Desktop GUI requires WebView2 on Windows.

## License

GPL-3.0-or-later. See `LICENSE`.

## Contributing

See `CONTRIBUTING.md`. Architecture overview, carver contribution guide, test fixture workflow.

## Security

See `SECURITY.md`. Threat model, vulnerability disclosure, signed-release verification.

## Acknowledgements

Built with: `serde`, `clap`, `memchr`, `crc32fast`, `flate2`, `sha2`, `thiserror`, `anyhow`, `rayon`, `nom`, `parking_lot`, `smallvec`, `blake3`, `aho-corasick`, `memmap2`, `tauri`, `svelte`, `tailwindcss`, `lucide-svelte`, `criterion`, `proptest`, `insta`, `cargo-fuzz`.