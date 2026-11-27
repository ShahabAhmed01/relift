# Verification report

Date: 2026-11-15
Host: Windows 11 x86_64
Toolchains: Rust 1.100.0 MSVC, Rust 1.90.0 MSVC, Node.js 26.13.0

## Checks completed

- cargo test --workspace --all-targets: passed, 94 tests.
- cargo +1.90.0 test --workspace --all-targets --offline: passed, 94 tests.
- cargo clippy --workspace --all-targets -- -D warnings: passed.
- cargo fmt --all -- --check: passed.
- cargo doc --workspace --no-deps: passed.
- npm ci: passed from the checked-in lockfile.
- npm run check: passed with 0 errors and 0 warnings.
- npm run build: passed.
- npm audit --omit=dev --audit-level=low: passed with no production dependency advisories.
- npm run tauri build: produced Windows MSI and NSIS installers.

The complete npm audit reports one low-severity advisory in the development-only transitive dependency deepmerge. The upstream advisory lists no patched release. Details are in docs/GAP_REPORT.md.

## Safety and recovery evidence

The test suite uses deterministic synthetic fixtures. It covers byte comparison, read-only image scanning, destination separation and path confinement. No physical drive or real deleted-volume corpus was used. These tests do not establish real-media accuracy or validate raw-device behavior on every operating system.

## Release state

The v0.1.0 GitHub preview is published with 12 cross-platform binaries and a SHA256SUMS manifest. Assets are unsigned and macOS bundles are not notarized; checksums match the uploaded files but do not authenticate the publisher. No stable release, signing certificates, notarization credentials, or accepted package-manager submissions are available.

The repository is not 100% release-ready. Open gates and their evidence requirements are tracked in docs/GAP_REPORT.md and docs/VERIFICATION.md.
