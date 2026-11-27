# Installation

## From Source (Rust)

Requires Rust 1.90 or newer. On Windows, the MSVC toolchain is recommended:

```sh
rustup default stable-x86_64-pc-windows-msvc
cargo build --release --workspace
```

The CLI binary is `target/release/relift.exe` (Windows) or `target/release/relift` (Unix).

## Desktop GUI (Tauri)

The GUI requires Node.js 22.17 or newer:

```sh
cd crates/relift-gui
npm ci
npm run tauri build
```

This produces Windows MSI and NSIS installers in `target/release/bundle/`.

## Prebuilt Binaries

Prebuilt binaries are available on the [Releases page](https://github.com/ShahabAhmed01/relift/releases). The v0.1.0 preview includes:

- Windows: MSI and NSIS installers (x86_64)
- macOS: x86_64 and arm64 bundles
- Linux: x86_64 and ARM64 CLI archives
- SHA256SUMS manifest for all assets

All preview assets are unsigned; hashes detect byte mismatches but do not authenticate the publisher.

## Package Managers

Package-manager manifests (Homebrew, winget, Scoop, AUR, Flatpak, Snap) are templates and have not yet been submitted to upstream repositories.
