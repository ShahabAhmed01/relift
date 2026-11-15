# Verification status

This record separates local checks from product-release evidence. A successful synthetic test suite does not prove recovery accuracy on real devices.

## Gate status

| Gate | Requirement | Status | Evidence or remaining work |
|---|---|---|---|
| G1 | Linux, macOS, and Windows CI | PENDING | The last completed CI run predates the current frontend migration; record the new GitHub Actions run after push |
| G2 | Metadata recall and precision targets | PENDING | No representative labeled filesystem corpus |
| G3 | Carving recall and precision targets | PENDING | No representative labeled file corpus |
| G4 | No source writes | PARTIAL | Read-only source abstraction and synthetic image hash checks; no physical-device before/after test |
| G5 | Refuse destination on source device | PARTIAL | Synthetic ancestry and destination tests; no mounted raw-device validation |
| G6 | Throughput targets | PARTIAL | Benchmark harness smoke runs only; no published hardware baseline |
| G7 | Quick scan of 1 TB NTFS under 90 seconds | PENDING | Not measured on a representative image and documented hardware |
| G8 | GUI cold start and large-result performance | PENDING | Cold start, accessibility, and 500,000-row behavior not measured |
| G9 | CLI JSON and exit-code contracts | PASS | CLI process-level contract tests; Tauri DTO casing contract tests |
| G10 | Resume interrupted scan after process termination | NOT IMPLEMENTED | Checkpoint-based scan resume is not a feature |
| G11 | 24-hour parser fuzzing | PENDING | No checked-in cargo-fuzz harness or long-run logs |
| G12 | Signed and notarized platform releases | PENDING | Release assets are unsigned; credentials are not configured |
| G13 | Screenshots from the running GUI | PENDING | No screenshot evidence checked in |
| G14 | Deployed, indexed documentation site | PENDING | No documentation site deployment or indexing evidence |
| G15 | Product and documentation honesty review | PARTIAL | Claims were reconciled with implementation; visual, accessibility, and external launch review remain |
| G16 | Installable package-manager releases | PENDING | Manifests are templates with placeholders; no upstream submissions |
| G17 | Launch materials and distribution | PENDING | No verified launch kit or launch execution |
| G18 | Current verification record | PASS | This file records current evidence and explicitly marks remaining gates |

## Local checks on 2026-11-15

Host: Windows 11 x86_64. Rust stable 1.100.0 and Rust 1.90.0 MSVC. Node.js 26.13.0.

| Check | Result |
|---|---|
| cargo test --workspace --all-targets --offline | PASS, 74 tests |
| cargo +1.90.0 test --workspace --all-targets --offline | PASS, 74 tests |
| cargo clippy --workspace --all-targets -- -D warnings | PASS |
| cargo +1.90.0 doc --workspace --no-deps --offline | PASS |
| cargo fmt --all -- --check | PASS |
| npm ci in crates/relift-gui | PASS |
| npm run check | PASS, 0 errors and 0 warnings |
| npm run build | PASS |
| npm audit --omit=dev --audit-level=low | PASS, no production dependency advisories |
| npm run tauri build on Windows | PASS, produced MSI and NSIS installers |
| cmd /d /c verify_structure.bat | PASS |

The full npm development dependency audit reports one low-severity advisory in the transitive deepmerge package used by the Svelte Vite plugin. The upstream advisory lists no patched release. The package is development-only and is not included in the desktop runtime; see docs/GAP_REPORT.md.

## CI evidence

The last completed cross-platform run before the current changes is [run 43691011208](https://github.com/ShahabAhmed01/relift/actions/runs/43691011208). It passed its configured Rust and dependency checks. The current revision adds a GUI frontend CI job and updates the release workflow to Node.js 22; G1 remains pending until those changes pass on GitHub Actions.

## Recovery evidence and limits

The Rust suite uses deterministic synthetic FAT32, exFAT, NTFS, and ext4 fixtures. It checks selected recovery bytes against expected data, source-image immutability in tests, destination separation, path confinement, CLI JSON contracts, exit codes, and Tauri DTO field names. These tests do not measure precision or recall on a representative corpus and do not exercise physical drives.

No physical device was accessed during this verification. macOS raw-device behavior remains untested, Windows raw-device enumeration requires elevation, and Linux destination ancestry tests are synthetic/CI-only.

## Release state

The public v0.1.0 GitHub release is a draft, not a published stable release. Locally built Windows installers are unsigned. Package-manager files contain explicit placeholders and must not be submitted until release assets, source URLs, checksums, and install behavior are independently verified.

See docs/GAP_REPORT.md for the authoritative open work list and docs/support-matrix.md for implementation status.
