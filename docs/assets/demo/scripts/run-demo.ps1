# Relift demo run sequence (PowerShell).
# Run from: C:\relift-demo\demo   (relative paths; no personal paths)
# All commands run live against a freshly built `relift` (release).
# Exit codes are shown after each command.

$relift = "C:\path\to\relift\target\release\relift.exe"   # edit to your build

# 1. version + doctor
& $relift --version
& $relift doctor

# 2. evidence hash (pre-scan), then deep scan (metadata + carving)
Get-FileHash evidence.img -Algorithm SHA256
& $relift scan evidence.img --session ./session --mode deep

# 3. list candidates
& $relift list ./session

# 4. dry-run recovery (nothing written)
& $relift recover ./session --dest ./out --dry-run

# 5. real recovery
& $relift recover ./session --dest ./out

# 6. independent hash check vs manifest (authored before deletion)
powershell -NoProfile -ExecutionPolicy Bypass -File verify.ps1

# 7. evidence hash again (must equal step 2 -> read-only proof)
Get-FileHash evidence.img -Algorithm SHA256

# 8. report export
& $relift report ./session --format html --out ./report.html

# 9. JSON output
& $relift list ./session --json

Expected (this session's genuine result):
- scan: FAT32 detected, 11 deleted metadata candidates, 75 carved, 86 total, exit 0
- recover: 86/86 recovered, exit 0
- verify: 11 deleted files byte-for-byte MATCH; 4 live controls correctly not recovered
- image hash: unchanged before/after
- exit codes: all 0 (except the deliberate destination-refusal test -> 3)
