[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $ManifestPath,

    [Parameter(Mandatory = $true)]
    [string] $RecoveryDirectory
)

$ErrorActionPreference = "Stop"
$manifestFile = (Resolve-Path -LiteralPath $ManifestPath).Path
$recoveryRoot = (Resolve-Path -LiteralPath $RecoveryDirectory).Path.TrimEnd('\', '/')
if (-not (Test-Path -LiteralPath $recoveryRoot -PathType Container)) {
    throw "Recovery directory is not a directory: $recoveryRoot"
}

$manifest = @{}
Get-Content -LiteralPath $manifestFile | ForEach-Object {
    if ($_ -match "^([0-9a-fA-F]{64})\s+(.+)$") {
        $manifest[$matches[1].ToLowerInvariant()] = $matches[2]
    }
}
if ($manifest.Count -eq 0) { throw "Manifest contains no SHA-256 entries" }

$recovered = @{}
$rootPrefix = $recoveryRoot + [System.IO.Path]::DirectorySeparatorChar
Get-ChildItem -LiteralPath $recoveryRoot -File -Recurse | ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $relativePath = $_.FullName.Substring($rootPrefix.Length)
    $recovered[$hash] = $relativePath
}

$matched = @{}
$missing = @()
$extra = @()
foreach ($hash in $manifest.Keys) {
    if ($recovered.ContainsKey($hash)) {
        $matched[$hash] = $manifest[$hash]
    } else {
        $missing += $manifest[$hash]
    }
}
foreach ($hash in $recovered.Keys) {
    if (-not $manifest.ContainsKey($hash)) { $extra += $recovered[$hash] }
}

Write-Host "Manifest entries: $($manifest.Count)"
Write-Host "Recovered files: $($recovered.Count)"
Write-Host "Byte-for-byte matches: $($matched.Count)"
Write-Host "Missing original files: $($missing.Count)"
Write-Host "Recovered files outside the deleted-file manifest: $($extra.Count)"
foreach ($path in $missing) { Write-Host "MISSING  $path" }

[pscustomobject]@{
    ExitCode = if ($missing.Count -eq 0) { 0 } else { 1 }
    Matched = $matched.Count
    Missing = $missing.Count
    Extra = $extra.Count
}
