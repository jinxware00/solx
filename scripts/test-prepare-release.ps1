# Isolated release-boundary fixtures: no real compiler, install, or network.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
. (Join-Path $project 'installer/windows/build.ps1')
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$fixture = Join-Path $project ('target/validation/release-fixture-' + [guid]::NewGuid().ToString('N'))
$script:passed = 0
function Check([string] $Name, [scriptblock] $Action) { & $Action; $script:passed++; Write-Output "PASS $Name" }
function Require([bool] $Condition, [string] $Message) { if (-not $Condition) { throw $Message } }
function Reject([scriptblock] $Action, [string] $Pattern) {
    $message = $null
    try { & $Action | Out-Null } catch { $message = $_.Exception.Message }
    Require ($message -and $message -match $Pattern) "Expected rejection matching '$Pattern', got '$message'"
}
function Write-Text([string] $Path, [string] $Text) {
    [IO.Directory]::CreateDirectory((Split-Path -Parent $Path)) | Out-Null
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding $false))
}
function Write-Json([string] $Path, $Object) { Write-Text $Path ($Object | ConvertTo-Json -Depth 10) }
function Fixture-Git([string[]] $Arguments) {
    & git -C $fixture -c core.autocrlf=false -c core.hooksPath=.git/no-hooks -c user.name=ReleaseFixture -c user.email=release-fixture@example.invalid @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Fixture Git operation failed.' }
}
$source = [ordered]@{
    'Cargo.toml' = "[package]`nname = `"solx`"`nversion = `"0.2.0`"`n"
    'Cargo.lock' = "version = 4`n[[package]]`nname = `"solx`"`nversion = `"0.2.0`"`n"
    '.gitignore' = "dist/`ninstaller/windows/.artifacts/`nvscode-solx/.test-artifacts/`n*.vsix`n"
    'src/main.rs' = 'fn main() {}'
    'LICENSE' = 'Fixture license'
    'docs/INSTALLING.md' = 'Fixture installation instructions'
    'assets/logo.png' = 'Fixture image bytes'
    'installer/windows/install.ps1' = '# Fixture install'
    'installer/windows/uninstall.ps1' = '# Fixture uninstall'
    'installer/windows/Installer.psm1' = '# Fixture installer module'
    'installer/windows/VSCode.psm1' = '# Fixture editor module'
    '.github/workflows/ci.yml' = 'name: Fixture CI'
    '.github/workflows/release.yml' = 'name: Fixture release'
    'vscode-solx/package.json' = '{"name":"solx-ls","publisher":"jinxware00","version":"0.1.0","files":["src/**","README.md","LICENSE","CHANGELOG.md","images/icon.png"]}'
    'vscode-solx/package-lock.json' = '{"name":"solx-ls","version":"0.1.0","lockfileVersion":3,"packages":{"":{"name":"solx-ls","version":"0.1.0"}}}'
    'vscode-solx/src/extension.cjs' = 'exports.activate = () => {};'
    'vscode-solx/README.md' = 'Fixture extension readme'
    'vscode-solx/LICENSE' = 'Fixture extension license'
    'vscode-solx/CHANGELOG.md' = 'Fixture extension changelog'
    'vscode-solx/images/icon.png' = 'Fixture icon bytes'
    'vscode-solx/scripts/test-host.cjs' = '// Fixture host'
    'vscode-solx/test/host/index.cjs' = '// Fixture test'
}
foreach ($entry in $source.GetEnumerator()) { Write-Text (Join-Path $fixture $entry.Key) $entry.Value }
foreach ($relative in @('installer/windows/build.ps1', 'scripts/prepare-release.ps1', 'scripts/test-prepare-release.ps1')) {
    $target = Join-Path $fixture $relative
    [IO.Directory]::CreateDirectory((Split-Path -Parent $target)) | Out-Null
    Copy-Item -LiteralPath (Join-Path $project $relative) -Destination $target
}
$vsix = Join-Path $fixture 'vscode-solx/solx-ls-0.1.0.vsix'
$zipEntries = [ordered]@{
    'extension/package.json' = 'package.json'; 'extension/src/extension.cjs' = 'src/extension.cjs'
    'extension/readme.md' = 'README.md'; 'extension/LICENSE.txt' = 'LICENSE'
    'extension/changelog.md' = 'CHANGELOG.md'; 'extension/images/icon.png' = 'images/icon.png'
}
$zip = [IO.Compression.ZipFile]::Open($vsix, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($entry in $zipEntries.GetEnumerator()) {
        [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $fixture ('vscode-solx/' + $entry.Value)), $entry.Key)
    }
    foreach ($entry in @(
        @('extension.vsixmanifest', '<PackageManifest xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011"><Metadata><Identity Id="solx-ls" Publisher="jinxware00" Version="0.1.0" /></Metadata></PackageManifest>'),
        @('[Content_Types].xml', '<Types />')
    )) {
        $writer = New-Object IO.StreamWriter ($zip.CreateEntry($entry[0]).Open())
        try { $writer.Write($entry[1]) } finally { $writer.Dispose() }
    }
} finally { $zip.Dispose() }
Write-Json (Join-Path $fixture 'vscode-solx/.test-artifacts/package-validation.json') @{
    success = $true; sha256 = (Get-FileHash -LiteralPath $vsix).Hash; checks = 18; snapshotsRemaining = 0; vscode = '1.85.2'
}
Fixture-Git @('init', '--quiet')
Fixture-Git @('config', 'core.autocrlf', 'false')
Fixture-Git @('add', '--', '.')
Fixture-Git @('commit', '--quiet', '-m', 'Release fixture source')
Fixture-Git @('tag', 'v0.2.0')
Fixture-Git @('tag', 'v9.9.9')
$commit = & git -C $fixture rev-parse HEAD
if ($LASTEXITCODE -ne 0) { throw 'Cannot read fixture commit.' }

# Invoke the real bundle orchestration with only compiler/version commands
# substituted. The resulting ZIP is never executed or installed.
function cargo {
    if ($args[0] -eq '--version') { $global:LASTEXITCODE = 0; return 'cargo 1.99.0 (5f94df478 2026-08-27)' }
    $arguments = @($args)
    $target = $arguments[[Array]::IndexOf($arguments, '--target-dir') + 1]
    Write-Text (Join-Path $target 'x86_64-pc-windows-msvc/release/solx.exe') 'Fixture executable bytes'
    $global:LASTEXITCODE = 0
}
function rustc { $global:LASTEXITCODE = 0; return 'rustc 1.99.0 (b940084d7 2026-09-28)' }
function node { $global:LASTEXITCODE = 0; return 'v24.19.0' }
function npm.cmd { $global:LASTEXITCODE = 0; return '11.6.2' }
Invoke-SolxBuild $fixture | Out-Null
$archive = Join-Path $fixture 'dist/solx-0.2.0-windows-x64.zip'
$rawPath = "$archive.json"
$record = Get-Content -LiteralPath $rawPath -Raw | ConvertFrom-Json
$record | Add-Member -NotePropertyName privateLocalPath -NotePropertyValue 'C:\Users\PRIVATE_MARKER\work'
$record.buildEnvironment.RUSTFLAGS = '--remap-path-prefix=C:\Users\PRIVATE_MARKER=src'
$record.packagedValidation | Add-Member -NotePropertyName privateLog -NotePropertyValue 'PRIVATE_MARKER'
Write-Json $rawPath $record
$rawBytes = [IO.File]::ReadAllBytes($rawPath)
$archiveBytes = [IO.File]::ReadAllBytes($archive)
$helper = Join-Path $fixture 'scripts/prepare-release.ps1'
$prepared = Join-Path $fixture 'dist/release-assets'
function Prepare { & $helper -Tag v0.2.0 -ExpectedCommit $commit -AssetDirectory (Join-Path $fixture ('dist/candidate-' + [guid]::NewGuid().ToString('N'))) }
function Verify { & $helper -Tag v0.2.0 -ExpectedCommit $commit -AssetDirectory $prepared -VerifyOnly }

Check 'matching tagged source stages exactly four public assets and verifies them' {
    & $helper -Tag v0.2.0 -ExpectedCommit $commit -AssetDirectory $prepared | Out-Null
    Require (@(Get-ChildItem -LiteralPath $prepared).Count -eq 4) 'Expected four public assets.'
    Verify | Out-Null
}
Check 'sanitization omits private fields and flags without changing original provenance' {
    $json = [IO.File]::ReadAllText((Join-Path $prepared 'solx-0.2.0-windows-x64.zip.json'))
    Require (-not $json.Contains('PRIVATE_MARKER')) 'Private fixture text leaked into public metadata.'
    Require (-not $json.Contains('privateLocalPath') -and -not $json.Contains('privateLog')) 'Unreviewed fields leaked into public metadata.'
    Require (([Convert]::ToBase64String([IO.File]::ReadAllBytes($rawPath))) -ceq ([Convert]::ToBase64String($rawBytes))) 'Original provenance changed.'
}
Check 'existing public assets cannot be overwritten' {
    Reject { & $helper -Tag v0.2.0 -ExpectedCommit $commit -AssetDirectory $prepared } 'already exists'
}
Check 'tag and expected commit must match the runtime and checkout' {
    Reject { & $helper -Tag v9.9.9 -ExpectedCommit $commit -VerifyOnly -AssetDirectory $prepared } 'Tag must match'
    Reject { & $helper -Tag v0.2.0 -ExpectedCommit ('a' * 40) -VerifyOnly -AssetDirectory $prepared } 'Checkout HEAD'
}
Check 'the empty-key lockfile root must match the extension version' {
    $file = Join-Path $fixture 'vscode-solx/package-lock.json'
    $before = [IO.File]::ReadAllBytes($file)
    try {
        Write-Text $file '{"name":"solx-ls","version":"0.1.0","lockfileVersion":3,"packages":{"":{"name":"solx-ls","version":"0.9.9"}}}'
        Reject { Prepare } 'Extension release identity or lockfile differs'
    } finally { [IO.File]::WriteAllBytes($file, $before) }
}
Check 'dirty packaged source is rejected before staging' {
    $file = Join-Path $fixture 'src/main.rs'
    $before = [IO.File]::ReadAllBytes($file)
    try { Write-Text $file 'fn changed() {}'; Reject { Prepare } 'Commit all packaged' }
    finally { [IO.File]::WriteAllBytes($file, $before) }
}
Check 'untracked packaged source is rejected' {
    $file = Join-Path $fixture 'src/untracked.rs'
    try { Write-Text $file '// new input'; Reject { Prepare } 'not tracked' }
    finally { Remove-Item -LiteralPath $file }
}
Check 'stale commit and dirty flags in provenance are rejected' {
    foreach ($change in @(@{ sourceCommit = ('b' * 40) }, @{ runtimeHasLocalChanges = $true })) {
        try {
            $changed = [Text.Encoding]::UTF8.GetString($rawBytes) | ConvertFrom-Json
            foreach ($name in $change.Keys) { $changed.$name = $change[$name] }
            Write-Json $rawPath $changed
            Reject { Prepare } 'tagged source commit|dirty or unknown'
        } finally { [IO.File]::WriteAllBytes($rawPath, $rawBytes) }
    }
}
Check 'changed source hash in otherwise matching provenance is rejected' {
    try {
        $changed = [Text.Encoding]::UTF8.GetString($rawBytes) | ConvertFrom-Json
        $changed.sourceFiles.'src/main.rs' = 'a' * 64
        Write-Json $rawPath $changed
        Reject { Prepare } 'Source provenance differs'
    } finally { [IO.File]::WriteAllBytes($rawPath, $rawBytes) }
}
Check 'unexpected ZIP content is rejected even with an updated archive hash' {
    try {
        $zip = [IO.Compression.ZipFile]::Open($archive, [IO.Compression.ZipArchiveMode]::Update)
        try { [void]$zip.CreateEntry('unexpected.txt') } finally { $zip.Dispose() }
        $changed = [Text.Encoding]::UTF8.GetString($rawBytes) | ConvertFrom-Json
        $changed.sha256 = (Get-FileHash -LiteralPath $archive).Hash
        Write-Json $rawPath $changed
        Reject { Prepare } 'Unexpected or duplicate installer ZIP entry'
    } finally { [IO.File]::WriteAllBytes($archive, $archiveBytes); [IO.File]::WriteAllBytes($rawPath, $rawBytes) }
}
Check 'downloaded archive tampering is rejected' {
    $file = Join-Path $prepared 'solx-0.2.0-windows-x64.zip'
    try { Write-Text $file 'damaged'; Reject { Verify } 'Installer archive SHA-256' }
    finally { [IO.File]::WriteAllBytes($file, $archiveBytes) }
}
Check 'downloaded checksum tampering is rejected' {
    $file = Join-Path $prepared 'SHA256SUMS'
    $before = [IO.File]::ReadAllBytes($file)
    try { Write-Text $file 'wrong checksum'; Reject { Verify } 'SHA256SUMS differs' }
    finally { [IO.File]::WriteAllBytes($file, $before) }
}
Check 'unreviewed public metadata and extra artifact files are rejected' {
    $file = Join-Path $prepared 'solx-0.2.0-windows-x64.zip.json'
    $before = [IO.File]::ReadAllBytes($file)
    try {
        $changed = [IO.File]::ReadAllText($file) | ConvertFrom-Json
        $changed | Add-Member -NotePropertyName privatePath -NotePropertyValue 'PRIVATE_MARKER'
        Write-Json $file $changed
        Reject { Verify } 'Unexpected public provenance fields'
    } finally { [IO.File]::WriteAllBytes($file, $before) }
    $extra = Join-Path $prepared 'local-log.txt'
    try { Write-Text $extra 'PRIVATE_MARKER'; Reject { Verify } 'exactly four files' }
    finally { Remove-Item -LiteralPath $extra }
}
Check 'verification needs only source and public assets, not local build receipts' {
    $receipt = Join-Path $fixture 'vscode-solx/.test-artifacts/package-validation.json'
    $receiptBytes = [IO.File]::ReadAllBytes($receipt)
    try { Write-Text $rawPath 'not local provenance'; Write-Text $receipt 'not a local receipt'; Verify | Out-Null }
    finally { [IO.File]::WriteAllBytes($rawPath, $rawBytes); [IO.File]::WriteAllBytes($receipt, $receiptBytes) }
}
Write-Output "$passed release preparation checks passed. Fixture: $fixture"
