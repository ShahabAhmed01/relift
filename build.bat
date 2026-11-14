@echo off
REM Build verification script for Relift (Windows)
REM Run from repo root: build.bat

echo === Relift Build Verification ===
echo.

echo 1. Checking Rust toolchain...
rustc --version
cargo --version
echo.

echo 2. Checking formatting...
cargo fmt --all -- --check
if errorlevel 1 (
    echo Formatting check failed!
    exit /b 1
)
echo ✓ Formatting OK
echo.

echo 3. Running clippy...
cargo clippy --workspace --all-targets -- -D warnings
if errorlevel 1 (
    echo Clippy check failed!
    exit /b 1
)
echo ✓ Clippy OK
echo.

echo 4. Running tests...
cargo test --workspace --all-targets
if errorlevel 1 (
    echo Tests failed!
    exit /b 1
)
echo ✓ Tests OK
echo.

echo 5. Building release...
cargo build --release --workspace
if errorlevel 1 (
    echo Build failed!
    exit /b 1
)
echo ✓ Build OK
echo.

echo 6. Checking binary size...
dir target\release\relift*.exe 2>nul || echo Binaries not found (expected for first run)
echo.

echo 7. Running cargo deny...
cargo deny check
if errorlevel 1 (
    echo Deny check failed!
    exit /b 1
)
echo ✓ Deny OK
echo.

echo 8. Building docs...
cargo doc --workspace --no-deps
if errorlevel 1 (
    echo Docs build failed!
    exit /b 1
)
echo ✓ Docs OK
echo.

echo === All checks passed! ===