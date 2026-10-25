$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'build.ps1')
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$artifacts = Join-Path $PSScriptRoot ('.artifacts/build-tests-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $artifacts | Out-Null
$script:passed = 0
function Check([string] $Name, [scriptblock] $Action) { & $Action; $script:passed++; Write-Output "PASS $Name" }
function Equal($Actual, $Expected) { if ($Actual -cne $Expected) { throw "Expected '$Expected', got '$Actual'" } }
function Reject([scriptblock] $Action, [string] $Pattern) {
    $message = $null
    try { & $Action | Out-Null } catch { $message = $_.Exception.Message }
    if (-not $message -or $message -notmatch $Pattern) { throw "Expected rejection matching '$Pattern', got '$message'" }
}
function Write-Text([string] $Path, [string] $Text) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
    [IO.File]::WriteAllText($Path, $Text)
}
function New-Fixture {
    $root = Join-Path $artifacts ([guid]::NewGuid().ToString('N'))
    $extension = Join-Path $root 'vscode-solx'
    $source = [ordered]@{
        'package.json' = '{"name":"solx-ls","publisher":"jinxware00","version":"1.2.3","description":"original","files":["src/**","README.md","LICENSE","CHANGELOG.md","images/icon.png"]}'
        'src/extension.cjs' = 'exports.activate = () => "original";'
        'src/nested/helper.cjs' = 'exports.helper = true;'
        'README.md' = 'Extension readme'
        'LICENSE' = 'Extension license'
        'CHANGELOG.md' = 'Extension changelog'
        'scripts/test-host.cjs' = 'host fixture'
        'test/host/index.cjs' = 'test fixture'
        'package-lock.json' = '{"lockfileVersion":3}'
    }
    foreach ($entry in $source.GetEnumerator()) { Write-Text (Join-Path $extension $entry.Key) $entry.Value }
    New-Item -ItemType Directory -Path (Join-Path $extension 'images') | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $extension 'images/icon.png'), [byte[]]@(0, 1, 255, 13, 10))
    foreach ($file in @('Cargo.toml', 'Cargo.lock', 'LICENSE', 'docs/INSTALLING.md', 'assets/logo.png', 'src/main.rs', 'installer/windows/build.ps1', 'installer/windows/VSCode.psm1')) {
        Write-Text (Join-Path $root $file) "source $file"
    }
    # Enumerated independently of the production allowlist to catch mapping errors.
    $entries = [ordered]@{
        'extension/package.json' = 'package.json'
        'extension/src/extension.cjs' = 'src/extension.cjs'
        'extension/src/nested/helper.cjs' = 'src/nested/helper.cjs'
        'extension/readme.md' = 'README.md'
        'extension/LICENSE.txt' = 'LICENSE'
        'extension/changelog.md' = 'CHANGELOG.md'
        'extension/images/icon.png' = 'images/icon.png'
    }
    $fixture = [pscustomobject]@{ Root = $root; Extension = $extension; Entries = $entries; Vsix = ''; ReceiptPath = (Join-Path $root 'receipt.json');
        Manifest = '<PackageManifest xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011"><Metadata><Identity Id="solx-ls" Publisher="jinxware00" Version="1.2.3" /></Metadata></PackageManifest>' }
    Write-FixtureVsix $fixture
    return $fixture
}
function Write-FixtureVsix($Fixture, [string] $Duplicate = '') {
    $Fixture.Vsix = Join-Path $Fixture.Root ([guid]::NewGuid().ToString('N') + '.vsix')
    $zip = [IO.Compression.ZipFile]::Open($Fixture.Vsix, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($entry in $Fixture.Entries.GetEnumerator()) {
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $Fixture.Extension $entry.Value), $entry.Key)
        }
        if ($Duplicate) { [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $Fixture.Extension 'src/extension.cjs'), $Duplicate) }
        foreach ($entry in @(@('extension.vsixmanifest', $Fixture.Manifest), @('[Content_Types].xml', '<Types />'))) {
            $writer = New-Object IO.StreamWriter ($zip.CreateEntry($entry[0]).Open())
            try { $writer.Write($entry[1]) } finally { $writer.Dispose() }
        }
    } finally { $zip.Dispose() }
    @{ success = $true; sha256 = (Get-FileHash -LiteralPath $Fixture.Vsix).Hash; checks = 12; snapshotsRemaining = 0; vscode = '1.85.2' } |
        ConvertTo-Json | Set-Content -LiteralPath $Fixture.ReceiptPath -Encoding UTF8
}
function Validate($Fixture) { Test-SolxVsix $Fixture.Extension $Fixture.Vsix $Fixture.ReceiptPath }

Check 'current package validates recursive code, binary assets, docs and manifest' {
    $f = New-Fixture
    $result = Validate $f
    Equal $result.Package.version '1.2.3'
    Equal $result.Files.Count 7
    Equal $result.Sha256 (Get-FileHash -LiteralPath $f.Vsix).Hash
}
Check 'only the current extension name and publisher can be packaged' {
    foreach ($replacement in @(@('solx-ls', 'solx-language-support'), @('jinxware00', 'other-publisher'))) {
        $f = New-Fixture
        $file = Join-Path $f.Extension 'package.json'
        Write-Text $file ([IO.File]::ReadAllText($file).Replace($replacement[0], $replacement[1]))
        Reject { Validate $f } 'Invalid extension package identity'
    }
}
Check 'stale successful receipt cannot authorize modified source at the same version' {
    $f = New-Fixture
    Write-Text (Join-Path $f.Extension 'src/extension.cjs') 'exports.activate = () => "changed";'
    Reject { Validate $f } 'content does not match current source: src/extension.cjs'
}
Check 'all package metadata is compared, beyond publisher, name and version' {
    $f = New-Fixture
    $file = Join-Path $f.Extension 'package.json'
    Write-Text $file ([IO.File]::ReadAllText($file).Replace('original', 'changed'))
    Reject { Validate $f } 'content does not match current source: package.json'
}
Check 'new and removed source files invalidate an old package' {
    $f = New-Fixture
    Write-Text (Join-Path $f.Extension 'src/new.cjs') 'new source'
    Reject { Validate $f } 'Missing VSIX entry: extension/src/new.cjs'
    $f = New-Fixture
    # A nonrecursive move within this test-owned fixture simulates a removed source.
    Move-Item -LiteralPath (Join-Path $f.Extension 'src/nested/helper.cjs') -Destination (Join-Path $f.Root 'helper.previous')
    Reject { Validate $f } 'Unexpected VSIX entry: extension/src/nested/helper.cjs'
}
Check 'documentation and binary changes invalidate a stale receipt' {
    foreach ($name in @('README.md', 'LICENSE', 'images/icon.png')) {
        $f = New-Fixture
        Write-Text (Join-Path $f.Extension $name) 'changed'
        Reject { Validate $f } 'content does not match current source'
    }
}
Check 'unexpected files, missing files and case-insensitive duplicates are rejected' {
    $f = New-Fixture
    $f.Entries['extension/unexpected.cjs'] = 'src/extension.cjs'
    Write-FixtureVsix $f
    Reject { Validate $f } 'Unexpected VSIX entry'
    $f = New-Fixture
    $f.Entries.Remove('extension/src/extension.cjs')
    Write-FixtureVsix $f
    Reject { Validate $f } 'Missing VSIX entry'
    $f = New-Fixture
    Write-FixtureVsix $f 'extension/SRC/extension.cjs'
    Reject { Validate $f } 'Duplicate VSIX entry'
}
Check 'VSIX manifest identity must match source even with an updated receipt' {
    $f = New-Fixture
    $f.Manifest = $f.Manifest.Replace('1.2.3', '0.0.1')
    Write-FixtureVsix $f
    Reject { Validate $f } 'manifest identity does not match'
}
Check 'missing and invalid host receipts are rejected' {
    $f = New-Fixture
    Reject { Test-SolxVsix $f.Extension $f.Vsix (Join-Path $f.Root 'absent.json') } 'run npm run test:package'
    foreach ($override in @(@{ success = $false }, @{ success = 'true' }, @{ sha256 = 'old hash' }, @{ checks = 0 }, @{ snapshotsRemaining = 1 }, @{ vscode = '' })) {
        $receipt = @{ success = $true; sha256 = (Get-FileHash -LiteralPath $f.Vsix).Hash; checks = 12; snapshotsRemaining = 0; vscode = '1.85.2' }
        foreach ($key in $override.Keys) { $receipt[$key] = $override[$key] }
        $receipt | ConvertTo-Json | Set-Content -LiteralPath $f.ReceiptPath -Encoding UTF8
        Reject { Validate $f } 'has not passed packaged host validation'
    }
}
Check 'provenance tracks runtime, extension, validation and installer sources' {
    $f = New-Fixture
    $validated = Validate $f
    $before = Get-SolxBuildSources $f.Root $validated.Files
    foreach ($name in @('src/main.rs', 'vscode-solx/src/nested/helper.cjs', 'vscode-solx/package-lock.json', 'vscode-solx/scripts/test-host.cjs', 'vscode-solx/test/host/index.cjs', 'installer/windows/VSCode.psm1', 'assets/logo.png', 'docs/INSTALLING.md')) {
        Equal $before.Contains($name) $true
    }
    Write-Text (Join-Path $f.Root 'installer/windows/VSCode.psm1') 'updated installer'
    Write-Text (Join-Path $f.Root 'src/new.rs') 'new runtime source'
    Write-Text (Join-Path $f.Root '.cargo/config.toml') '[build]'
    $after = Get-SolxBuildSources $f.Root $validated.Files
    Equal $after.Contains('src/new.rs') $true
    Equal $after.Contains('.cargo/config.toml') $true
    Equal ($before['installer/windows/VSCode.psm1'] -ne $after['installer/windows/VSCode.psm1']) $true
}
Check 'repeated builds preserve all previous archives and provenance bytes' {
    $output = Join-Path $artifacts 'archive history'
    New-Item -ItemType Directory -Path $output | Out-Null
    $archive = Join-Path $output 'solx-1.2.3-windows-x64.zip'
    for ($iteration = 1; $iteration -le 3; $iteration++) {
        $pending = Join-Path $output "pending-$iteration.zip"
        Write-Text $pending "archive bytes $iteration"
        Write-Text ($pending + '.json') "provenance bytes $iteration"
        Save-SolxArchive $pending $archive ($pending + '.json') | Out-Null
        Equal ([IO.File]::ReadAllText($archive)) "archive bytes $iteration"
        Equal ([IO.File]::ReadAllText($archive + '.json')) "provenance bytes $iteration"
    }
    $history = @(Get-ChildItem -LiteralPath (Join-Path $output 'history') -Directory)
    Equal $history.Count 2
    $archives = @($history | ForEach-Object { [IO.File]::ReadAllText((Join-Path $_.FullName 'solx-1.2.3-windows-x64.zip')) })
    $provenance = @($history | ForEach-Object { [IO.File]::ReadAllText((Join-Path $_.FullName 'solx-1.2.3-windows-x64.zip.json')) })
    foreach ($iteration in @(1, 2)) {
        Equal ($archives -contains "archive bytes $iteration") $true
        Equal ($provenance -contains "provenance bytes $iteration") $true
    }
}
Check 'an incomplete pending archive leaves the previous release untouched' {
    $output = Join-Path $artifacts 'incomplete release'
    $archive = Join-Path $output 'release.zip'
    $pending = Join-Path $output 'pending.zip'
    Write-Text $archive 'previous archive'
    Write-Text ($archive + '.json') 'previous provenance'
    Write-Text $pending 'new archive'
    Reject { Save-SolxArchive $pending $archive ($pending + '.json') } 'must both exist'
    Equal ([IO.File]::ReadAllText($archive)) 'previous archive'
    Equal ([IO.File]::ReadAllText($archive + '.json')) 'previous provenance'
}

# Exercise the production orchestration in a test-owned repository. Only the
# compiler/tool version commands are substituted; no real release is rebuilt.
function New-BuildFixture {
    $f = New-Fixture
    Write-Text (Join-Path $f.Root 'Cargo.toml') 'version = "1.2.3"'
    foreach ($file in @('install.ps1', 'uninstall.ps1', 'Installer.psm1')) { Write-Text (Join-Path $f.Root "installer/windows/$file") "fixture $file" }
    $vsix = Join-Path $f.Extension 'solx-ls-1.2.3.vsix'
    Copy-Item -LiteralPath $f.Vsix -Destination $vsix
    $receipt = Join-Path $f.Extension '.test-artifacts/package-validation.json'
    New-Item -ItemType Directory -Path (Split-Path -Parent $receipt) | Out-Null
    Copy-Item -LiteralPath $f.ReceiptPath -Destination $receipt
    return $f
}
function cargo {
    if ($args[0] -eq '--version') { $global:LASTEXITCODE = 0; return 'cargo fixture' }
    $arguments = @($args)
    $target = $arguments[[Array]::IndexOf($arguments, '--target-dir') + 1]
    Write-Text (Join-Path $target 'x86_64-pc-windows-msvc/release/solx.exe') 'compiled fixture'
    if ($script:duringBuild) { & $script:duringBuild }
    $global:LASTEXITCODE = 0
}
function rustc { $global:LASTEXITCODE = 0; return 'rustc fixture' }
function node { $global:LASTEXITCODE = 0; return 'node fixture' }
function npm.cmd { $global:LASTEXITCODE = 0; return 'npm fixture' }
Check 'packaging records complete payload hashes, toolchain and source hashes' {
    $f = New-BuildFixture
    Invoke-SolxBuild $f.Root | Out-Null
    $archive = Join-Path $f.Root 'dist/solx-1.2.3-windows-x64.zip'
    $record = Get-Content -LiteralPath ($archive + '.json') -Raw | ConvertFrom-Json
    Equal $record.schemaVersion 2
    Equal $record.extensionName 'solx-ls'
    Equal $record.extensionPublisher 'jinxware00'
    Equal $record.sha256 (Get-FileHash -LiteralPath $archive).Hash
    Equal $record.vsixSha256 (Get-FileHash -LiteralPath $f.Vsix).Hash
    Equal $record.toolchain.rustc 'rustc fixture'
    Equal $record.packagedValidation.success $true
    Equal $record.sourceFiles.'vscode-solx/src/extension.cjs' (Get-FileHash -LiteralPath (Join-Path $f.Extension 'src/extension.cjs')).Hash
    $unpacked = Join-Path $f.Root 'unpacked'
    Expand-Archive -LiteralPath $archive -DestinationPath $unpacked
    $bundle = Get-Content -LiteralPath (Join-Path $unpacked 'bundle.json') -Raw | ConvertFrom-Json
    Equal $bundle.extensionName $record.extensionName
    Equal $bundle.extensionPublisher $record.extensionPublisher
    $payload = @(Get-ChildItem -LiteralPath $unpacked -Recurse -File)
    Equal $payload.Count @($record.payloadFiles.PSObject.Properties).Count
    foreach ($file in $payload) {
        $relative = $file.FullName.Substring($unpacked.Length + 1).Replace('\', '/')
        Equal $record.payloadFiles.$relative (Get-FileHash -LiteralPath $file.FullName).Hash
    }
}
Check 'source changes during compilation prevent a release from being written' {
    $f = New-BuildFixture
    $script:duringBuild = { Write-Text (Join-Path $f.Root 'src/main.rs') 'changed during build' }
    try { Reject { Invoke-SolxBuild $f.Root } 'Build inputs changed during packaging' }
    finally { $script:duringBuild = $null }
    Equal (Test-Path -LiteralPath (Join-Path $f.Root 'dist/solx-1.2.3-windows-x64.zip')) $false
}
Write-Output "$passed installer build checks passed. Artifacts: $artifacts"
