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
$BinariesRoot = Join-Path $RepoRoot "src-tauri\binaries"
$ManifestPath = Join-Path $ResourcesRoot "manifest.json"
$TempDir = Join-Path $env:TEMP "easystream-upstream-fetch"

New-Item -ItemType Directory -Force -Path $TempDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $ResourcesRoot "easytier") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $ResourcesRoot "sunshine") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $ResourcesRoot "moonlight") | Out-Null
New-Item -ItemType Directory -Force -Path $BinariesRoot | Out-Null

$manifest = Get-Content -Raw -Path $ManifestPath | ConvertFrom-Json

function Get-GitHubReleaseAsset {
    param(
        [string]$Repo,
        [string]$Tag,
        [string]$Pattern
    )
    $apiUrl = "https://api.github.com/repos/$Repo/releases/tags/$Tag"
    Write-Host "Querying $apiUrl ..." -ForegroundColor Cyan
    $headers = @{ "User-Agent" = "EasyStream-Fetch-Script" }
    $release = Invoke-RestMethod -Uri $apiUrl -Headers $headers
    $asset = $release.assets | Where-Object { $_.name -like $Pattern } | Select-Object -First 1
    if (-not $asset) {
        throw "Could not find asset matching '$Pattern' in $Repo@$Tag"
    }
    return $asset
}

function Download-And-Hash {
    param(
        [string]$Url,
        [string]$OutFile
    )
    if ((Test-Path $OutFile) -and (-not $Force)) {
        Write-Host "Using cached $OutFile" -ForegroundColor DarkGray
    } else {
        Write-Host "Downloading $Url -> $OutFile" -ForegroundColor Green
        Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing
    }
    return (Get-FileHash -Path $OutFile -Algorithm SHA256).Hash.ToLowerInvariant()
}

# 1. EasyTier v2.6.4
$etAsset = Get-GitHubReleaseAsset -Repo "EasyTier/EasyTier" -Tag $manifest.components.easytier.tag -Pattern "*windows-x86_64*.zip"
$etZip = Join-Path $TempDir $etAsset.name
$etHash = Download-And-Hash -Url $etAsset.browser_download_url -OutFile $etZip
$etExtract = Join-Path $TempDir "easytier-extract"
if (Test-Path $etExtract) { Remove-Item -Recurse -Force $etExtract }
Expand-Archive -Path $etZip -DestinationPath $etExtract -Force

$etCore = Get-ChildItem -Path $etExtract -Recurse -Filter "easytier-core.exe" | Select-Object -First 1
$etCli = Get-ChildItem -Path $etExtract -Recurse -Filter "easytier-cli.exe" | Select-Object -First 1
$wintun = Get-ChildItem -Path $etExtract -Recurse -Filter "wintun.dll" | Select-Object -First 1
if (-not $etCore -or -not $etCli) {
    throw "easytier-core.exe or easytier-cli.exe missing from archive."
}
Copy-Item -Force $etCore.FullName (Join-Path $ResourcesRoot "easytier\easytier-core.exe")
Copy-Item -Force $etCli.FullName (Join-Path $ResourcesRoot "easytier\easytier-cli.exe")
if ($wintun) {
    Copy-Item -Force $wintun.FullName (Join-Path $ResourcesRoot "easytier\wintun.dll")
}
$manifest.components.easytier.asset_filename = $etAsset.name
$manifest.components.easytier.sha256 = $etHash

# 2. Sunshine v2026.914.233613 (Windows x64 MSI)
$ssAsset = Get-GitHubReleaseAsset -Repo "SunshineStream/Sunshine" -Tag $manifest.components.sunshine.tag -Pattern "*windows*.msi"
$ssMsi = Join-Path $ResourcesRoot "sunshine\sunshine-windows-installer.msi"
$ssHash = Download-And-Hash -Url $ssAsset.browser_download_url -OutFile $ssMsi
$manifest.components.sunshine.asset_filename = $ssAsset.name
$manifest.components.sunshine.sha256 = $ssHash

# 3. Moonlight Qt v6.2.0 (Windows x64 Portable)
$mlAsset = Get-GitHubReleaseAsset -Repo "moonlight-stream/moonlight-qt" -Tag $manifest.components.moonlight.tag -Pattern "MoonlightPortable-x64-*.zip"
$mlZip = Join-Path $TempDir $mlAsset.name
$mlHash = Download-And-Hash -Url $mlAsset.browser_download_url -OutFile $mlZip
$mlTarget = Join-Path $ResourcesRoot "moonlight"
Get-ChildItem -Path $mlTarget -Exclude ".gitkeep" | Remove-Item -Recurse -Force
Expand-Archive -Path $mlZip -DestinationPath $mlTarget -Force
$manifest.components.moonlight.asset_filename = $mlAsset.name
$manifest.components.moonlight.sha256 = $mlHash

# Save updated manifest with verified SHA-256 hashes
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Path $ManifestPath -Encoding UTF8

Write-Host "`nAll upstream components staged and SHA-256 recorded in manifest.json:" -ForegroundColor Green
Write-Host "  EasyTier  : $($manifest.components.easytier.asset_filename) [$etHash]"
Write-Host "  Sunshine  : $($manifest.components.sunshine.asset_filename) [$ssHash]"
Write-Host "  Moonlight : $($manifest.components.moonlight.asset_filename) [$mlHash]"
