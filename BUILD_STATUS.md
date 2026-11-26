# Build status

Last verified: 2026-11-15 on Windows 11 x86_64.
Toolchains: Rust 1.100.0 MSVC, Rust 1.90.0 MSVC, Node.js 26.13.0.

## Local verification

| Check | Result |
|---|---|
| Rust formatting | PASS: cargo fmt --all -- --check |
| Rust lints | PASS: cargo clippy --workspace --all-targets -- -D warnings |
| Workspace tests | PASS on stable and Rust 1.90.0: 82 tests |
| Rust documentation | PASS: cargo doc --workspace --no-deps |
| Frontend dependency install | PASS: npm ci |
| Svelte and TypeScript checks | PASS: npm run check, 0 errors and 0 warnings |
| Frontend production build | PASS: npm run build |
| Production dependency audit | PASS: npm audit --omit=dev --audit-level=low |
| Windows Tauri packages | PASS: MSI and NSIS installers built locally |
| Full development dependency audit | One low-severity transitive deepmerge advisory remains; no upstream patched version is available |

The current release-workflow source passed [cross-platform CI run 43701255160](https://github.com/ShahabAhmed01/relift/actions/runs/43701255160), including frontend, dependency/license, documentation, and Linux/macOS/Windows Rust jobs on stable and Rust 1.90. The [v0.1.0 release workflow](https://github.com/ShahabAhmed01/relift/actions/runs/43701736196) built all 12 binaries and generated SHA256SUMS; its entries were checked against GitHub's asset digests. The published [v0.1.0 preview](https://github.com/ShahabAhmed01/relift/releases/tag/v0.1.0) is unsigned and not notarized. The benchmark job was skipped because CI did not request benchmark execution.

## Workspace

- crates/relift-core: recovery engine and filesystem parsers
- crates/relift-cli: command-line interface
- crates/relift-testkit: deterministic synthetic fixtures
- crates/relift-gui: SvelteKit frontend
- crates/relift-gui/src-tauri: Tauri desktop backend
- crates/relift-bench: Criterion benchmarks

## Remaining release gates

See docs/GAP_REPORT.md and docs/VERIFICATION.md. Physical-media validation,
representative accuracy data, long-duration fuzzing, hardware performance
targets, macOS raw-device testing, signing and notarization, and package-manager
submissions remain outstanding. Checkpoint-based scan resume (G10) is
implemented and verified.
