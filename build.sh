#!/bin/bash
# Build verification script for Relift
# Run from repo root: ./build.sh

set -euo pipefail

echo "=== Relift Build Verification ==="
echo

echo "1. Checking Rust toolchain..."
rustc --version
cargo --version
echo

echo "2. Checking formatting..."
cargo fmt --all -- --check
echo "✓ Formatting OK"
echo

echo "3. Running clippy..."
cargo clippy --workspace --all-targets -- -D warnings
echo "✓ Clippy OK"
echo

echo "4. Running tests..."
cargo test --workspace --all-targets 2>&1 | tail -30
echo "✓ Tests OK"
echo

echo "5. Building release..."
cargo build --release --workspace
echo "✓ Build OK"
echo

echo "6. Checking binary size..."
ls -lh target/release/relift* 2>/dev/null || echo "Binaries not found (expected for first run)"
echo

echo "7. Running cargo deny..."
cargo deny check
echo "✓ Deny OK"
echo

echo "8. Building docs..."
cargo doc --workspace --no-deps
echo "✓ Docs OK"
echo

echo "=== All checks passed! ==="