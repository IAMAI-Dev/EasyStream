# Read-only Windows preflight for a host or controller. Works in Windows PowerShell 5.1+.
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Host', 'Client')]
    [string]$Role,
    [ipaddress]$PeerAddress,
    [ValidateRange(1, 65535)]
    [int]$BasePort = 47989,
    [string]$ResourcesRoot
)

$ErrorActionPreference = 'Stop'
if (-not $ResourcesRoot) {
    $ResourcesRoot = Join-Path $PSScriptRoot '..\src-tauri\resources'
}

function Test-Component([string]$Component, [string]$File, [string[]]$ExtraPaths = @()) {
    $paths = @(
        (Join-Path $ResourcesRoot "$Component\$File")
        (Join-Path $env:LOCALAPPDATA "EasyStream\components\$Component\$File")
    ) + $ExtraPaths
    if ($paths | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }) { return $true }
    return [bool](Get-Command -Name $File -CommandType Application -ErrorAction SilentlyContinue)
}

function Test-ServerInfo([ipaddress]$Address, [int]$Port) {
    $uri = [UriBuilder]::new('http', $Address.ToString(), $Port, '/serverinfo').Uri
    $webRequest = [System.Net.HttpWebRequest]::Create($uri)
    $webRequest.Proxy = $null
    $webRequest.Timeout = 3000
    $webRequest.ReadWriteTimeout = 3000
    $webRequest.AllowAutoRedirect = $false
    try {
        $response = $webRequest.GetResponse()
        try {
            $reader = [System.Xml.XmlReader]::Create($response.GetResponseStream(), [System.Xml.XmlReaderSettings]@{
                DtdProcessing = 'Prohibit'
                MaxCharactersInDocument = 65536
            })
            try {
                $info = New-Object System.Xml.XmlDocument
                $info.Load($reader)
            } finally { $reader.Dispose() }
            if ($info.root.status_code -eq '200' -and $info.root.uniqueid -and $info.root.state) {
                return 'GameStream responded'
            }
            return 'Unexpected response'
        } finally { $response.Close() }
    } catch [System.Net.WebException] {
        if ($_.Exception.Response) { $_.Exception.Response.Close() }
        return "Request failed ($($_.Exception.Status))"
    } catch [System.Xml.XmlException] {
        return 'Unexpected response'
    }
}

$result = [ordered]@{
    Role = $Role
    WindowsBuild = [Environment]::OSVersion.Version.Build
    EasyTierCoreFound = Test-Component 'easytier' 'easytier-core.exe'
    EasyTierCliFound = Test-Component 'easytier' 'easytier-cli.exe'
}
if ($Role -eq 'Host') {
    $service = Get-Service -Name SunshineService -ErrorAction SilentlyContinue
    $result.SunshineService = if ($service) { $service.Status.ToString() } else { 'Not found' }
    $result.SunshineMsiStaged = Test-Path -LiteralPath (Join-Path $ResourcesRoot 'sunshine\sunshine-windows-installer.msi') -PathType Leaf
    $result.LocalServerInfo = Test-ServerInfo ([ipaddress]::Loopback) $BasePort
} else {
    $moonlightPaths = @((Join-Path $env:ProgramFiles 'Moonlight Game Streaming\Moonlight.exe'))
    $result.MoonlightFound = Test-Component 'moonlight' 'Moonlight.exe' $moonlightPaths
}
if ($PeerAddress) {
    $result.PeerServerInfo = Test-ServerInfo $PeerAddress $BasePort
}
[pscustomobject]$result
