$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$requiredDirectories = @(
    'crates/relift-core/src',
    'crates/relift-core/tests',
    'crates/relift-cli/src',
    'crates/relift-testkit/src',
    'crates/relift-gui/src-tauri/src',
    'crates/relift-gui/src/routes',
    'crates/relift-gui/src/lib/components',
    'crates/relift-bench/src',
    'docs/decisions',
    'docs/research',
    '.github/workflows',
    'packaging'
)
$requiredFiles = @(
    'Cargo.toml',
    'Cargo.lock',
    'rust-toolchain.toml',
    'README.md',
    'AGENTS.md',
    'verify_structure.bat',
    'verify_structure.ps1',
    'CONTRIBUTING.md',
    'SECURITY.md',
    'LICENSE',
    'docs/CLI.md',
    'docs/GUI.md',
    'docs/support-matrix.md',
    'docs/GAP_REPORT.md',
    'docs/VERIFICATION.md',
    'crates/relift-gui/package.json',
    'crates/relift-gui/package-lock.json',
    'crates/relift-gui/vite.config.ts',
    'crates/relift-gui/tsconfig.json',
    'crates/relift-gui/src/routes/+layout.ts',
    'crates/relift-gui/src-tauri/tauri.conf.json',
    '.github/workflows/ci.yml',
    '.github/workflows/release.yml',
    'packaging/homebrew/relift.rb',
    'packaging/scoop/relift.json',
    'packaging/winget/relift.vestige.yaml',
    'packaging/aur/PKGBUILD',
    'packaging/flatpak/com.github.ShahabAhmed01.relift.yml',
    'packaging/snap/snapcraft.yaml'
)
$missing = @()
foreach ($path in $requiredDirectories) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $path) -PathType Container)) {
        $missing += $path
    }
}
foreach ($path in $requiredFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $path) -PathType Leaf)) {
        $missing += $path
    }
}
if ($missing.Count -gt 0) {
    Write-Error ("Missing required project paths: " + ($missing -join ', '))
    exit 1
}
Write-Output ("Project structure verified: {0} directories and {1} files." -f $requiredDirectories.Count, $requiredFiles.Count)
exit 0
