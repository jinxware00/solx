param([switch] $ValidateOnly)
$ErrorActionPreference = 'Stop'

function Get-SolxSha256 {
    param([IO.Stream] $Stream)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($Stream))).Replace('-', '') }
    finally { $sha.Dispose() }
}

function Get-SolxExtensionFiles {
    param([string] $Directory, $Package)
    $directoryPrefix = [IO.Path]::GetFullPath($Directory).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $files = [ordered]@{ 'extension/package.json' = 'package.json' }
    if (-not $Package.files) { throw 'The extension package must declare its packaged files.' }
    foreach ($pattern in $Package.files) {
        # New glob syntax needs a reviewed implementation of the release allowlist.
        $recursive = $pattern.EndsWith('/**')
        $relative = if ($recursive) { $pattern.Substring(0, $pattern.Length - 3) } else { $pattern }
        if ($relative -match '(^/|[\\:*?\[\]]|(^|/)\.\.?(/|$))') { throw "Unsupported extension file pattern: $pattern" }
        $source = [IO.Path]::GetFullPath((Join-Path $Directory $relative))
        if (-not $source.StartsWith($directoryPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe extension path: $pattern" }
        if ($recursive) {
            if (-not (Test-Path -LiteralPath $source -PathType Container)) { throw "Missing extension directory: $relative" }
            $items = @(Get-ChildItem -LiteralPath $source -Recurse -File | Sort-Object FullName)
        } else {
            if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing extension source: $relative" }
            $items = @(Get-Item -LiteralPath $source)
        }
        foreach ($item in $items) {
            $name = $item.FullName.Substring($directoryPrefix.Length).Replace('\', '/')
            $packagedName = switch -CaseSensitive ($name) {
                'LICENSE' { 'LICENSE.txt' }
                'README.md' { 'readme.md' }
                'CHANGELOG.md' { 'changelog.md' }
                default { $name }
            }
            $files['extension/' + $packagedName] = $name
        }
    }
    return $files
}

function Test-SolxVsix {
    param([string] $Directory, [string] $Vsix, [string] $ReceiptPath)
    $package = Get-Content -LiteralPath (Join-Path $Directory 'package.json') -Raw | ConvertFrom-Json
    if ($package.name -cne 'solx-ls' -or $package.publisher -cne 'jinxware00' -or
        $package.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid extension package identity.' }
    if (-not (Test-Path -LiteralPath $Vsix -PathType Leaf) -or -not (Test-Path -LiteralPath $ReceiptPath -PathType Leaf)) {
        throw 'Build the VSIX and run npm run test:package before building the installer.'
    }
    $hash = (Get-FileHash -LiteralPath $Vsix -Algorithm SHA256).Hash
    $receipt = Get-Content -LiteralPath $ReceiptPath -Raw | ConvertFrom-Json
    if ($receipt.success -isnot [bool] -or -not $receipt.success -or $receipt.sha256 -ne $hash -or
        $receipt.checks -lt 1 -or $receipt.snapshotsRemaining -ne 0 -or $receipt.vscode -notmatch '^\d+\.\d+\.\d+') {
        throw 'The current VSIX has not passed packaged host validation.'
    }
    $files = Get-SolxExtensionFiles $Directory $package
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Vsix)
    try {
        $seen = @{}
        foreach ($entry in $zip.Entries) {
            if ($seen.ContainsKey($entry.FullName)) { throw "Duplicate VSIX entry: $($entry.FullName)" }
            $seen[$entry.FullName] = $true
            if ($entry.FullName -ceq 'extension.vsixmanifest' -or $entry.FullName -ceq '[Content_Types].xml') { continue }
            if (-not $files.Contains($entry.FullName)) { throw "Unexpected VSIX entry: $($entry.FullName)" }
            $stream = $entry.Open()
            try { $entryHash = Get-SolxSha256 $stream } finally { $stream.Dispose() }
            $sourceHash = (Get-FileHash -LiteralPath (Join-Path $Directory $files[$entry.FullName]) -Algorithm SHA256).Hash
            if ($entryHash -ne $sourceHash) { throw "VSIX content does not match current source: $($files[$entry.FullName]). Repackage and rerun npm run test:package." }
        }
        foreach ($name in @($files.Keys) + @('extension.vsixmanifest', '[Content_Types].xml')) {
            if (-not $seen.ContainsKey($name)) { throw "Missing VSIX entry: $name" }
        }
        $reader = New-Object IO.StreamReader ($zip.GetEntry('extension.vsixmanifest').Open())
        try { $manifest = [xml]$reader.ReadToEnd() } finally { $reader.Dispose() }
        $identity = $manifest.SelectSingleNode('//*[local-name()="Identity"]')
        if (-not $identity -or $identity.Id -cne $package.name -or $identity.Publisher -cne $package.publisher -or $identity.Version -cne $package.version) {
            throw 'VSIX manifest identity does not match source.'
        }
    } finally { $zip.Dispose() }
    return [pscustomobject]@{ Package = $package; Files = $files; Sha256 = $hash; Receipt = $receipt }
}

function Get-SolxBuildSources {
    param([string] $Root, $ExtensionFiles)
    $paths = @('Cargo.toml', 'Cargo.lock', 'LICENSE', 'docs/INSTALLING.md', 'assets/logo.png', 'vscode-solx/package-lock.json')
    foreach ($optional in @('build.rs', 'rust-toolchain', 'rust-toolchain.toml', '.cargo/config', '.cargo/config.toml')) {
        if (Test-Path -LiteralPath (Join-Path $Root $optional) -PathType Leaf) { $paths += $optional }
    }
    $prefix = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    foreach ($directory in @('src', 'vscode-solx/scripts', 'vscode-solx/test')) {
        $paths += @(Get-ChildItem -LiteralPath (Join-Path $Root $directory) -Recurse -File | ForEach-Object { $_.FullName.Substring($prefix.Length).Replace('\', '/') })
    }
    $paths += @(Get-ChildItem -LiteralPath (Join-Path $Root 'installer/windows') -File | Where-Object { $_.Extension -in @('.ps1', '.psm1') } | ForEach-Object { $_.FullName.Substring($prefix.Length).Replace('\', '/') })
    $paths += @($ExtensionFiles.Values | ForEach-Object { 'vscode-solx/' + $_ })
    $hashes = [ordered]@{}
    foreach ($file in $paths | Sort-Object -Unique) { $hashes[$file] = (Get-FileHash -LiteralPath (Join-Path $Root $file) -Algorithm SHA256).Hash }
    return $hashes
}

function Save-SolxArchive {
    param([string] $Pending, [string] $Archive, [string] $PendingProvenance)
    if (-not (Test-Path -LiteralPath $Pending -PathType Leaf) -or -not (Test-Path -LiteralPath $PendingProvenance -PathType Leaf)) {
        throw 'The pending archive and provenance must both exist before replacement.'
    }
    # Preserve prior bytes and provenance before replacing the conventional filename.
    $history = $null
    if ((Test-Path -LiteralPath $Archive) -or (Test-Path -LiteralPath ($Archive + '.json'))) {
        $history = Join-Path (Split-Path -Parent $Archive) ('history/' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $history | Out-Null
        foreach ($previous in @($Archive, ($Archive + '.json'))) {
            if (Test-Path -LiteralPath $previous) {
                $copy = Join-Path $history (Split-Path -Leaf $previous)
                Copy-Item -LiteralPath $previous -Destination $copy
                if ((Get-FileHash -LiteralPath $copy).Hash -ne (Get-FileHash -LiteralPath $previous).Hash) { throw 'Previous archive backup failed verification.' }
            }
        }
    }
    Move-Item -LiteralPath $Pending -Destination $Archive -Force
    Move-Item -LiteralPath $PendingProvenance -Destination ($Archive + '.json') -Force
    if ($history) { Write-Output "Previous release preserved: $history" }
}

function Invoke-SolxBuild {
    param([string] $Root, [switch] $ValidateOnly)
    $installer = Join-Path $Root 'installer/windows'
    $extension = Join-Path $Root 'vscode-solx'
    $package = Get-Content -LiteralPath (Join-Path $extension 'package.json') -Raw | ConvertFrom-Json
    $version = [regex]::Match((Get-Content -LiteralPath (Join-Path $Root 'Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Cannot read Solx version.' }
    $vsixName = "$($package.name)-$($package.version).vsix"
    $vsix = Join-Path $extension $vsixName
    $receiptPath = Join-Path $extension '.test-artifacts/package-validation.json'
    $validated = Test-SolxVsix $extension $vsix $receiptPath
    $sources = Get-SolxBuildSources $Root $validated.Files
    if ($ValidateOnly) { Write-Output "Validated current VSIX contents and host receipt: $vsixName"; return }

    $output = Join-Path $Root 'dist'
    $payload = Join-Path $installer ('.artifacts/bundle-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $payload,$output,(Join-Path $payload 'bin'),(Join-Path $payload 'editor') | Out-Null
    & cargo build --release --locked --manifest-path (Join-Path $Root 'Cargo.toml') --bin solx --target x86_64-pc-windows-msvc --target-dir (Join-Path $installer '.artifacts/cargo')
    if ($LASTEXITCODE -ne 0) { throw 'Cargo release build failed.' }
    Copy-Item -LiteralPath (Join-Path $installer '.artifacts/cargo/x86_64-pc-windows-msvc/release/solx.exe') -Destination (Join-Path $payload 'bin/solx.exe')
    Copy-Item -LiteralPath $vsix -Destination (Join-Path $payload "editor/$vsixName")
    foreach ($file in @('install.ps1', 'uninstall.ps1', 'Installer.psm1', 'VSCode.psm1')) { Copy-Item -LiteralPath (Join-Path $installer $file) -Destination $payload }
    Copy-Item -LiteralPath (Join-Path $Root 'LICENSE') -Destination $payload
    Copy-Item -LiteralPath (Join-Path $Root 'docs/INSTALLING.md') -Destination $payload
    Copy-Item -LiteralPath (Join-Path $Root 'assets/logo.png') -Destination $payload
    $files = [ordered]@{}
    foreach ($file in @('bin/solx.exe', "editor/$vsixName", 'LICENSE', 'INSTALLING.md', 'uninstall.ps1', 'Installer.psm1')) { $files[$file] = (Get-FileHash -LiteralPath (Join-Path $payload $file)).Hash }
    [ordered]@{ name = 'solx'; version = $version; extensionName = $package.name; extensionPublisher = $package.publisher;
        extensionVersion = $package.version; files = $files } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $payload 'bundle.json') -Encoding UTF8

    # Do not label an artifact with a snapshot that changed while Cargo or copying ran.
    $current = Test-SolxVsix $extension $vsix $receiptPath
    $currentSources = Get-SolxBuildSources $Root $current.Files
    if (($sources | ConvertTo-Json -Compress) -cne ($currentSources | ConvertTo-Json -Compress) -or
        $validated.Sha256 -ne $current.Sha256 -or $validated.Sha256 -ne $files["editor/$vsixName"]) { throw 'Build inputs changed during packaging. Run the build again.' }
    $payloadHashes = [ordered]@{}
    foreach ($file in Get-ChildItem -LiteralPath $payload -Recurse -File | Sort-Object FullName) {
        $name = $file.FullName.Substring($payload.Length + 1).Replace('\', '/')
        $payloadHashes[$name] = (Get-FileHash -LiteralPath $file.FullName).Hash
    }
    $archive = Join-Path $output "solx-$version-windows-x64.zip"
    $pending = Join-Path $output ('bundle-' + [guid]::NewGuid().ToString('N') + '.zip')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($payload, $pending)
    $head = $null; $runtimeDirty = $null; $extensionDirty = $null; $installerDirty = $null
    if (Test-Path -LiteralPath (Join-Path $Root '.git')) {
        $head = & git -C $Root rev-parse HEAD
        if ($LASTEXITCODE -ne 0) { throw 'Cannot determine source commit.' }
        $runtimeDirty = @(& git -C $Root status --porcelain --untracked-files=all -- src Cargo.toml Cargo.lock build.rs .cargo rust-toolchain rust-toolchain.toml).Count -gt 0
        if ($LASTEXITCODE -ne 0) { throw 'Cannot determine runtime source status.' }
        $extensionDirty = @(& git -C $Root status --porcelain --untracked-files=all -- vscode-solx).Count -gt 0
        if ($LASTEXITCODE -ne 0) { throw 'Cannot determine extension source status.' }
        $installerDirty = @(& git -C $Root status --porcelain --untracked-files=all -- installer/windows docs/INSTALLING.md LICENSE assets/logo.png).Count -gt 0
        if ($LASTEXITCODE -ne 0) { throw 'Cannot determine installer source status.' }
    }
    $toolchain = [ordered]@{}
    foreach ($tool in @('rustc', 'cargo', 'node', 'npm.cmd')) {
        $toolchain[$tool] = (& $tool --version | Out-String).Trim()
        if ($LASTEXITCODE -ne 0) { throw "Cannot read $tool version." }
    }
    [ordered]@{ schemaVersion = 2; interpreterVersion = $version; extensionName = $package.name; extensionPublisher = $package.publisher;
        extensionVersion = $package.version; sourceCommit = $head;
        runtimeHasLocalChanges = $runtimeDirty; extensionHasLocalChanges = $extensionDirty; installerHasLocalChanges = $installerDirty;
        target = 'x86_64-pc-windows-msvc'; builtAtUtc = [DateTime]::UtcNow.ToString('o'); toolchain = $toolchain;
        buildEnvironment = @{ RUSTFLAGS = $env:RUSTFLAGS; CARGO_ENCODED_RUSTFLAGS = $env:CARGO_ENCODED_RUSTFLAGS; SOURCE_DATE_EPOCH = $env:SOURCE_DATE_EPOCH };
        sourceFiles = $sources; payloadFiles = $payloadHashes; packagedValidation = $validated.Receipt;
        sha256 = (Get-FileHash -LiteralPath $pending).Hash; vsixSha256 = $validated.Sha256
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath ($pending + '.json') -Encoding UTF8
    Save-SolxArchive $pending $archive ($pending + '.json')
    Write-Output "Terminal installer: $archive"
    Write-Output 'Extract the ZIP, open a terminal in that folder, then run: powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1'
}

# Dot-sourcing loads the same validation functions for isolated fixture tests.
if ($MyInvocation.InvocationName -ne '.') {
    Invoke-SolxBuild ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))) -ValidateOnly:$ValidateOnly
}
