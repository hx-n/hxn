$ErrorActionPreference = 'Stop'
$installer = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../../install.ps1') -Raw
$root = Join-Path ([IO.Path]::GetTempPath()) ('hxn-installer-test-' + [Guid]::NewGuid())
$savedEnvironment = @{}
foreach ($key in @('OS', 'PROCESSOR_ARCHITEW6432', 'PROCESSOR_ARCHITECTURE', 'HXN_VERSION', 'HXN_INSTALL_DIR', 'LOCALAPPDATA', 'Path')) {
    $savedEnvironment[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
}
$savedUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$script:downloadPaths = @()
$script:requests = @()
$payload = [Text.Encoding]::UTF8.GetBytes('fixture hxn')

function Assert($condition, $message) {
    if (-not $condition) { throw "FAIL: $message" }
}

function New-Release($version, $mode = 'valid') {
    $folder = Join-Path $root $version
    $null = New-Item -ItemType Directory -Path $folder
    $sums = @()
    foreach ($architecture in @('x86_64', 'aarch64')) {
        $name = "hxn-$version-$architecture-pc-windows-msvc"
        $archive = Join-Path $folder "$name.zip"
        $zip = [IO.Compression.ZipFile]::Open($archive, 'Create')
        try {
            $binary = if ($mode -eq 'missing-binary') { 'missing.exe' } else { 'hxn.exe' }
            $entry = $zip.CreateEntry("$name/$binary")
            $stream = $entry.Open()
            try { $stream.Write($payload, 0, $payload.Length) } finally { $stream.Dispose() }
        } finally { $zip.Dispose() }
        $hash = if ($mode -eq 'corrupt') { '0' * 64 } else { (Get-FileHash $archive -Algorithm SHA256).Hash }
        $sums += "$hash  $name.zip"
    }
    if ($mode -eq 'missing-checksum') { $sums = @() }
    if ($mode -eq 'duplicate-checksum') { $sums = $sums + $sums }
    Set-Content -LiteralPath (Join-Path $folder 'SHA256SUMS') -Value ($sums -join "`n")
}

# Keep the actual download, verification, ZIP extraction and PATH code intact.
# Only HTTP is replaced with local release archives.
function Invoke-RestMethod($Uri, $ErrorAction) {
    Assert ($Uri -eq 'https://api.github.com/repos/hx-n/hxn/releases/latest') "unexpected API URL: $Uri"
    $script:requests += $Uri
    return @{ tag_name = 'v0.2.0' }
}

function Invoke-WebRequest($Uri, $OutFile, [switch]$UseBasicParsing, $ErrorAction) {
    Assert ($Uri -cmatch '^https://github.com/hx-n/hxn/releases/download/(v[0-9][a-zA-Z0-9._-]*)/([^/]+)$') "unexpected URL: $Uri"
    $version = $Matches[1]
    $file = $Matches[2]
    $script:requests += $Uri
    $script:downloadPaths += $OutFile
    Copy-Item -LiteralPath (Join-Path (Join-Path $root $version) $file) -Destination $OutFile
}

function Run-Installer($expectedError = '') {
    $script:downloadPaths = @()
    $script:requests = @()
    $beforePath = $env:Path
    $beforeUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $beforeTls = [Net.ServicePointManager]::SecurityProtocol
    $beforePreference = $ErrorActionPreference
    $failure = ''
    try { Invoke-Expression $installer } catch { $failure = $_.Exception.Message }
    if ($expectedError) {
        Assert ($failure.Contains($expectedError)) "expected '$expectedError', got '$failure'"
        Assert ($env:Path -eq $beforePath) 'a failed installation changed the current PATH'
        Assert ([Environment]::GetEnvironmentVariable('Path', 'User') -eq $beforeUserPath) 'a failed installation changed the user PATH'
    } else {
        Assert (-not $failure) $failure
    }
    Assert ([Net.ServicePointManager]::SecurityProtocol -eq $beforeTls) 'TLS configuration leaked'
    Assert ($ErrorActionPreference -eq $beforePreference) 'error preference leaked'
    foreach ($download in $script:downloadPaths) {
        Assert (-not (Test-Path -LiteralPath (Split-Path $download))) 'temporary files remain after installation'
    }
}

try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $null = New-Item -ItemType Directory -Path $root
    $env:OS = 'Windows_NT'
    $env:PROCESSOR_ARCHITEW6432 = ''
    $env:HXN_VERSION = ''
    $env:HXN_INSTALL_DIR = ''
    $env:LOCALAPPDATA = Join-Path $root 'local app data'
    $env:PROCESSOR_ARCHITECTURE = 'AMD64'
    New-Release 'v0.2.0'
    New-Release 'v0.1.0'

    Run-Installer
    $default = Join-Path $env:LOCALAPPDATA 'hxn/bin'
    Assert (([IO.File]::ReadAllText((Join-Path $default 'hxn.exe'))) -eq 'fixture hxn') 'default destination has the wrong binary'
    Assert ($script:requests[0] -like '*/latest') 'latest release was not resolved'
    Assert (@($env:Path -split ';') -contains $default) 'current PATH lacks the destination'
    if ([Environment]::OSVersion.Platform -eq 'Win32NT') {
        Assert (@([Environment]::GetEnvironmentVariable('Path', 'User') -split ';') -contains $default) 'persistent user PATH lacks the destination'
    }

    $env:HXN_VERSION = 'v0.1.0'
    $env:HXN_INSTALL_DIR = Join-Path $root "custom install with 'quotes'"
    $binary = Join-Path $env:HXN_INSTALL_DIR 'hxn.exe'
    foreach ($architecture in @('AMD64', 'ARM64')) {
        $env:PROCESSOR_ARCHITECTURE = $architecture
        Run-Installer
        $target = if ($architecture -eq 'AMD64') { 'x86_64' } else { 'aarch64' }
        Assert ($script:requests[0] -like "*/v0.1.0/*-$target-pc-windows-msvc.zip") "wrong target for $architecture"
        Assert (([IO.File]::ReadAllText($binary)) -eq 'fixture hxn') 'incorrect installed binary'
        Assert (@($script:requests | Where-Object { $_ -like '*/latest' }).Count -eq 0) 'pinned release resolved latest'
    }
    $env:PROCESSOR_ARCHITECTURE = 'x86'
    $env:PROCESSOR_ARCHITEW6432 = 'ARM64'
    Run-Installer
    Assert ($script:requests[0] -like '*-aarch64-pc-windows-msvc.zip') '32-bit shell did not use the native CPU'
    $env:PROCESSOR_ARCHITEW6432 = ''
    $env:PROCESSOR_ARCHITECTURE = 'AMD64'

    $cases = @(
        @('v9.0.0', 'corrupt', 'checksum mismatch'),
        @('v9.0.1', 'missing-checksum', 'no unique SHA-256'),
        @('v9.0.2', 'duplicate-checksum', 'no unique SHA-256'),
        @('v9.0.3', 'missing-binary', 'unique CLI binary')
    )
    foreach ($case in $cases) {
        New-Release $case[0] $case[1]
        $env:HXN_VERSION = $case[0]
        Run-Installer $case[2]
        Assert (([IO.File]::ReadAllText($binary)) -eq 'fixture hxn') 'a rejected archive replaced the installed CLI'
    }
    $env:HXN_VERSION = '../../bad'
    Run-Installer 'invalid release tag'
    $env:HXN_VERSION = 'v99.0.0'
    Run-Installer 'Cannot find path'
    $env:PROCESSOR_ARCHITECTURE = 'riscv64'
    Run-Installer 'unsupported architecture'
    $env:OS = 'Other'
    Run-Installer 'use https://hxn.dev/install.sh'

    Write-Host 'PASS: PowerShell x86-64/ARM64 and 32-bit shells, latest/pinned releases, paths and PATH updates,'
    Write-Host '      rejected checksums, missing assets, unsupported platforms, and temporary cleanup.'
} finally {
    foreach ($key in $savedEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($key, $savedEnvironment[$key], 'Process')
    }
    [Environment]::SetEnvironmentVariable('Path', $savedUserPath, 'User')
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
