param([Parameter(Mandatory=$true)][string] $CodeRoot, [switch] $SkipBuild)
$ErrorActionPreference = 'Stop'
trap {
    $failure = $_
    if ($artifacts -and (Test-Path -LiteralPath $artifacts -PathType Container)) {
        [IO.File]::WriteAllText((Join-Path $artifacts 'failure.log'), $failure.ToString())
    }
    # A bare throw here produces ScriptHalted and hides pre-artifact failures.
    throw $failure
}
Import-Module (Join-Path $PSScriptRoot 'VSCode.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'Installer.psm1') -Force
if (-not (Get-SolxVSCode -Candidates @($CodeRoot))) { throw 'Pass a supported desktop VS Code root for isolated installation.' }
if (-not $SkipBuild) { & (Join-Path $PSScriptRoot 'build.ps1') }
$version = [regex]::Match((Get-Content (Join-Path $PSScriptRoot '../../Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$artifacts = Join-Path $PSScriptRoot ('.artifacts/terminal-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $artifacts | Out-Null
$bundle = Join-Path $artifacts 'bundle'
$archive = Join-Path $PSScriptRoot "../../dist/solx-$version-windows-x64.zip"
$record = Get-Content -LiteralPath ($archive + '.json') -Raw | ConvertFrom-Json
if ($record.schemaVersion -ne 2 -or $record.sha256 -ne (Get-FileHash -LiteralPath $archive).Hash) { throw 'Archive provenance or SHA-256 mismatch.' }
Expand-Archive -LiteralPath $archive -DestinationPath $bundle
$payload = @(Get-ChildItem -LiteralPath $bundle -Recurse -File)
if ($payload.Count -ne @($record.payloadFiles.PSObject.Properties).Count) { throw 'Unexpected extracted payload file count.' }
foreach ($file in $payload) {
    $relative = $file.FullName.Substring($bundle.Length + 1).Replace('\', '/')
    if ($record.payloadFiles.$relative -ne (Get-FileHash -LiteralPath $file.FullName).Hash) { throw "Extracted payload checksum mismatch: $relative" }
}
$manifest = Get-SolxBundle $bundle
if ($manifest.version -ne $version -or $manifest.version -ne $record.interpreterVersion -or
    $manifest.extensionName -cne $record.extensionName -or $manifest.extensionPublisher -cne $record.extensionPublisher -or
    $manifest.extensionVersion -ne $record.extensionVersion) { throw 'Bundle identity and versions do not match source and provenance.' }
$vsixName = "$($manifest.extensionName)-$($manifest.extensionVersion).vsix"
$extensionId = "$($manifest.extensionPublisher).$($manifest.extensionName)"
$vsix = Join-Path $bundle "editor/$vsixName"
if ((Get-FileHash -LiteralPath $vsix).Hash -ne $record.vsixSha256 -or -not $record.packagedValidation.success -or
    $record.packagedValidation.sha256 -ne $record.vsixSha256) { throw 'Bundled VSIX does not match its successful host validation.' }
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
# Recompute the allowlist as well as hashes: added inputs must invalidate a
# skipped build just as modified/deleted files do.
. (Join-Path $PSScriptRoot 'build.ps1')
$extensionDirectory = Join-Path $root 'vscode-solx'
$currentVsix = Test-SolxVsix $extensionDirectory (Join-Path $extensionDirectory $vsixName) (Join-Path $extensionDirectory '.test-artifacts/package-validation.json')
$currentSources = Get-SolxBuildSources $root $currentVsix.Files
if (($currentSources | ConvertTo-Json -Compress) -cne ($record.sourceFiles | ConvertTo-Json -Compress)) { throw 'Bundle was built from different source. Rebuild before testing.' }
$originalPath = Get-SolxUserPath
$installedExe = Join-Path $env:LOCALAPPDATA 'Solx/bin/solx.exe'
$originalExeHash = if (Test-Path $installedExe) { (Get-FileHash $installedExe).Hash } else { '' }
$normalProfileFiles = @((Join-Path $env:APPDATA 'Code/User/settings.json'), (Join-Path $env:USERPROFILE '.vscode/extensions/extensions.json'))
$originalProfileHashes = @{}
foreach ($file in $normalProfileFiles) { $originalProfileHashes[$file] = if (Test-Path -LiteralPath $file -PathType Leaf) { (Get-FileHash -LiteralPath $file).Hash } else { '' } }
$badCode = Join-Path $artifacts 'failing Code'
New-Item -ItemType Directory -Force -Path (Join-Path $badCode 'resources/app/out') | Out-Null
[IO.File]::WriteAllText((Join-Path $badCode 'resources/app/package.json'), '{"version":"1.85.2"}')
[IO.File]::WriteAllText((Join-Path $badCode 'resources/app/out/cli.js'), '// fixture')
Add-Type -TypeDefinition 'public class FailingCli { public static int Main(string[] args) { System.Console.Error.WriteLine("expected installation failure"); return 42; } }' -OutputAssembly (Join-Path $badCode 'Code.exe') -OutputType ConsoleApplication
$missingCli = Join-Path $artifacts 'missing CLI'
New-Item -ItemType Directory -Force -Path (Join-Path $missingCli 'resources/app') | Out-Null
Copy-Item (Join-Path $badCode 'Code.exe') (Join-Path $missingCli 'Code.exe')
Copy-Item (Join-Path $badCode 'resources/app/package.json') (Join-Path $missingCli 'resources/app/package.json')
function Stop-OwnedProcess([Diagnostics.Process] $Process) {
    if ($Process.HasExited) { return }
    # This process was created by this harness. Never select an editor by name.
    $killerInfo = New-Object Diagnostics.ProcessStartInfo
    $killerInfo.FileName = Join-Path $env:SystemRoot 'System32/taskkill.exe'
    $killerInfo.Arguments = '/PID ' + $Process.Id + ' /T /F'
    $killerInfo.UseShellExecute = $false; $killerInfo.CreateNoWindow = $true
    try {
        $killer = [Diagnostics.Process]::Start($killerInfo)
        try {
            if (-not $killer.WaitForExit(5000)) { $killer.Kill() }
        } finally { $killer.Dispose() }
    } finally {
        if (-not $Process.HasExited) { $Process.Kill() }
        if (-not $Process.WaitForExit(5000)) { throw 'Owned test process did not exit after termination.' }
    }
}
function Run-OwnedProcess([string] $Executable, [string[]] $Arguments, [string] $InputText = '', [int] $TimeoutMilliseconds = 180000) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $Executable
    $info.Arguments = ($Arguments | ForEach-Object { ConvertTo-WindowsArgument $_ }) -join ' '
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true; $info.RedirectStandardInput = $true
    $process = [Diagnostics.Process]::Start($info)
    try {
        $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
        $process.StandardInput.Write($InputText); $process.StandardInput.Close()
        if (-not $process.WaitForExit($TimeoutMilliseconds)) {
            throw "Owned test process timed out after $TimeoutMilliseconds ms: $Executable"
        }
        # An exited parent does not guarantee that an inherited pipe was closed.
        if (-not $stdout.Wait(5000) -or -not $stderr.Wait(5000)) { throw 'Owned test process output did not close.' }
        $output = $stdout.GetAwaiter().GetResult()
        $errorOutput = $stderr.GetAwaiter().GetResult()
        return @{ Code = $process.ExitCode; Stdout = $output; Stderr = $errorOutput; Output = $output + $errorOutput }
    } finally {
        try { Stop-OwnedProcess $process } finally { $process.Dispose() }
    }
}
function Run-Terminal([string[]] $Arguments, [string] $InputText = '') {
    $executable = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
    return Run-OwnedProcess $executable (@('-NoProfile','-ExecutionPolicy','Bypass','-File') + $Arguments) $InputText
}
$cases = @(
    @{ Name = 'skipped'; Choice = 'No'; Root = $CodeRoot; Expected = 'skipped' },
    @{ Name = 'selected'; Choice = 'Yes'; Root = $CodeRoot; Expected = 'installed for VS Code' },
    @{ Name = 'missing-code'; Choice = 'Yes'; Root = (Join-Path $artifacts 'absent'); Expected = 'were not found' },
    @{ Name = 'missing-cli'; Choice = 'Yes'; Root = $missingCli; Expected = 'were not found' },
    @{ Name = 'failed-cli'; Choice = 'Yes'; Root = $badCode; Expected = 'code 42' }
)
$results = @()
function Record([string] $Name, [bool] $Success, [string] $Details = '') {
    $script:results += @{ name = $Name; success = $Success; details = $Details }
    $script:results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $artifacts 'results.json') -Encoding UTF8
    if ($Success) { Write-Output "PASS terminal installer: $Name" }
}
function Run-Logged([string] $Name, [string[]] $Arguments, [string] $InputText = '') {
    try { $result = Run-Terminal $Arguments $InputText }
    catch {
        [IO.File]::WriteAllText((Join-Path $artifacts ($Name + '.log')), $_.Exception.ToString())
        throw
    }
    [IO.File]::WriteAllText((Join-Path $artifacts ($Name + '.log')), $result.Output)
    return $result
}
function Install-Arguments([string] $Destination, [string] $BundleDirectory = $bundle) {
    return @((Join-Path $BundleDirectory 'install.ps1'), '-Yes', '-NoColor', '-AddToPath', 'No', '-VSCode', 'No',
        '-InstallDirectory', $Destination, '-CodeRoot', $CodeRoot,
        '-ExtensionsDirectory', (Join-Path $Destination 'test-extensions'), '-UserDataDirectory', (Join-Path $Destination 'test-user-data'))
}
function Assert-Interpreter([string] $Exe) {
    $reportedVersion = Run-OwnedProcess $Exe @('--version') -TimeoutMilliseconds 30000
    if ($reportedVersion.Code -ne 0 -or $reportedVersion.Stdout.TrimEnd("`r", "`n") -cne "solx $version" -or $reportedVersion.Stderr) {
        throw "Installed interpreter version mismatch: $($reportedVersion.Output)"
    }
    $output = Run-OwnedProcess $Exe @('--timeout-ms', '5000', '-e', "import 'json' as json var values = [9007199254740993, nil] print(json.stringify(values))") -TimeoutMilliseconds 30000
    if ($output.Code -ne 0 -or $output.Stdout.TrimEnd("`r", "`n") -cne '[9007199254740993,null]' -or $output.Stderr) {
        throw "Installed runtime does not support the current integer, nil and JSON semantics: $($output.Output)"
    }
    if ((Get-FileHash -LiteralPath $Exe).Hash -ne $manifest.files.'bin/solx.exe') { throw 'Interpreter payload mismatch.' }
}
try {
Assert-Interpreter (Join-Path $bundle 'bin/solx.exe')
Record 'archive and extracted payload provenance, runtime version and semantics' $true
foreach ($case in $cases) {
    $destination = Join-Path $artifacts ($case.Name + ' space & path')
    $result = Run-Logged $case.Name @((Join-Path $bundle 'install.ps1'), '-Yes','-NoColor','-AddToPath','No',
        '-InstallDirectory',$destination,'-VSCode',$case.Choice,'-CodeRoot',$case.Root,
        '-ExtensionsDirectory',(Join-Path $destination 'test-extensions'),'-UserDataDirectory',(Join-Path $destination 'test-user-data'))
    if ($result.Code -ne 0 -or $result.Output -notmatch $case.Expected -or $result.Output -notmatch '\[5/5\]') { throw "Installer $($case.Name) failed: $($result.Output)" }
    if ($result.Output.Contains([char]27)) { throw 'Plain-text mode emitted terminal escape codes.' }
    $exe = Join-Path $destination 'bin/solx.exe'
    Assert-Interpreter $exe
    if ($case.Name -eq 'selected') {
        $package = Get-Content (Join-Path $destination "test-extensions/$extensionId-$($manifest.extensionVersion)/package.json") -Raw | ConvertFrom-Json
        if ($package.name -cne $manifest.extensionName -or $package.publisher -cne $manifest.extensionPublisher -or
            $package.version -ne $manifest.extensionVersion) { throw 'Installed extension identity or version mismatch' }
    } elseif (Test-Path (Join-Path $destination 'test-extensions')) { throw 'Unexpected extension installation' }
    Record $case.Name $true
}
foreach ($cancelCase in @(@{ Name = 'cancel-decline'; Input = "n`n" }, @{ Name = 'cancel-quit'; Input = "q`n" }, @{ Name = 'cancel-eof'; Input = '' })) {
    $cancelTarget = Join-Path $artifacts $cancelCase.Name
    $cancel = Run-Logged $cancelCase.Name @((Join-Path $bundle 'install.ps1'),'-InstallDirectory',$cancelTarget,'-AddToPath','No','-VSCode','No','-NoColor','-CodeRoot',$CodeRoot) $cancelCase.Input
    if ($cancel.Code -ne 130 -or (Test-Path $cancelTarget)) { throw "Cancellation failed: $($cancel.Output)" }
    Record $cancelCase.Name $true
}
$promptTarget = Join-Path $artifacts 'interactive'
$prompt = Run-Logged 'interactive' @((Join-Path $bundle 'install.ps1'),'-InstallDirectory',$promptTarget,'-AddToPath','No','-NoColor','-CodeRoot',$CodeRoot) "n`ny`n"
if ($prompt.Code -ne 0 -or $prompt.Output -notmatch 'Install Solx for Visual Studio Code\?' -or $prompt.Output -notmatch '\[Skipped\]') { throw "Interactive prompts failed: $($prompt.Output)" }
Record 'interactive skip and confirmation' $true

# Exercise upgrades through the extracted entrypoint, including prior metadata.
$upgradeTarget = Join-Path $artifacts 'skipped space & path'
$oldExe = Join-Path $upgradeTarget 'bin/solx.exe'
[IO.File]::WriteAllText($oldExe, 'previous interpreter fixture')
$previousHash = (Get-FileHash -LiteralPath $oldExe).Hash
$upgradeUserFile = Join-Path $upgradeTarget 'keep.solx'
[IO.File]::WriteAllText($upgradeUserFile, 'user program')
$statePath = Join-Path $upgradeTarget 'install-state.json'
$oldState = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
$oldState.version = '0.0.1'
$oldState | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $statePath -Encoding UTF8
$upgrade = Run-Logged 'upgrade' (Install-Arguments $upgradeTarget)
if ($upgrade.Code -ne 0 -or $upgrade.Output -notmatch 'backed up') { throw "Upgrade failed: $($upgrade.Output)" }
Assert-Interpreter $oldExe
$backupExes = @(Get-ChildItem -LiteralPath (Join-Path $upgradeTarget '.solx-backups') -Recurse -File | Where-Object { $_.Name -eq 'solx.exe' })
if ($backupExes.Count -ne 1 -or (Get-FileHash -LiteralPath $backupExes[0].FullName).Hash -ne $previousHash -or
    [IO.File]::ReadAllText($upgradeUserFile) -cne 'user program') { throw 'Upgrade did not preserve previous files and user data.' }
$newState = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
if ($newState.version -ne $version -or $newState.pathAdded) { throw 'Upgrade metadata mismatch.' }
Record 'upgrade preserves backups and user programs' $true

# A directory at a later payload target forces real Copy-Item rollback without
# simulating the production file helper or requiring disk failure injection.
$rollbackTarget = Join-Path $artifacts 'failed replacement'
New-Item -ItemType Directory -Force -Path (Join-Path $rollbackTarget 'bin'), (Join-Path $rollbackTarget 'LICENSE') | Out-Null
[IO.File]::WriteAllText((Join-Path $rollbackTarget 'bin/solx.exe'), 'old interpreter')
[IO.File]::WriteAllText((Join-Path $rollbackTarget 'LICENSE/keep.txt'), 'unrelated data')
$rollback = Run-Logged 'rollback' (Install-Arguments $rollbackTarget)
if ($rollback.Code -ne 1 -or $rollback.Output -notmatch 'Cannot replace a directory' -or
    [IO.File]::ReadAllText((Join-Path $rollbackTarget 'bin/solx.exe')) -cne 'old interpreter' -or
    [IO.File]::ReadAllText((Join-Path $rollbackTarget 'LICENSE/keep.txt')) -cne 'unrelated data' -or
    (Test-Path -LiteralPath (Join-Path $rollbackTarget "editor/$vsixName")) -or
    (Test-Path -LiteralPath (Join-Path $rollbackTarget 'install-state.json'))) { throw "File rollback failed: $($rollback.Output)" }
Record 'failed replacement restores old files and preserves unrelated directories' $true

$damagedBundle = Join-Path $artifacts 'damaged bundle'
Expand-Archive -LiteralPath $archive -DestinationPath $damagedBundle
[IO.File]::AppendAllText((Join-Path $damagedBundle 'bin/solx.exe'), 'corruption')
$damageTarget = Join-Path $artifacts 'damage target'
$damage = Run-Logged 'damaged-bundle' (Install-Arguments $damageTarget $damagedBundle)
if ($damage.Code -ne 1 -or $damage.Output -notmatch 'incomplete or damaged' -or (Test-Path -LiteralPath $damageTarget)) { throw "Damaged bundle was not rejected before installation: $($damage.Output)" }
Record 'damaged bundle is rejected before creating installation files' $true

$unsafeTarget = Join-Path $artifacts 'missing-code space & path'
$unsafeStatePath = Join-Path $unsafeTarget 'install-state.json'
$unsafeState = Get-Content -LiteralPath $unsafeStatePath -Raw | ConvertFrom-Json
$unsafeState.files | Add-Member -NotePropertyName '../outside.txt' -NotePropertyValue ('0' * 64)
$unsafeState | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $unsafeStatePath -Encoding UTF8
$unsafe = Run-Logged 'unsafe-uninstall' @((Join-Path $unsafeTarget 'uninstall.ps1'), '-Yes')
if ($unsafe.Code -ne 1 -or $unsafe.Output -notmatch 'Unsafe installation metadata') { throw "Unsafe uninstall metadata accepted: $($unsafe.Output)" }
Assert-Interpreter (Join-Path $unsafeTarget 'bin/solx.exe')
Record 'uninstall rejects unsafe metadata before deleting any files' $true

$uninstallCancel = Run-Logged 'uninstall-cancel' @((Join-Path $promptTarget 'uninstall.ps1')) "n`n"
if ($uninstallCancel.Code -ne 130) { throw "Uninstall cancellation failed: $($uninstallCancel.Output)" }
Assert-Interpreter (Join-Path $promptTarget 'bin/solx.exe')
Record 'uninstall cancellation preserves installation' $true
$userFile = Join-Path $promptTarget 'my-program.solx'
[IO.File]::WriteAllText($userFile, 'print(42)')
[IO.File]::AppendAllText((Join-Path $promptTarget 'INSTALLING.md'), 'user edits')
$uninstall = Run-Logged 'uninstall' @((Join-Path $promptTarget 'uninstall.ps1'), '-Yes')
if ($uninstall.Code -ne 0 -or (Test-Path (Join-Path $promptTarget 'bin/solx.exe')) -or
    -not (Test-Path $userFile) -or -not (Test-Path (Join-Path $promptTarget 'INSTALLING.md'))) { throw "Safe uninstall failed: $($uninstall.Output)" }
Record 'uninstall preserves user and modified files' $true

$upgradeUninstall = Run-Logged 'upgrade-uninstall' @((Join-Path $upgradeTarget 'uninstall.ps1'), '-Yes')
if ($upgradeUninstall.Code -ne 0 -or (Test-Path -LiteralPath $oldExe) -or
    (Get-FileHash -LiteralPath $backupExes[0].FullName).Hash -ne $previousHash -or
    [IO.File]::ReadAllText($upgradeUserFile) -cne 'user program') { throw "Upgraded uninstall lost backups or user files: $($upgradeUninstall.Output)" }
Record 'uninstall after upgrade preserves backups' $true

$selectedTarget = Join-Path $artifacts 'selected space & path'
$selectedPackage = Join-Path $selectedTarget "test-extensions/$extensionId-$($manifest.extensionVersion)/package.json"
$selectedPackageHash = (Get-FileHash -LiteralPath $selectedPackage).Hash
$selectedUninstall = Run-Logged 'selected-uninstall' @((Join-Path $selectedTarget 'uninstall.ps1'), '-Yes')
if ($selectedUninstall.Code -ne 0 -or (Test-Path -LiteralPath (Join-Path $selectedTarget 'bin/solx.exe')) -or
    (Get-FileHash -LiteralPath $selectedPackage).Hash -ne $selectedPackageHash) { throw "Uninstall removed editor integration: $($selectedUninstall.Output)" }
Record 'uninstall preserves the separately installed editor extension' $true
} catch {
    Record 'failure' $false $_.Exception.Message
    throw
} finally {
if ((Get-SolxUserPath) -cne $originalPath) { throw 'Host user PATH changed.' }
$finalExeHash = if (Test-Path $installedExe) { (Get-FileHash $installedExe).Hash } else { '' }
if ($originalExeHash -ne $finalExeHash) { throw 'Host Solx executable changed.' }
foreach ($file in $normalProfileFiles) {
    $hash = if (Test-Path -LiteralPath $file -PathType Leaf) { (Get-FileHash -LiteralPath $file).Hash } else { '' }
    if ($hash -ne $originalProfileHashes[$file]) { throw 'Normal VS Code settings or extension registration changed.' }
}
}
Write-Output "$($results.Count) terminal installer checks passed; host PATH, interpreter and normal editor settings preserved. Artifacts: $artifacts"
