# Verification status

This record separates local checks from product-release evidence. A successful synthetic test suite does not prove recovery accuracy on real devices.

## Latest local audit run (2026-11-28)

On the current Windows working tree, `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings` passed, and
`cargo test --workspace --all-targets` passed all 112 tests (85 core unit, 11
end-to-end, 8 CLI unit, 4 CLI contract, 2 GUI, and 2 testkit). These checks use
synthetic fixtures and do not measure real-media recovery accuracy. This is a
local run, not a replacement for the cross-platform CI evidence below.

## Gate status

| Gate | Requirement | Status | Evidence or remaining work |
|---|---|---|---|
| G1 | Linux, macOS, and Windows CI | PASS | [GitHub Actions run 43701255160](https://github.com/ShahabAhmed01/relift/actions/runs/43701255160): frontend, dependency/license, documentation, and Rust jobs passed across Linux, macOS, and Windows on stable and Rust 1.90 |
| G2 | Metadata recall and precision targets | PARTIAL | Deterministic FAT32/exFAT/NTFS/ext4 fixtures with byte-exact SHA-256 verification (end_to_end tests). Probe detection fixtures added for HFS+, APFS, Btrfs, XFS. No representative labeled corpus for recall/precision percentages yet. |
| G3 | Carving recall and precision targets | PARTIAL | 153 identifiers are registered, including marker-only entries; only a subset has structural validators and fixture tests. Validators vary in maturity. No representative labeled file corpus or measured precision/recall rates yet. |
| G4 | No source writes | PARTIAL | Read-only source abstraction and synthetic image hash checks; no physical-device before/after test |
| G5 | Refuse destination on source device | PARTIAL | Synthetic ancestry and destination tests; no mounted raw-device validation |
| G6 | Throughput targets | PARTIAL | Criterion baselines measured on Windows 11 x86_64, Rust 1.100: signature sweep 4.10-4.48 GiB/s (64 MiB buffer); JPEG carver validation 6.6-6.8 ms / 1 MiB; ZIP carver validation 6.3-6.5 ms / 1 MiB; NTFS metadata scan ~1.05 ms; ext4 metadata scan ~0.8 ms; FAT32 metadata scan ~0.17 ms; filesystem probes 37-64 µs. No published hardware baseline or 1 TB device measurement yet. |
| G7 | Quick scan of 1 TB NTFS under 90 seconds | PENDING | Not measured on a representative image and documented hardware |
| G8 | GUI cold start and large-result performance | PENDING | Cold start, accessibility, and 500,000-row behavior not measured |
| G9 | CLI JSON and exit-code contracts | PASS | CLI process-level contract tests; Tauri DTO casing contract tests |
| G10 | Resume interrupted scan after process termination | IMPLEMENTED | Volume-level checkpoint resume: `relift scan --resume` revalidates source size/fingerprint, skips completed volumes, and rescans the interrupted volume from its start. Verified by unit tests (cancelled-carve, multi-volume skip, mismatch refusal, completed-session refusal) and a real process-kill test whose resumed `candidates.jsonl` is byte-identical (SHA-256) to an uninterrupted scan. |
| G11 | 24-hour parser fuzzing | PARTIAL | cargo-fuzz harnesses for the carver and filesystem probes are checked in under fuzz/ and compile cleanly on nightly; a CI fuzz job runs bounded executions on Linux (nightly + clang). Local Windows execution is blocked by the missing clang/libFuzzer runtime DLL. Sustained 24-hour run and crash-minimized corpus remain pending |
| G12 | Signed and notarized platform releases | PENDING | Release assets are unsigned; credentials are not configured |
| G13 | Screenshots from the running GUI | PENDING | Existing visual assets have contradictory provenance; CLI images expose a local Windows profile path and at least one supposed GUI image is an editor/agent conversation. Re-capture and review before use. |
| G14 | Deployed, indexed documentation site | PENDING | No documentation site deployment or indexing evidence |
| G15 | Product and documentation honesty review | PARTIAL | Claims were reconciled with implementation; visual, accessibility, and external launch review remain |
| G16 | Installable package-manager releases | PENDING | Manifests are templates with placeholders; no upstream submissions |
| G17 | Launch materials and distribution | PENDING | No verified launch kit or launch execution |
| G18 | Current verification record | PASS | This file records current evidence and explicitly marks remaining gates |

## Local checks on 2026-11-15

Host: Windows 11 x86_64. Rust stable 1.100.0 and Rust 1.90.0 MSVC. Node.js 26.13.0.

| Check | Result |
|---|---|
| cargo test --workspace --all-targets --offline | PASS, 93 tests |
| cargo +1.90.0 test --workspace --all-targets --offline | PASS, 93 tests |
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

The current release workflow source passed [GitHub Actions run 43701255160](https://github.com/ShahabAhmed01/relift/actions/runs/43701255160). Frontend, dependency/license, documentation, and all configured Rust jobs passed, including Linux, macOS, and Windows with stable and Rust 1.90. The release workflow passed [run 43701736196](https://github.com/ShahabAhmed01/relift/actions/runs/43701736196), building 12 binaries and attaching a SHA256SUMS manifest. All 12 manifest entries were checked against the SHA-256 asset digests reported by GitHub. The benchmark job was skipped because the CI run did not request benchmark execution.

## Recovery evidence and limits

The Rust suite uses deterministic synthetic FAT32, exFAT, NTFS, and ext4 fixtures with byte-exact SHA-256 verification. Probe detection fixtures were added for HFS+, APFS, Btrfs, and XFS (identified but metadata recovery not implemented). Carving now includes structurally validated FLAC, OLE, MPEG-TS, RAR5, and ISO 9660 in addition to the previous 200+ formats. Tests check source-image immutability, destination separation, path confinement, CLI JSON contracts, exit codes, and Tauri DTO field names. These tests do not measure precision or recall on a representative corpus and do not exercise physical drives.

No physical device was accessed during this verification. macOS raw-device behavior remains untested, Windows raw-device enumeration requires elevation, and Linux destination ancestry tests are synthetic/CI-only.

## Release state

The [v0.1.0 GitHub preview](https://github.com/ShahabAhmed01/relift/releases/tag/v0.1.0) is published as a prerelease with 12 unsigned binaries and a SHA256SUMS manifest. The manifest matches GitHub's asset digests but does not authenticate the publisher. No signing or notarization credentials are configured. Package-manager files still contain placeholders and are not installable or submitted.

See docs/GAP_REPORT.md for the authoritative open work list and docs/support-matrix.md for implementation status.
