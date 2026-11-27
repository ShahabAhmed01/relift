# Independent recovery verification script.
# Compares every recovered file's SHA-256 against manifest.sha256 (computed
# BEFORE deletion, by genimg, never by Relift). Matches by content hash because
# carved/undeleted files may not keep their original names.
#
# Usage (from C:\relift-demo\demo): powershell -File verify.ps1

$ErrorActionPreference = "Stop"
$manifestPath = "manifest.sha256"
$outDir = "out"

if (-not (Test-Path $manifestPath)) { Write-Error "manifest.sha256 not found"; exit 1 }
if (-not (Test-Path $outDir)) { Write-Error "out/ not found; run recovery first"; exit 1 }

# Load manifest: "<hash>  <path>"
$manifest = @{}
Get-Content $manifestPath | ForEach-Object {
    if ($_ -match "^([0-9a-fA-F]{64})\s+(.+)$") {
        $manifest[$matches[1].ToLower()] = $matches[2]
    }
}
Write-Host "manifest entries: $($manifest.Count)"

# Hash every recovered file.
$recovered = @{}
Get-ChildItem -Path $outDir -File | ForEach-Object {
    $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
    $recovered[$h] = $_.Name
}
Write-Host "recovered files:  $($recovered.Count)"

# Match.
$matched = @{}
$missingOriginals = @()
$unmatchedRecovered = @()
foreach ($hash in $manifest.Keys) {
    $orig = $manifest[$hash]
    if ($recovered.ContainsKey($hash)) {
        $matched[$hash] = @{ original = $orig; recovered = $recovered[$hash] }
    } else {
        $missingOriginals += $orig
    }
}
foreach ($hash in $recovered.Keys) {
    if (-not $manifest.ContainsKey($hash)) {
        $unmatchedRecovered += @{ hash = $hash; file = $recovered[$hash] }
    }
}

Write-Host ""
Write-Host "=== MATCH (byte-for-byte, $($matched.Count)) ==="
foreach ($hash in $matched.Keys) {
    $m = $matched[$hash]
    Write-Host "  MATCH  $($m.original)  <-  $($m.recovered)"
}
Write-Host ""
Write-Host "=== MISSING (original not recovered, $($missingOriginals.Count)) ==="
foreach ($o in $missingOriginals) { Write-Host "  MISSING  $o" }
Write-Host ""
Write-Host "=== UNMATCHED RECOVERED (no original has this hash, $($unmatchedRecovered.Count)) ==="
foreach ($u in $unmatchedRecovered) { Write-Host "  EXTRA    $($u.file)  ($($u.hash))" }

$exit = 0
if ($missingOriginals.Count -gt 0) { $exit = 1 }
Write-Host ""
Write-Host "SUMMARY: matched=$($matched.Count) missing=$($missingOriginals.Count) extra_recovered=$($unmatchedRecovered.Count)"
exit $exit
