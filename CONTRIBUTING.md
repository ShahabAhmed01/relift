# Contributing to Relift

Thank you for contributing! This document explains how to set up a development environment, the architecture, and how to add new features.

## Development setup

1. Install Rust 1.90+ (MSVC toolchain recommended on Windows):
   ```sh
   rustup toolchain install stable --profile minimal --component rustfmt,clippy
   rustup default stable
   ```

2. Clone and build:
   ```sh
   git clone https://github.com/ShahabAhmed01/relift
   cd relift
   cargo build --workspace
   ```

3. Run tests:
   ```sh
   cargo test --workspace
   ```

4. For GUI development:
   ```sh
   cd crates/relift-gui
   npm install
   npm run tauri dev
   ```

## Architecture overview

```
crates/
├── relift-core/       # Recovery engine (no UI deps)
│   ├── carve/         # Signature carving + validators
│   ├── fs/            # Filesystem parsers (NTFS, FAT, exFAT, ext, APFS, HFS+, Btrfs, XFS)
│   ├── imaging/       # Disk imaging (ddrescue-style)
│   ├── session/       # Session persistence, resume
│   └── recover/       # Safe recovery writer
├── relift-cli/        # CLI binary (clap)
├── relift-gui/        # Tauri 2 + Svelte 5 GUI
├── relift-testkit/    # Deterministic filesystem fixtures
└── relift-bench/      # Criterion benchmarks
```

Key design rules:
- **Read-only invariant:** `ReadOnlySource` trait has no write methods. All source I/O goes through it.
- **Streaming:** scans stream candidates via `CandidateSink`; nothing is fully buffered in memory.
- **Validation:** every carved file passes a structural validator (CRC, Huffman, box tree, etc.).
- **Confidence scoring:** every candidate has a `Confidence` with label (Verified/Likely/Partial/Unverified), score (0-100), and reasons.

## Adding a new file carver

1. Create `crates/relift-core/src/carve/<format>.rs` with a validator function:
   ```rust
   fn validate_<format>(src: &dyn ReadOnlySource, start: u64, limit: u64) -> Option<Validated> {
       // 1. Read magic bytes
       // 2. Walk container structure
       // 3. Verify checksums / end markers
       // 4. Return Some(Validated::new(length, score).note("...")) or None
   }
   ```

2. Add the format to `MARKERS` array in `carve.rs`:
   ```rust
   Marker {
       format: "ext",
       ext: "ext",
       magic: b"MAGIC",
       adjust: 0,
   },
   ```

3. Add to `marker_max()` and `validate()` match arms.

4. Add MAX constant at top of file.

5. Write tests in `carve.rs` test module:
   ```rust
   #[test]
   fn valid_<format>_is_carved() { ... }
   #[test]
   fn truncated_<format>_rejected() { ... }
   ```

6. Add a fuzz target once the `cargo-fuzz` harness lands (tracked in
   `docs/GAP_REPORT.md`); until then, cover malformed inputs with unit tests.

7. Run `cargo test -p relift-core` and `cargo clippy --workspace --all-targets -- -D warnings`.

## Adding a new filesystem parser

1. Create `crates/relift-core/src/fs/<fs>.rs` implementing:
   - `probe()` — detect from superblock/boot sector
   - `scan()` — enumerate deleted entries, emit via `CandidateSink`

2. Add module declaration in `fs/mod.rs`.

3. Add to `probe()` and `scan_metadata()` match arms.

4. Create fixture generator in `relift-testkit` if possible.

5. Add end-to-end test in `crates/relift-core/tests/end_to_end.rs`.

## Test fixtures

Fixtures live in `crates/relift-testkit/src/lib.rs`. Each fixture:
- Builds a real filesystem image byte-by-byte
- Contains live + deleted files with known SHA-256
- Documents deletion scenario (normal delete, format, partial overwrite, etc.)

To add a fixture:
1. Add builder struct in `relift-testkit`
2. Implement filesystem structures per spec
3. Export `*_image()` function returning `Fixture` with `image()` and `files`
4. Add test in `crates/relift-core/tests/end_to_end.rs`

## Code style

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- No `unwrap()`/`expect()` on external input in library code
- Use `thiserror` for error types, `anyhow` only at CLI boundary
- Structured logging with `tracing`
- All public items documented (`#![warn(missing_docs)]`)

## Commit messages

Follow Conventional Commits:
```
feat(carve): add WebP validator with VP8/VP8L chunk validation
fix(ntfs): handle sparse file segments in runlist decoder
docs: update CLI.md with new --filter syntax
test(fixtures): add fragmented JPEG test case
```

## Pull request checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] New carvers have validator + tests + fuzz target
- [ ] New filesystems have fixture + e2e test
- [ ] CHANGELOG.md updated (keepachangelog format)
- [ ] Docs updated if user-facing behavior changes

## Reporting issues

Use the issue templates:
- Bug report: include `relift doctor` output, OS, filesystem, scan mode, session file
- False positive/negative: provide fixture or steps to reproduce
- Feature request: explain use case and expected behavior

## Security

See `SECURITY.md`. Report vulnerabilities privately via GitHub Security Advisory.