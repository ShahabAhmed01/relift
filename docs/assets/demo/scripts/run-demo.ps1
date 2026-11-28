[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $ReliftPath,

    [Parameter(Mandatory = $true)]
    [string] $DemoDirectory,

    [Parameter(Mandatory = $true)]
    [string] $RecoveryDestination,

    [switch] $WriteRecoveredFiles
)

$ErrorActionPreference = "Stop"

$relift = (Resolve-Path -LiteralPath $ReliftPath).Path
$demoRoot = [System.IO.Path]::GetFullPath($DemoDirectory)
New-Item -ItemType Directory -Path $demoRoot -Force | Out-Null

$runId = [DateTime]::UtcNow.ToString("yyyyMMdd-HHmmss") + "-" + [Guid]::NewGuid().ToString("N")
$runRoot = Join-Path $demoRoot ("run-" + $runId)
New-Item -ItemType Directory -Path $runRoot | Out-Null
$source = Join-Path $runRoot "evidence.img"
$manifest = Join-Path $runRoot "manifest.sha256"
$generatorManifest = Join-Path $PSScriptRoot "Cargo.toml"
Write-Host "Generating a fresh synthetic 128 MiB FAT32 image at $source"
& cargo run --quiet --manifest-path $generatorManifest -- $runRoot
if ($LASTEXITCODE -ne 0) { throw "Synthetic image generation failed" }

$destinationItem = Get-Item -LiteralPath $RecoveryDestination -ErrorAction Stop
if (-not $destinationItem.PSIsContainer) {
    throw "Recovery destination must be an existing directory"
}
$destination = $destinationItem.FullName

# Refuse a destination backed by the same physical disk as the source image.
# If the OS cannot map either path to a physical disk, fail closed.
try {
    $sourceDrive = [System.IO.Path]::GetPathRoot((Resolve-Path -LiteralPath $source).Path).Substring(0, 1)
    $destinationDrive = [System.IO.Path]::GetPathRoot($destination).Substring(0, 1)
    $sourceDisk = (Get-Partition -DriveLetter $sourceDrive -ErrorAction Stop | Select-Object -First 1).DiskNumber
    $destinationDisk = (Get-Partition -DriveLetter $destinationDrive -ErrorAction Stop | Select-Object -First 1).DiskNumber
} catch {
    throw "Could not prove the source image and recovery destination are on different physical disks: $_"
}
if ($sourceDisk -eq $destinationDisk) {
    throw "Choose a recovery destination on a different physical disk from the demo image"
}

$session = Join-Path $runRoot "session"
$sourceHashBefore = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash

& $relift --version
if ($LASTEXITCODE -ne 0) { throw "relift --version failed" }
& $relift doctor
if ($LASTEXITCODE -ne 0) { throw "relift doctor failed" }

& $relift scan $source --session $session --mode deep
if ($LASTEXITCODE -ne 0) { throw "scan failed with exit code $LASTEXITCODE" }

& $relift list $session
if ($LASTEXITCODE -ne 0) { throw "list failed with exit code $LASTEXITCODE" }

Write-Host "Dry-run recovery to $destination"
& $relift recover $session --dest $destination --dry-run
if ($LASTEXITCODE -ne 0) { throw "dry-run failed with exit code $LASTEXITCODE" }

if (-not $WriteRecoveredFiles) {
    Write-Host "Dry run complete. To write recovered files, rerun with -WriteRecoveredFiles and confirm RECOVER."
    return
}

$confirmation = Read-Host "Review the dry-run summary. Type RECOVER to write files to the separate destination"
if ($confirmation -cne "RECOVER") {
    Write-Host "Recovery cancelled; no recovered files were written."
    return
}

& $relift recover $session --dest $destination
if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 5) {
    throw "recovery failed with exit code $LASTEXITCODE"
}

$verification = & (Join-Path $PSScriptRoot "verify.ps1") `
    -ManifestPath $manifest `
    -RecoveryDirectory $destination
if ($verification.ExitCode -ne 0) {
    throw "Independent SHA-256 verification found missing original files"
}

$report = Join-Path $runRoot "report.html"
& $relift report $session --format html --out $report
if ($LASTEXITCODE -ne 0) { throw "report export failed with exit code $LASTEXITCODE" }

$sourceHashAfter = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
if ($sourceHashBefore -ne $sourceHashAfter) {
    throw "Synthetic source image hash changed during the demo"
}
Write-Host "Source image SHA-256 unchanged: $sourceHashAfter"
