# Improvement Report - Relift

Branch: fix/ci-and-claim-reconciliation
Commit: 0a75615
Base: main a7cb3e0
Date: 2026-11-27

## Summary of changes

All changes are on branch fix/ci-and-claim-reconciliation, committed as 0a75615. No changes were pushed to main. No PRs were opened or merged. No packages were published. No releases were created or overwritten.

### 1. CI stabilization (P0)

Dependency and license check - was failing on cargo deny check:
- chrono 0.4.45 unmaintained (RUSTSEC-2026-0543): removed chrono from the workspace and replaced its three usages in filesystem parsers with local date/time helpers in util.rs (unix_to_iso, unix_to_iso_nanos, filetime_to_iso, fat_datetime_to_iso). No safe upgrade path existed per the advisory.
- url 2.5.8 stack overflow in Url::origin() (RUSTSEC-2026-0592): upgraded to 2.5.9 via cargo update -p url --precise 2.5.9.
- cookie 0.18.2 private cookie nonce repeat (RUSTSEC-2026-0594): upgraded to 0.18.3 via cargo update -p cookie. Advisory notes affected apps only when fork() + private cookies on the forked thread; Relift does not fork.
- Added .github/dependabot.yml for weekly cargo/npm/GitHub-Actions dependency updates.

Documentation build - was failing with RUSTDOCFLAGS=-D warnings:
- crates/relift-testkit/src/lib.rs doc comments used [0x10040], [1026], [1027] which rustdoc interpreted as intra-doc links. Escaped them as \[0x10040\] etc.

CI workflow hardening:
- All GitHub Actions pinned to full commit SHAs with version comments:
  - actions/checkout -> 3d3c42e5aac5ba805825da76410c181273ba90b1 (v7.0.1)
  - actions/setup-node -> 949feb2413d6458794dcd2491c4babbbce0c15c1 (v7.1.0)
  - Swatinem/rust-cache -> d1527c8f67ef9af357290475d4af4b0fab8f12e0 (v2.9.3)
  - peaceiris/actions-gh-pages -> 84c30a85c19949d7eee79c4ff27748b70285e453 (v4.1.0)
- Node.js 22 (was 20, which is deprecated on ubuntu-26 runners).
- Added repository-checks job: runs scripts/check-links.mjs and scripts/check-encoding.mjs.
- Added per-job timeout-minutes.
- docs.yml: Pages deployment separated into build + deploy jobs with pages: write / id-token: write permissions.

### 2. Claim reconciliation (P1)

- Format count: README, docs/src/formats.md, docs/src/support-matrix.md, llms.txt, llms-full.txt all now state 36 registered carving formats (was "200+ more" in several places). Actual code registers 36 unique format identifiers.
- Test count: README and verification docs updated to 94 tests (was 93 / 82 conflicting records). Actual suite: 73 core + 11 end-to-end + 6 CLI + 2 GUI + 2 testkit = 94.
- Experimental parsers: support-matrix.md and filesystems.md corrected - HFS+/APFS/Btrfs/XFS are "experimental (probe-tested)" with deterministic probe fixtures, not "no fixture yet."
- Not-emitted formats: formats.md corrected - FLAC, OLE, MPEG-TS, RAR5, ISO 9660 are now registered and emitted (they were previously listed as not emitted in some docs).
- Docs links: fixed 10 broken relative links (README, release notes, landscape, formats).

### 3. Encoding and link guards (P1)

- Fixed README.md UTF-8 BOM and mojibake sequences (Windows-1252 misinterpretation of UTF-8 punctuation).
- Added scripts/check-links.mjs - validates all relative Markdown links in tracked .md files.
- Added scripts/check-encoding.mjs - guards against BOMs, NUL bytes, invalid UTF-8, and common mojibake sequences.
- Added .gitattributes - normalizes all text files to LF; marks images as binary.
- Both scripts run in CI repository-checks job.

### 4. Test addition (P2)

- crates/relift-core/src/carve.rs: registered_format_count_is_stable() - asserts exactly 36 unique registered carving formats, failing if the marker set changes without corresponding doc updates.
- Added MAX_RAR4 constant and "rar" => MAX_RAR4 marker_max entry (RAR4 previously fell through to the default limit).

### 5. Files changed

File                          Change type
----                          -----------
.gitattributes                new
.github/dependabot.yml        new
.github/workflows/ci.yml      modified
.github/workflows/docs.yml    modified
.github/workflows/release.yml modified
Cargo.lock                    modified
Cargo.toml                    modified
README.md                     modified
crates/relift-core/Cargo.toml modified
crates/relift-core/src/carve.rs modified
crates/relift-core/src/fs/btrfs.rs modified
crates/relift-core/src/fs/hfs.rs modified
crates/relift-core/src/fs/xfs.rs modified
crates/relift-core/src/util.rs modified
crates/relift-gui/src-tauri/Cargo.toml modified
crates/relift-testkit/src/lib.rs modified
docs/MASTER_AUDIT.md          new
docs/IMPROVEMENT_REPORT.md    new
docs/TEST_RESULTS.md          new
docs/releases/v0.1.0.md       modified
docs/research/landscape.md    modified
docs/src/README.md            modified
docs/src/filesystems.md       modified
docs/src/formats.md           modified
docs/src/limitations.md       modified
docs/src/support-matrix.md    modified
llms-full.txt                 modified
llms.txt                      modified
scripts/check-encoding.mjs    new
scripts/check-links.mjs       new

## Verification performed

All commands run locally on Windows 11 x86_64:

Command                          Result
-------                          ------
cargo fmt --all -- --check     PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --all-targets PASS, 94 tests, 0 failed
cargo doc --workspace --no-deps (RUSTDOCFLAGS=-D warnings) PASS
cargo deny check               PASS
mdbook build docs (0.4.35)     PASS
node scripts/check-links.mjs   PASS
node scripts/check-encoding.mjs PASS

No physical devices were accessed. All validation uses deterministic synthetic fixtures.

## What was deliberately NOT done

- No push to main (branch only).
- No PR opened or merged.
- No release created, tagged, or overwritten.
- No package published to crates.io, Homebrew, winget, Scoop, AUR, Flatpak, or Snap.
- No signing/notarization credentials applied.
- No physical-media testing.
- No GUI screenshot capture (no graphical desktop available in this environment).
- No external outreach or package-manager submissions.

## Next steps (require owner action or separate execution)

1. Push fix/ci-and-claim-reconciliation to trigger remote CI validation.
2. Open a PR from fix/ci-and-claim-reconciliation to main (owner reviews and merges).
3. Update GitHub repository metadata (description, homepage, topics) - reversible, authorized.
4. Capture genuine GUI screenshots from the running application using synthetic fixtures.
5. Prepare release signing/notarization steps (owner approval for credentials).
6. Submit package-manager manifests (owner approval per channel).
7. Create docs/RELEASE_READINESS.md follow-up for stable release.
