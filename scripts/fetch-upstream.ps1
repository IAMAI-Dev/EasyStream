<#
.SYNOPSIS
    Downloads, verifies SHA-256, and stages the pinned Windows x64 upstream binaries
    for EasyStream:
      - EasyTier v2.6.4 (easytier-core.exe, easytier-cli.exe)
      - Sunshine v2026.914.233613 (Windows x64 MSI)
      - Moonlight Qt v6.2.0 (Windows x64 Portable ZIP)
#>

[CmdletBinding()]
param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ResourcesRoot = Join-Path $RepoRoot "src-tauri\resources"
$ManifestPath = Join-Path $ResourcesRoot "manifest.json"
$TempDir = Join-Path $env:TEMP "easystream-upstream-fetch"
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
$manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json

function Get-VerifiedAsset {
    param([string]$Repo, $Component)
    $name = $Component.asset_filename
    $expected = $Component.sha256
    if ($expected -notmatch '^[a-f0-9]{64}$' -or [IO.Path]::GetFileName($name) -cne $name) {
        throw "Invalid release pin for $Repo"
    }
    $path = Join-Path $TempDir $name
    if ($Force -or -not (Test-Path -LiteralPath $path -PathType Leaf)) {
        $url = "https://github.com/$Repo/releases/download/$($Component.tag)/$name"
        $partial = Join-Path $TempDir ([guid]::NewGuid().ToString() + '.part')
        try {
            Invoke-WebRequest -Uri $url -OutFile $partial -UseBasicParsing
            if ((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $expected) {
                throw "SHA-256 mismatch for downloaded $name"
            }
            Move-Item -LiteralPath $partial -Destination $path -Force
        } finally {
            if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
        }
    }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $expected) {
        throw "SHA-256 mismatch for cached $name. Run again with -Force to download a fresh copy."
    }
    return $path
}

# Verify all three inputs before modifying bundled resources.
$etZip = Get-VerifiedAsset 'EasyTier/EasyTier' $manifest.components.easytier
$ssMsi = Get-VerifiedAsset 'SunshineStream/Sunshine' $manifest.components.sunshine
$mlZip = Get-VerifiedAsset 'moonlight-stream/moonlight-qt' $manifest.components.moonlight
$extract = New-Item -ItemType Directory -Path (Join-Path $TempDir ([guid]::NewGuid().ToString()))
try {
    $etExtract = Join-Path $extract.FullName 'easytier'
    $mlExtract = Join-Path $extract.FullName 'moonlight'
    Expand-Archive -LiteralPath $etZip -DestinationPath $etExtract
    Expand-Archive -LiteralPath $mlZip -DestinationPath $mlExtract
    $mlExe = @(Get-ChildItem -LiteralPath $mlExtract -Recurse -Filter Moonlight.exe -File)
    if ($mlExe.Count -ne 1) { throw "Expected one Moonlight.exe in portable archive" }
    $etCore = Get-ChildItem -Path $etExtract -Recurse -Filter "easytier-core.exe" | Select-Object -First 1
    $etCli = Get-ChildItem -Path $etExtract -Recurse -Filter "easytier-cli.exe" | Select-Object -First 1
    if (-not $etCore -or -not $etCli) {
        throw "easytier-core.exe or easytier-cli.exe missing from archive."
    }
    $etRuntime = @($etCore, $etCli)
    foreach ($name in 'wintun.dll', 'Packet.dll', 'WinDivert64.sys') {
        $path = Join-Path $etCore.DirectoryName $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "EasyTier release is missing $name."
        }
        $etRuntime += Get-Item -LiteralPath $path
    }
    foreach ($item in $etRuntime) {
        Copy-Item -LiteralPath $item.FullName -Destination (Join-Path $ResourcesRoot 'easytier') -Force
    }
    Copy-Item -LiteralPath $ssMsi -Destination (Join-Path $ResourcesRoot 'sunshine\sunshine-windows-installer.msi') -Force
    $mlTarget = (Get-Item -LiteralPath (Join-Path $ResourcesRoot 'moonlight')).FullName
    foreach ($item in (Get-ChildItem -LiteralPath $mlTarget -Force)) {
        if ($item.Name -eq '.gitkeep') { continue }
        if ([IO.Path]::GetDirectoryName($item.FullName) -ne $mlTarget) {
            throw "Unexpected Moonlight resource path"
        }
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Unexpected resource link" }
        Remove-Item -LiteralPath $item.FullName -Recurse -Force
    }
    Get-ChildItem -LiteralPath $mlExe[0].DirectoryName -Force |
        ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $mlTarget -Recurse -Force }
} finally {
    if ($extract.Parent.FullName -eq [IO.Path]::GetFullPath($TempDir)) {
        Remove-Item -LiteralPath $extract.FullName -Recurse -Force
    }
}
Write-Host 'All pinned x64 assets verified and staged.'
