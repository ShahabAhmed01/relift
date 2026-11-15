# Build Status

Last verified: 2026-11-15 on Windows 11 (x86_64), Rust 1.100.0 MSVC,
Node.js 26.13.0.

## Verified gates

| Gate | Command | Result |
|------|---------|--------|
| Formatting | `cargo fmt --all -- --check` | PASS |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | PASS |
| Tests | `cargo test --workspace --all-targets` | PASS — 51 tests (43 core unit, 6 end-to-end, 2 CLI) |
| MSRV | `cargo +1.90 check --workspace --all-targets` | PASS |
| Release build | `cargo build --release --workspace` | PASS |
| Frontend | `npm run build` in `crates/relift-gui` | PASS — static output in `crates/relift-gui/build` |
| GUI packaging | `npm run tauri build` in `crates/relift-gui` | Configured; bundles built by CI on tags |
| Icons | `npm run tauri icon src-tauri/icons/icon.svg` | PASS — ICO/ICNS/PNG set generated |

The only toolchain requirement on Windows is the MSVC toolchain with Visual
Studio Build Tools (C++ workload); MinGW is not used. Linux GUI builds require
the webkit2gtk-4.1 development packages listed in `.github/workflows/ci.yml`.

## Workspace layout

```
Cargo.toml                     workspace root (members below)
crates/relift-core             recovery engine
crates/relift-cli              `relift` CLI
crates/relift-testkit          deterministic fixture generators
crates/relift-gui              SvelteKit frontend (package.json, src/)
crates/relift-gui/src-tauri    Tauri 2 backend crate (Cargo.toml, tauri.conf.json)
crates/relift-bench            Criterion benchmarks
```

## Release process

1. Update `CHANGELOG.md` and the workspace version.
2. Run the verification gates above.
3. Commit, tag `vX.Y.Z`, and push the tag.
4. `.github/workflows/release.yml` builds desktop bundles and CLI archives for
   Linux x86_64/aarch64, Windows x86_64, and macOS x86_64/aarch64, then attaches
   them to a draft GitHub release.
5. Review the draft, add checksums, and publish.

The release workflow does not sign artifacts: no Authenticode, Developer ID, or
notarization credentials are configured.

## Known gaps

See `docs/GAP_REPORT.md` for the authoritative list. Release-blocking items
before a 1.0 claim are real-media validation, a checked-in fuzz harness, signed
artifacts, and package-manager submissions.
