# Stage or verify the four public assets. No build, install, network, or publication.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidatePattern('^v[0-9]+\.[0-9]+\.[0-9]+$')][string] $Tag,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-f]{40}$')][string] $ExpectedCommit,
    [string] $AssetDirectory,
    [switch] $VerifyOnly
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $AssetDirectory) { $AssetDirectory = Join-Path $root 'dist/release-assets' }
$AssetDirectory = [IO.Path]::GetFullPath($AssetDirectory)
. (Join-Path $root 'installer/windows/build.ps1')

function Assert-Release {
    param([bool] $Condition, [string] $Message)
    if (-not $Condition) { throw $Message }
}

function Get-ReleaseHash {
    param([string] $Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-ReleaseKeys {
    param($InputObject, [string[]] $Expected)
    $names = @($InputObject.PSObject.Properties.Name)
    Assert-Release ($names.Count -eq $Expected.Count) 'Unexpected public provenance fields.'
    foreach ($name in $names) { Assert-Release ($Expected -ccontains $name) 'Unexpected public provenance field.' }
}

function Get-ReleaseMap {
    param($InputObject)
    Assert-Release ($null -ne $InputObject) 'Missing provenance hash map.'
    $result = [ordered]@{}
    foreach ($property in $InputObject.PSObject.Properties) {
        $name = $property.Name
        Assert-Release ($name -cmatch '^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$' -and
            $name -cnotmatch '(^|/)\.\.?(/|$)') 'Provenance contains an unsafe relative path.'
        Assert-Release ($property.Value -is [string] -and $property.Value -cmatch '^[A-Fa-f0-9]{64}$') 'Invalid provenance SHA-256.'
        $result[$name] = $property.Value.ToLowerInvariant()
    }
    return $result
}

function Assert-ReleaseMapsEqual {
    param($Actual, $Expected, [string] $Description)
    Assert-Release ($Actual.Count -eq $Expected.Count) "$Description file count differs."
    foreach ($name in $Expected.Keys) {
        Assert-Release ($Actual.Contains($name) -and $Actual[$name] -ieq $Expected[$name]) "$Description differs: $name"
    }
}

function Get-ReleaseToolchain {
    param($Tools)
    # Keep only recognizable public version identifiers. Do not copy arbitrary
    # tool output, environment flags, local paths, or logs into release metadata.
    $result = [ordered]@{}
    foreach ($name in @('rustc', 'cargo')) {
        $value = $Tools.$name
        Assert-Release ($value -is [string] -and $value -cmatch ('^' + $name + ' ([0-9]+\.[0-9]+\.[0-9]+) \(([0-9a-f]+) ([0-9]{4}-[0-9]{2}-[0-9]{2})\)$')) 'Unrecognized Rust toolchain version record.'
        $result[$name] = [ordered]@{ version = $Matches[1]; commit = $Matches[2]; date = $Matches[3] }
    }
    Assert-Release ($Tools.node -is [string] -and $Tools.node -cmatch '^v([0-9]+\.[0-9]+\.[0-9]+)$') 'Unrecognized Node version record.'
    $result.node = $Matches[1]
    Assert-Release ($Tools.'npm.cmd' -is [string] -and $Tools.'npm.cmd' -cmatch '^([0-9]+\.[0-9]+\.[0-9]+)$') 'Unrecognized npm version record.'
    $result.npm = $Matches[1]
    return $result
}

function Assert-ReleaseRecord {
    param($Record, $Sources, [string] $RuntimeVersion, $Package)
    Assert-Release ($Record.sourceCommit -ceq $ExpectedCommit) 'Build provenance does not identify the tagged source commit.'
    foreach ($name in @('runtimeHasLocalChanges', 'extensionHasLocalChanges', 'installerHasLocalChanges')) {
        Assert-Release ($Record.$name -is [bool] -and -not $Record.$name) 'Build provenance contains dirty or unknown packaged inputs.'
    }
    Assert-Release ($Record.interpreterVersion -ceq $RuntimeVersion -and $Record.extensionVersion -ceq $Package.version -and
        $Record.extensionName -ceq $Package.name -and $Record.extensionPublisher -ceq $Package.publisher -and
        $Record.target -ceq 'x86_64-pc-windows-msvc') 'Release versions or target do not match source.'
    Assert-Release ($Record.sha256 -is [string] -and $Record.sha256 -cmatch '^[a-fA-F0-9]{64}$' -and
        $Record.vsixSha256 -is [string] -and $Record.vsixSha256 -cmatch '^[a-fA-F0-9]{64}$') 'Invalid artifact SHA-256.'
    $validation = $Record.packagedValidation
    Assert-Release ($validation.success -is [bool] -and $validation.success -and $validation.sha256 -ieq $Record.vsixSha256 -and
        ($validation.checks -is [int] -or $validation.checks -is [long]) -and
        $validation.checks -ge 18 -and $validation.snapshotsRemaining -eq 0 -and
        $validation.vscode -is [string] -and $validation.vscode -cmatch '^[0-9]+\.[0-9]+\.[0-9]+$') 'Missing successful packaged desktop validation.'
    $recordedSources = Get-ReleaseMap $Record.sourceFiles
    Assert-ReleaseMapsEqual $recordedSources $Sources 'Source provenance'
}

function Assert-ReleaseArchive {
    param([string] $Archive, [string] $Vsix, $Record, [string] $RuntimeVersion, $Package)
    Assert-Release ((Get-ReleaseHash $Archive) -ieq $Record.sha256) 'Installer archive SHA-256 differs from provenance.'
    Assert-Release ((Get-ReleaseHash $Vsix) -ieq $Record.vsixSha256) 'VSIX SHA-256 differs from provenance.'
    $payload = Get-ReleaseMap $Record.payloadFiles
    $editorPath = "editor/$($Package.name)-$($Package.version).vsix"
    $expectedNames = @('bin/solx.exe', $editorPath, 'install.ps1', 'uninstall.ps1', 'Installer.psm1', 'VSCode.psm1', 'LICENSE', 'INSTALLING.md', 'logo.png', 'bundle.json')
    Assert-Release ($payload.Count -eq $expectedNames.Count) 'Unexpected installer payload file count.'
    foreach ($name in $expectedNames) { Assert-Release ($payload.Contains($name)) "Missing installer payload: $name" }
    Assert-Release ($payload[$editorPath] -ieq $Record.vsixSha256) 'Bundled extension differs from standalone VSIX.'
    $copiedSources = [ordered]@{
        'install.ps1' = 'installer/windows/install.ps1'; 'uninstall.ps1' = 'installer/windows/uninstall.ps1'
        'Installer.psm1' = 'installer/windows/Installer.psm1'; 'VSCode.psm1' = 'installer/windows/VSCode.psm1'
        'LICENSE' = 'LICENSE'; 'INSTALLING.md' = 'docs/INSTALLING.md'; 'logo.png' = 'assets/logo.png'
    }
    foreach ($name in $copiedSources.Keys) {
        Assert-Release ($payload[$name] -ieq $Record.sourceFiles.($copiedSources[$name])) "Bundled source differs from provenance: $name"
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $seen = New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::Ordinal)
        foreach ($entry in $zip.Entries) {
            Assert-Release ($seen.Add($entry.FullName) -and $expectedNames -ccontains $entry.FullName) 'Unexpected or duplicate installer ZIP entry.'
            Assert-Release ($entry.Length -le 134217728) 'Installer ZIP entry exceeds the release size bound.'
            $stream = $entry.Open()
            try { $actual = Get-SolxSha256 $stream } finally { $stream.Dispose() }
            Assert-Release ($actual -ieq $payload[$entry.FullName]) "Installer ZIP content hash differs: $($entry.FullName)"
        }
        Assert-Release ($seen.Count -eq $expectedNames.Count) 'Installer ZIP is missing a payload file.'
        Assert-Release ($zip.GetEntry('bundle.json').Length -le 1048576) 'Internal bundle manifest exceeds the release size bound.'
        $reader = New-Object IO.StreamReader ($zip.GetEntry('bundle.json').Open())
        try { $bundle = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
        Assert-Release ($bundle.name -ceq 'solx' -and $bundle.version -ceq $RuntimeVersion -and $bundle.extensionVersion -ceq $Package.version -and
            $bundle.extensionName -ceq $Package.name -and $bundle.extensionPublisher -ceq $Package.publisher) 'Internal bundle versions differ from source.'
        $installed = Get-ReleaseMap $bundle.files
        $installedNames = @('bin/solx.exe', $editorPath, 'LICENSE', 'INSTALLING.md', 'uninstall.ps1', 'Installer.psm1')
        Assert-Release ($installed.Count -eq $installedNames.Count) 'Unexpected installed-file manifest count.'
        foreach ($name in $installedNames) {
            Assert-Release ($installed.Contains($name) -and $installed[$name] -ieq $payload[$name]) "Internal installed-file checksum differs: $name"
        }
    } finally { $zip.Dispose() }
}

function Get-ReleaseChecksums {
    param([string] $Directory, [string[]] $Names)
    return (($Names | ForEach-Object { (Get-ReleaseHash (Join-Path $Directory $_)) + '  ' + $_ }) -join "`n") + "`n"
}

$head = & git -C $root rev-parse --verify HEAD
Assert-Release ($LASTEXITCODE -eq 0 -and $head -ceq $ExpectedCommit) 'Checkout HEAD does not equal the expected release commit.'
$tagCommit = & git -C $root rev-parse --verify ('refs/tags/' + $Tag + '^{commit}')
Assert-Release ($LASTEXITCODE -eq 0 -and $tagCommit -ceq $ExpectedCommit) 'Release tag does not resolve to the expected commit.'
$package = Get-Content -LiteralPath (Join-Path $root 'vscode-solx/package.json') -Raw | ConvertFrom-Json
$lockText = Get-Content -LiteralPath (Join-Path $root 'vscode-solx/package-lock.json') -Raw
# npm lockfiles use an empty key for the root package. PowerShell 5 cannot
# represent that key in a PSCustomObject, so deserialize to a dictionary.
if ($PSVersionTable.PSVersion.Major -ge 6) {
    $lock = ConvertFrom-Json -InputObject $lockText -AsHashtable
} else {
    Add-Type -AssemblyName System.Web.Extensions
    $lock = (New-Object System.Web.Script.Serialization.JavaScriptSerializer).DeserializeObject($lockText)
}
$version = [regex]::Match((Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
Assert-Release ($version -cmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -and $Tag -ceq "v$version") 'Tag must match the Cargo runtime version.'
$cargoLock = [regex]::Matches((Get-Content -LiteralPath (Join-Path $root 'Cargo.lock') -Raw), '(?m)^name\s*=\s*"solx"\r?\nversion\s*=\s*"([^"]+)"')
Assert-Release ($cargoLock.Count -eq 1 -and $cargoLock[0].Groups[1].Value -ceq $version) 'Cargo lockfile runtime version differs from source.'
Assert-Release ($package.name -ceq 'solx-ls' -and $package.publisher -ceq 'jinxware00' -and
    $package.version -cmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -and $lock.name -ceq $package.name -and
    $lock.version -ceq $package.version -and $lock.packages[''].name -ceq $package.name -and
    $lock.packages[''].version -ceq $package.version) 'Extension release identity or lockfile differs from source.'
$files = Get-SolxExtensionFiles (Join-Path $root 'vscode-solx') $package
$sources = Get-SolxBuildSources $root $files
$checkedPaths = @($sources.Keys) + @('scripts/prepare-release.ps1', 'scripts/test-prepare-release.ps1', '.github/workflows/ci.yml', '.github/workflows/release.yml')
$tracked = @(& git -C $root ls-files -- $checkedPaths)
Assert-Release ($LASTEXITCODE -eq 0) 'Cannot read tracked release inputs.'
foreach ($name in $checkedPaths) { Assert-Release ($tracked -ccontains $name) "Release input is not tracked: $name" }
$dirty = @(& git -C $root status --porcelain --untracked-files=all -- $checkedPaths)
Assert-Release ($LASTEXITCODE -eq 0 -and $dirty.Count -eq 0) 'Commit all packaged and release-automation input changes before preparing a release.'
$archiveName = "solx-$version-windows-x64.zip"
$vsixName = "$($package.name)-$($package.version).vsix"
$provenanceName = "$archiveName.json"
$payloadNames = @($vsixName, $archiveName, $provenanceName)

if (-not $VerifyOnly) {
    Assert-Release (-not (Test-Path -LiteralPath $AssetDirectory)) 'Release asset directory already exists; choose a new directory to preserve earlier output.'
    $archive = Join-Path $root "dist/$archiveName"
    $vsix = Join-Path $root "vscode-solx/$vsixName"
    $rawProvenance = "$archive.json"
    $record = Get-Content -LiteralPath $rawProvenance -Raw | ConvertFrom-Json
    Assert-Release ($record.schemaVersion -eq 2) 'Unsupported installer provenance schema.'
    Assert-ReleaseRecord $record $sources $version $package
    Assert-ReleaseArchive $archive $vsix $record $version $package
    $validated = Test-SolxVsix (Join-Path $root 'vscode-solx') $vsix (Join-Path $root 'vscode-solx/.test-artifacts/package-validation.json')
    Assert-Release ($validated.Sha256 -ieq $record.vsixSha256) 'Current packaged host validation differs from installer VSIX.'
    $public = [ordered]@{
        schemaVersion = 1; kind = 'solx-public-release'; tag = $Tag; sourceCommit = $ExpectedCommit
        interpreterVersion = $version; extensionName = $package.name; extensionPublisher = $package.publisher; extensionVersion = $package.version
        runtimeHasLocalChanges = $false; extensionHasLocalChanges = $false; installerHasLocalChanges = $false
        target = $record.target; builtAtUtc = ([DateTimeOffset]::Parse($record.builtAtUtc)).UtcDateTime.ToString('o')
        toolchain = Get-ReleaseToolchain $record.toolchain
        buildEnvironment = [ordered]@{
            rustFlagsConfigured = -not [string]::IsNullOrEmpty($record.buildEnvironment.RUSTFLAGS)
            encodedRustFlagsConfigured = -not [string]::IsNullOrEmpty($record.buildEnvironment.CARGO_ENCODED_RUSTFLAGS)
            sourceDateEpochConfigured = -not [string]::IsNullOrEmpty($record.buildEnvironment.SOURCE_DATE_EPOCH)
        }
        sourceFiles = Get-ReleaseMap $record.sourceFiles; payloadFiles = Get-ReleaseMap $record.payloadFiles
        packagedValidation = [ordered]@{
            sha256 = $record.vsixSha256.ToLowerInvariant(); vscode = $record.packagedValidation.vscode
            checks = [int]$record.packagedValidation.checks; success = $true; snapshotsRemaining = 0
        }
        sha256 = $record.sha256.ToLowerInvariant(); vsixSha256 = $record.vsixSha256.ToLowerInvariant()
        archive = $archiveName; vsix = $vsixName; buildProvenanceSha256 = Get-ReleaseHash $rawProvenance
        privacy = 'Public fields are explicitly selected. Raw environment values, paths, logs, and the original local build record are omitted.'
        reproducibility = 'Hashes identify the tested bytes and inputs; timestamps and compiler/linker environments can prevent byte-identical rebuilds.'
    }
    New-Item -ItemType Directory -Path $AssetDirectory | Out-Null
    Copy-Item -LiteralPath $archive -Destination (Join-Path $AssetDirectory $archiveName)
    Copy-Item -LiteralPath $vsix -Destination (Join-Path $AssetDirectory $vsixName)
    [IO.File]::WriteAllText((Join-Path $AssetDirectory $provenanceName), (($public | ConvertTo-Json -Depth 10) + "`n"), (New-Object Text.UTF8Encoding $false))
    [IO.File]::WriteAllText((Join-Path $AssetDirectory 'SHA256SUMS'), (Get-ReleaseChecksums $AssetDirectory $payloadNames), (New-Object Text.UTF8Encoding $false))
}

# Also run after download in the publishing job. The artifact has no build
# caches, local receipts, or original provenance; all inputs are checked anew.
$entries = @(Get-ChildItem -LiteralPath $AssetDirectory -Force)
Assert-Release ($entries.Count -eq 4) 'Public artifact must contain exactly four files.'
foreach ($entry in $entries) {
    Assert-Release (-not $entry.PSIsContainer -and (@($payloadNames) + @('SHA256SUMS')) -ccontains $entry.Name) 'Unexpected public artifact entry.'
}
$publicRecord = Get-Content -LiteralPath (Join-Path $AssetDirectory $provenanceName) -Raw | ConvertFrom-Json
Assert-ReleaseKeys $publicRecord @('schemaVersion', 'kind', 'tag', 'sourceCommit', 'interpreterVersion', 'extensionName', 'extensionPublisher', 'extensionVersion',
    'runtimeHasLocalChanges', 'extensionHasLocalChanges', 'installerHasLocalChanges', 'target', 'builtAtUtc', 'toolchain', 'buildEnvironment', 'sourceFiles', 'payloadFiles',
    'packagedValidation', 'sha256', 'vsixSha256', 'archive', 'vsix', 'buildProvenanceSha256', 'privacy', 'reproducibility')
Assert-ReleaseKeys $publicRecord.toolchain @('rustc', 'cargo', 'node', 'npm')
foreach ($name in @('rustc', 'cargo')) {
    $tool = $publicRecord.toolchain.$name
    Assert-ReleaseKeys $tool @('version', 'commit', 'date')
    Assert-Release ($tool.version -is [string] -and $tool.version -cmatch '^[0-9]+\.[0-9]+\.[0-9]+$' -and
        $tool.commit -is [string] -and $tool.commit -cmatch '^[0-9a-f]+$' -and
        $tool.date -is [string] -and $tool.date -cmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}$') 'Invalid public Rust toolchain metadata.'
}
foreach ($name in @('node', 'npm')) {
    Assert-Release ($publicRecord.toolchain.$name -is [string] -and $publicRecord.toolchain.$name -cmatch '^[0-9]+\.[0-9]+\.[0-9]+$') 'Invalid public Node toolchain metadata.'
}
Assert-ReleaseKeys $publicRecord.buildEnvironment @('rustFlagsConfigured', 'encodedRustFlagsConfigured', 'sourceDateEpochConfigured')
foreach ($property in $publicRecord.buildEnvironment.PSObject.Properties) { Assert-Release ($property.Value -is [bool]) 'Invalid public environment indicator.' }
Assert-ReleaseKeys $publicRecord.packagedValidation @('sha256', 'vscode', 'checks', 'success', 'snapshotsRemaining')
Assert-Release ($publicRecord.builtAtUtc -is [string] -and $publicRecord.builtAtUtc -cmatch '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z$' -and
    $publicRecord.buildProvenanceSha256 -is [string] -and $publicRecord.buildProvenanceSha256 -cmatch '^[a-f0-9]{64}$' -and
    $publicRecord.privacy -ceq 'Public fields are explicitly selected. Raw environment values, paths, logs, and the original local build record are omitted.' -and
    $publicRecord.reproducibility -ceq 'Hashes identify the tested bytes and inputs; timestamps and compiler/linker environments can prevent byte-identical rebuilds.') 'Invalid public provenance metadata.'
Assert-Release ($publicRecord.schemaVersion -eq 1 -and $publicRecord.kind -ceq 'solx-public-release' -and
    $publicRecord.tag -ceq $Tag -and $publicRecord.archive -ceq $archiveName -and $publicRecord.vsix -ceq $vsixName -and
    $publicRecord.extensionName -ceq $package.name -and $publicRecord.extensionPublisher -ceq $package.publisher) 'Public release identity differs from source.'
Assert-ReleaseRecord $publicRecord $sources $version $package
Assert-ReleaseArchive (Join-Path $AssetDirectory $archiveName) (Join-Path $AssetDirectory $vsixName) $publicRecord $version $package
$checksums = [IO.File]::ReadAllText((Join-Path $AssetDirectory 'SHA256SUMS'))
Assert-Release ($checksums -ceq (Get-ReleaseChecksums $AssetDirectory $payloadNames)) 'SHA256SUMS differs from the three public payloads.'
Write-Output "Verified $Tag ($ExpectedCommit): $vsixName, $archiveName, $provenanceName, SHA256SUMS"
