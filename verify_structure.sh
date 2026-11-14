#!/bin/bash
# Verify Relift project structure
# Run from repo root

set -euo pipefail

echo "=== Relift Project Structure Verification ==="
echo

# Required directories
DIRS=(
    "crates/relift-core/src"
    "crates/relift-core/src/fs"
    "crates/relift-core/tests"
    "crates/relift-cli/src"
    "crates/relift-testkit/src"
    "crates/relift-gui/src-tauri/src"
    "crates/relift-gui/src/routes"
    "crates/relift-gui/src/lib/components"
    "crates/relift-gui/static"
    "crates/relift-bench/src"
    "docs/decisions"
    "docs/research"
    "docs/benches/results"
    ".github/workflows"
    ".github/ISSUE_TEMPLATE"
    "packaging"
    "fixtures"
    "growth"
)

echo "Checking directories..."
for dir in "${DIRS[@]}"; do
    if [ -d "$dir" ]; then
        echo "  ✓ $dir"
    else
        echo "  ✗ MISSING: $dir"
    fi
done

# Required files
FILES=(
    "Cargo.toml"
    "Cargo.lock"
    "rust-toolchain.toml"
    "rustfmt.toml"
    "clippy.toml"
    "deny.toml"
    ".editorconfig"
    ".gitignore"
    "README.md"
    "LICENSE"
    "CONTRIBUTING.md"
    "SECURITY.md"
    "CODE_OF_CONDUCT.md"
    "SUPPORT.md"
    "CHANGELOG.md"
    "CITATION.cff"
    "AGENTS.md"
    "ROADMAP.md"
    "GOVERNANCE.md"
    "IMPLEMENTATION_PLAN.md"
    "build.sh"
    "build.bat"
    "llms.txt"
    "llms-full.txt"
    "docs/architecture.md"
    "docs/CLI.md"
    "docs/GUI.md"
    "docs/filesystems.md"
    "docs/formats.md"
    "docs/forensic-use.md"
    "docs/benchmarks.md"
    "docs/troubleshooting.md"
    "docs/comparison.md"
    "docs/FAQ.md"
    "docs/support-matrix.md"
    "docs/limitations.md"
    "docs/threat-model.md"
    "docs/GAP_REPORT.md"
    "docs/VERIFICATION.md"
    "docs/decisions/ADR-0001-canonical-name-stack.md"
    "docs/decisions/ADR-0002-read-only-invariant.md"
    "docs/decisions/ADR-0003-sparse-segment-model.md"
    "docs/decisions/ADR-0004-carving-policy.md"
    "docs/decisions/ADR-0005-gui-deferral.md"
    "docs/decisions/ADR-0006-exit-code-grammar.md"
    "docs/decisions/CONFLICT_REGISTER.md"
    "docs/research/landscape.md"
    "docs/benches/METHODOLOGY.md"
    "docs/robots.txt"
    "crates/relift-core/Cargo.toml"
    "crates/relift-core/src/lib.rs"
    "crates/relift-core/src/scan.rs"
    "crates/relift-core/src/carve.rs"
    "crates/relift-core/src/fs/mod.rs"
    "crates/relift-core/src/fs/ntfs.rs"
    "crates/relift-core/src/fs/fat.rs"
    "crates/relift-core/src/fs/exfat.rs"
    "crates/relift-core/src/fs/ext.rs"
    "crates/relift-core/src/fs/apfs.rs"
    "crates/relift-core/src/fs/hfs.rs"
    "crates/relift-core/src/fs/btrfs.rs"
    "crates/relift-core/src/fs/xfs.rs"
    "crates/relift-core/src/devices.rs"
    "crates/relift-core/src/imaging.rs"
    "crates/relift-core/src/recover.rs"
    "crates/relift-core/src/report.rs"
    "crates/relift-core/src/session.rs"
    "crates/relift-core/src/source.rs"
    "crates/relift-core/src/safety.rs"
    "crates/relift-core/src/candidate.rs"
    "crates/relift-core/src/error.rs"
    "crates/relift-core/src/hash.rs"
    "crates/relift-core/src/partitions.rs"
    "crates/relift-core/src/util.rs"
    "crates/relift-core/tests/end_to_end.rs"
    "crates/relift-cli/Cargo.toml"
    "crates/relift-cli/src/main.rs"
    "crates/relift-testkit/Cargo.toml"
    "crates/relift-testkit/src/lib.rs"
    "crates/relift-gui/Cargo.toml"
    "crates/relift-gui/package.json"
    "crates/relift-gui/svelte.config.js"
    "crates/relift-gui/vite.config.ts"
    "crates/relift-gui/tsconfig.json"
    "crates/relift-gui/src/app.html"
    "crates/relift-gui/src/app.css"
    "crates/relift-gui/src/main.ts"
    "crates/relift-gui/src/App.svelte"
    "crates/relift-gui/src/routes/+layout.svelte"
    "crates/relift-gui/src/routes/+page.svelte"
    "crates/relift-gui/src/routes/scan/+page.svelte"
    "crates/relift-gui/src/routes/results/+page.svelte"
    "crates/relift-gui/src/routes/recover/+page.svelte"
    "crates/relift-gui/src/lib/components/Icons.svelte"
    "crates/relift-gui/src-tauri/Cargo.toml"
    "crates/relift-gui/src-tauri/tauri.conf.json"
    "crates/relift-gui/src-tauri/src/main.rs"
    "crates/relift-gui/src-tauri/icons/icon.svg"
    "crates/relift-gui/src-tauri/icons/README.md"
    "crates/relift-gui/static/favicon.svg"
    "crates/relift-gui/static/manifest.json"
    "crates/relift-bench/Cargo.toml"
    "crates/relift-bench/src/main.rs"
    ".github/workflows/ci.yml"
    ".github/CODEOWNERS"
    ".github/ISSUE_TEMPLATE/bug_report.yml"
    ".github/ISSUE_TEMPLATE/feature_request.yml"
    ".github/ISSUE_TEMPLATE/recovery-failure.md"
    ".github/PULL_REQUEST_TEMPLATE.md"
    "packaging/winget"
    "packaging/scoop"
    "packaging/homebrew"
    "packaging/aur"
    "packaging/snap"
    "packaging/flatpak"
)

echo
echo "Checking files..."
MISSING=0
for file in "${FILES[@]}"; do
    if [ -f "$file" ]; then
        echo "  ✓ $file"
    else
        echo "  ✗ MISSING: $file"
        MISSING=$((MISSING + 1))
    fi
done

echo
if [ $MISSING -eq 0 ]; then
    echo "=== All required files present! ==="
else
    echo "=== $MISSING files missing ==="
fi