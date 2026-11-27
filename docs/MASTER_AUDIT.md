# Master Audit - Relift

Date: 2026-11-27
Branch: fix/ci-and-claim-reconciliation (from main a7cb3e0)
Repository: https://github.com/ShahabAhmed01/relift
Auditor: Autonomous agent execution under MASTER EXECUTION PROMPT

## Executive summary

Relift is a GPL-3.0-or-later Rust workspace (relift-core, relift-cli, relift-testkit, relift-gui, relift-bench) with a Tauri 2 + Svelte 5 desktop GUI. It performs read-only deleted-file recovery from raw disk images and devices, recovering validated byte ranges only to a separate destination.

Overall condition: The codebase is functionally sound and safe. The two CI jobs that were failing on main (Dependency and license check, Documentation build) have been diagnosed and fixed at root cause on this branch. Documentation facts (test counts, format counts, support status) have been reconciled with actual code and test output. UTF-8 BOM and mojibake corruption in README.md has been fixed. Relative link checking and encoding guards have been added to CI.

No physical drives were accessed. All validation is against deterministic synthetic fixtures and loopback filesystem images. No real-media recovery accuracy has been established.

## P0-P4 risk register

ID   Severity  Area             Description                                                                              Status
----- --------- ---------------- ---------------------------------------------------------------------------------------- --------------------------------------------
R1    P0        Dependencies     chrono 0.4.45 unmaintained (RUSTSEC-2026-0543), url 2.5.8 stack overflow (RUSTSEC-2026-0592), cookie 0.18.2 fork nonce repeat (RUSTSEC-2026-0594) — all failing cargo deny check | FIXED on branch (chrono removed, url->2.5.9, cookie->0.18.3)
R2    P0        Documentation    cargo doc --workspace --no-deps failed with RUSTDOCFLAGS=-D warnings due to broken intra-doc links in testkit comments | FIXED on branch (escaped bracket references)
R3    P1        Claim drift      README said "200+ more" carving formats and "93 tests"; actual code registers 36 formats; current suite passes 94 tests | FIXED on branch
R4    P1        Claim drift      Support matrix said HFS+/APFS/Btrfs/XFS had "no fixture yet"; probe fixtures exist and are tested | FIXED on branch
R5    P1        Encoding         README.md had UTF-8 BOM and mojibake sequences | FIXED on branch
R6    P1        Link drift       10 broken relative links across README and docs | FIXED on branch
R7    P2        CI hygiene       GitHub Actions unpinned to @v4; Node.js 20 deprecation warnings on ubuntu-26 runners | FIXED on branch (pinned SHAs, Node 22)
R8    P3        Release          v0.1.0 assets unsigned, not notarized; no SBOM/attestations; packaging manifests contain placeholders | OPEN (owner gate - no publishing without approval)
R9    P3        GUI              No genuine screenshot evidence; 4 checked-in PNGs are identical VS Code window captures | OPEN
R10   P3        Discoverability  No GitHub topics set; homepage empty; description generic | OPEN

## Files affected (this branch)

### Source changes
- Cargo.toml — removed chrono workspace dependency
- crates/relift-core/Cargo.toml — removed chrono dependency
- crates/relift-core/src/util.rs — added unix_to_iso_nanos() and kept unix_to_iso(), filetime_to_iso(), fat_datetime_to_iso() (no chrono)
- crates/relift-core/src/fs/btrfs.rs — replaced chrono::DateTime::to_rfc3339() with unix_to_iso_nanos()
- crates/relift-core/src/fs/hfs.rs — replaced chrono::DateTime::to_rfc3339() with unix_to_iso()
- crates/relift-core/src/fs/xfs.rs — replaced chrono::DateTime::to_rfc3339() with unix_to_iso()
- crates/relift-core/src/carve.rs — added MAX_RAR4, "rar" => MAX_RAR4 marker entry, registered_format_count_is_stable() test
- crates/relift-core/src/fs/btrfs.rs, hfs.rs, xfs.rs — clippy formatting
- crates/relift-gui/src-tauri/Cargo.toml — removed chrono dependency
- crates/relift-testkit/src/lib.rs — escaped [/] in doc comments to fix rustdoc links
- Cargo.lock — regenerated (chrono removed, url->2.5.9, cookie->0.18.3)

### Documentation changes
- README.md — removed BOM and mojibake; updated carving claim to 36 formats; updated test count to 94; fixed relative links; added benchmark results section; honest preview status
- docs/src/README.md — updated carving format claim; added preview status banner
- docs/src/formats.md — accurate 36-format list; removed incorrect "200+ more"; documented not-emitted formats
- docs/src/support-matrix.md — corrected HFS+/APFS/Btrfs/XFS to "experimental (probe-tested)"; 36-format carver list; doc-drift guard note
- docs/src/filesystems.md — corrected experimental parser status (probe fixtures exist)
- docs/src/limitations.md — corrected experimental parser note
- docs/releases/v0.1.0.md — fixed relative links to support-matrix/verification
- docs/research/landscape.md — fixed relative links
- llms.txt / llms-full.txt — updated to reflect 36 formats, 94 tests, probe-tested status

### CI / workflow changes
- .github/workflows/ci.yml — pinned actions to SHAs; added repository-checks job (link check + encoding check); Node 22; timeouts
- .github/workflows/docs.yml — Pages deployment with pages: write / id-token: write; mdbook build step
- .github/workflows/release.yml — pinned action versions
- .github/dependabot.yml — new Dependabot config (cargo, npm, github-actions)
- .gitattributes — new LF normalization
- scripts/check-links.mjs — new relative link checker for CI
- scripts/check-encoding.mjs — new BOM/UTF-8/mojibake guard for CI

## Local verification results (Windows 11 x86_64, Rust 1.100.0 MSVC + 1.90.0 MSVC, Node 26.13.0)

Check                          Command                                                        Result
-----                          -------                                                        ------
Format                         cargo fmt --all -- --check                                     PASS
Lints                          cargo clippy --workspace --all-targets -- -D warnings            PASS
Tests                          cargo test --workspace --all-targets                           PASS, 94 tests, 0 failed
Rust docs                      cargo doc --workspace --no-deps (RUSTDOCFLAGS=-D warnings)     PASS
Deny check                     cargo deny check                                               PASS
Link check                     node scripts/check-links.mjs                                   PASS, 54 Markdown files
Encoding check                 node scripts/check-encoding.mjs                                PASS, 131 text files
Docs site                      mdbook build docs (0.4.35)                                     PASS
Release binaries               cargo build --release --workspace                              PASS (previously verified)

Test breakdown:
- relift-core lib: 73 passed
- relift-core end_to_end: 11 passed
- relift-cli: 6 passed (2 unit + 4 contract)
- relift-gui: 2 passed
- relift-testkit: 2 passed
- relift-bench: 0 tests (binary only)

Total: 94 tests, 0 failures.

## Safety invariants reviewed

1. Source read-only: ReadOnlySource exposes no write method. Platform open paths use GENERIC_READ only (Windows), O_RDONLY (Linux/macOS). No source-mutating code path exists in scan/recover.
2. Destination separation: safety.rs resolves destination ancestry and refuses same-device destinations (including partition/stacked-device ancestry on Linux).
3. Path confinement: Destination writes use canonical paths from volume root with no-follow directory opens; retained directory handles; no-replace rename (Linux/macOS) / no-overwrite move (Windows).
4. Session identity: Source size/fingerprint revalidated before resume; mismatched sources refused (exit code 4).
5. Recovery verification: SHA-256 of recovered file compared against bytes read from destination (proves write-back integrity, not original-file identity).
6. Carving: Magic-byte match alone never emitted; 36 formats each require structural validator acceptance. Random data produces zero false positives (tested).
7. Exit codes: 0 OK, 1 RUNTIME, 2 USAGE, 3 UNSAFE, 4 MISMATCH, 5 PARTIAL — preserved unchanged.

No safety boundary was weakened. No new write-capable paths were introduced.

## Remaining gaps (not addressed on this branch)

Area              Gap                                               Evidence state              Required action
----------------- ------------------------------------------------- ------------------------ ------------------------------------------------
Real-media validation | No physical drive, deleted volume, or representative corpus tested | Synthetic fixtures only        | Human-run protocol; owner gate (rule 3.1)
Release signing   | Assets unsigned; no Authenticode/notarization; no SBOM/attestations | v0.1.0 prerelease, 12 unsigned binaries | Owner approval for credentials/programs
Package managers  | Homebrew/winget/Scoop/AUR/Flatpak/Snap manifests contain placeholders | Templates only               | Owner approval per channel
GUI screenshots   | 4 checked-in PNGs are identical VS Code captures, not running GUI | Not genuine evidence         | Capture real GUI from synthetic fixture (owner gate if no display)
Search indexing   | Docs site deployed but indexing not confirmed                     | Deployment live; no Search Console data | Owner verification via Search Console/Bing
GitHub metadata   | No topics; empty homepage; no discussions                         | Public repo, 0 stars/forks/issues | Reversible metadata update (authorized)
Fuzzing           | Bounded CI fuzz runs (5000 runs each); no 24-hour campaign        | Harnesses compile; bounded CI job exists | Scheduled deeper fuzzing (optional)

## Human gates identified

1. GitHub metadata update (description, homepage, topics) — reversible, authorized; can proceed with gh API.
2. Release signing/notarization — requires credentials; owner must approve enrollment in any paid program.
3. Package-manager submissions — manifests are drafts; owner must approve submission.
4. Physical-media testing — prohibited by rule 3.1 on this agent; prepare human-run protocol if desired.
5. GUI screenshot capture — if no graphical desktop available, provide owner-run capture procedure.
6. Publishing to crates.io or any registry — requires owner approval.

## CI status on branch

All required checks pass locally. The branch has not been pushed to GitHub; CI has not been re-run on the remote. Push to fix/ci-and-claim-reconciliation to trigger remote CI validation. Do not push to main.
