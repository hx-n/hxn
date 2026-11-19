# Install with: irm https://hxn.dev/install.ps1 | iex
# HXN_VERSION selects a release tag. HXN_INSTALL_DIR overrides the destination.
& {
    $ErrorActionPreference = 'Stop'
    if ($env:OS -ne 'Windows_NT') {
        throw 'hxn: use https://hxn.dev/install.sh on macOS or Linux'
    }

    # PROCESSOR_ARCHITEW6432 identifies the native CPU in a 32-bit shell.
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
    switch ($architecture) {
        'AMD64' { $target = 'x86_64-pc-windows-msvc' }
        'ARM64' { $target = 'aarch64-pc-windows-msvc' }
        default { throw "hxn: unsupported architecture: $architecture" }
    }

    $directory = $env:HXN_INSTALL_DIR
    if (-not $directory) {
        if (-not $env:LOCALAPPDATA) { throw 'hxn: LOCALAPPDATA is not set' }
        $directory = Join-Path $env:LOCALAPPDATA 'hxn\bin'
    }
    $directory = [IO.Path]::GetFullPath($directory)
    $temporary = Join-Path ([IO.Path]::GetTempPath()) ('hxn-install-' + [Guid]::NewGuid())
    $previousTls = [Net.ServicePointManager]::SecurityProtocol
    try {
        [Net.ServicePointManager]::SecurityProtocol = $previousTls -bor [Net.SecurityProtocolType]::Tls12
        $version = $env:HXN_VERSION
        if (-not $version) {
            $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/hx-n/hxn/releases/latest' -ErrorAction Stop
            $version = $release.tag_name
        }
        if ($version -cnotmatch '^v[0-9][a-zA-Z0-9._-]*$') {
            throw "hxn: invalid release tag: $version"
        }

        $null = New-Item -ItemType Directory -Path $temporary
        $name = "hxn-$version-$target"
        $archive = "$name.zip"
        $url = "https://github.com/hx-n/hxn/releases/download/$version"
        $archivePath = Join-Path $temporary $archive
        $sumsPath = Join-Path $temporary 'SHA256SUMS'
        Write-Host "Downloading hxn $version for $target..."
        Invoke-WebRequest -UseBasicParsing -Uri "$url/$archive" -OutFile $archivePath -ErrorAction Stop
        Invoke-WebRequest -UseBasicParsing -Uri "$url/SHA256SUMS" -OutFile $sumsPath -ErrorAction Stop
        $checksums = @(Get-Content -LiteralPath $sumsPath | ForEach-Object {
            if ($_ -cmatch ('^([a-fA-F0-9]{64})\s+\*?' + [regex]::Escape($archive) + '$')) {
                $Matches[1]
            }
        })
        if ($checksums.Count -ne 1) {
            throw "hxn: no unique SHA-256 checksum for $archive"
        }
        $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
        if ($actual -ine $checksums[0]) {
            throw "hxn: checksum mismatch for $archive; nothing installed"
        }

        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [IO.Compression.ZipFile]::OpenRead($archivePath)
        try {
            $entries = @($zip.Entries | Where-Object { $_.FullName -ceq "$name/hxn.exe" })
            if ($entries.Count -ne 1) {
                throw 'hxn: the release archive does not contain a unique CLI binary'
            }
            $binary = Join-Path $temporary 'hxn.exe'
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entries[0], $binary)
        } finally {
            $zip.Dispose()
        }
        $null = New-Item -ItemType Directory -Path $directory -Force
        Copy-Item -LiteralPath $binary -Destination (Join-Path $directory 'hxn.exe') -Force

        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (@($userPath -split ';') -notcontains $directory) {
            $updatedPath = if ($userPath) { "$($userPath.TrimEnd(';'));$directory" } else { $directory }
            [Environment]::SetEnvironmentVariable('Path', $updatedPath, 'User')
        }
        if (@($env:Path -split ';') -notcontains $directory) {
            $env:Path = "$directory;$env:Path"
        }
        Write-Host "Installed hxn $version to $directory\hxn.exe"
        Write-Host 'Run hxn version to check your installation. New terminals use the updated user PATH.'
    } finally {
        [Net.ServicePointManager]::SecurityProtocol = $previousTls
        if (Test-Path -LiteralPath $temporary) {
            Remove-Item -LiteralPath $temporary -Recurse -Force
        }
    }
}
