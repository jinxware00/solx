[CmdletBinding()]
param(
    [string] $InstallDirectory = (Join-Path $env:LOCALAPPDATA 'Solx'),
    [ValidateSet('Prompt','Yes','No')][string] $VSCode = 'Prompt',
    [ValidateSet('Prompt','Yes','No')][string] $AddToPath = 'Prompt',
    [switch] $Yes,
    [switch] $NoColor,
    [string] $CodeRoot,
    [string] $ExtensionsDirectory,
    [string] $UserDataDirectory
)
$ErrorActionPreference = 'Stop'
$useColor = -not ($NoColor -or $env:NO_COLOR -or $env:TERM -eq 'dumb' -or [Console]::IsOutputRedirected)
function Say([string] $Text, [ConsoleColor] $Color = 'Gray') {
    if ($useColor) { Write-Host $Text -ForegroundColor $Color } else { Write-Host $Text }
}
function Ask([string] $Prompt, [string] $Default) {
    if ([Console]::IsInputRedirected) { Write-Host "$Prompt [$Default]: " -NoNewline; $answer = [Console]::ReadLine() }
    else { $answer = Read-Host "$Prompt [$Default]" }
    if ($null -eq $answer -or $answer -match '^(q|quit|cancel)$') { throw [OperationCanceledException]::new('Installation cancelled.') }
    if ($answer -eq '') { return $Default }
    return $answer.Trim()
}
function Confirm([string] $Prompt, [bool] $Default) {
    $hint = if ($Default) { 'Y/n' } else { 'y/N' }
    while ($true) {
        $answer = Ask $Prompt $hint
        if ($answer -eq $hint) { return $Default }
        if ($answer -match '^(y|yes)$') { return $true }
        if ($answer -match '^(n|no)$') { return $false }
        Say '  Enter y or n, or q to cancel.' Yellow
    }
}
$filesInstalled = $false
$finished = $false
$handled = $false
try {
    Say ''
    Say '   ____   ___   _    __  __' Yellow
    Say '  / ___| / _ \ | |   \ \/ /' Yellow
    Say '  \___ \| | | || |    >  < ' Yellow
    Say '   ___) | |_| || |___/ /\ \' Yellow
    Say '  |____/ \___/ |_____/_/  \_\' Yellow
    Say '  Language tools / Windows x64 installer' DarkGray
    Say '  --------------------------------------' DarkGray
    Say ''
    Say '[1/5] Check bundled files and environment' Cyan
    Import-Module (Join-Path $PSScriptRoot 'Installer.psm1') -Force
    Import-Module (Join-Path $PSScriptRoot 'VSCode.psm1') -Force
    if (-not [Environment]::Is64BitOperatingSystem) { throw 'This bundle requires 64-bit Windows.' }
    $manifest = Get-SolxBundle $PSScriptRoot
    Say "  [+] Solx $($manifest.version) / Language Support $($manifest.extensionVersion)" Green
    if ($PSBoundParameters.ContainsKey('CodeRoot')) { $code = Get-SolxVSCode -Candidates @($CodeRoot) }
    else { $code = Get-SolxVSCode }
    if ($code) { Say "  [+] VS Code found: $code" Green }
    else { Say '  [!] VS Code 1.85+ and its CLI were not found. Solx can still be installed.' Yellow }

    Say '[2/5] Choose installation options' Cyan
    if (-not $Yes -and -not $PSBoundParameters.ContainsKey('InstallDirectory')) { $InstallDirectory = Ask '  Installation folder (q cancels)' $InstallDirectory }
    $InstallDirectory = Get-SolxInstallDirectory $InstallDirectory
    $pathSelected = if ($AddToPath -eq 'Prompt') { if ($Yes) { $true } else { Confirm '  Add Solx to your user PATH?' $true } } else { $AddToPath -eq 'Yes' }
    $editorSelected = if ($VSCode -eq 'Prompt') { if ($Yes -or -not $code) { $false } else { Confirm '  Install Solx for Visual Studio Code?' $true } } else { $VSCode -eq 'Yes' }
    Say "  Location : $InstallDirectory"
    Say "  User PATH: $pathSelected"
    Say "  VS Code  : $editorSelected (bundled extension; no downloads)"
    if (Test-Path -LiteralPath (Join-Path $InstallDirectory 'bin/solx.exe')) { Say '  [!] Existing Solx files will be backed up before replacement.' Yellow }
    if (-not $Yes -and -not (Confirm '  Install with these settings?' $true)) { throw [OperationCanceledException]::new('Installation cancelled.') }

    Say '[3/5] Install and verify Solx' Cyan
    $oldStatePath = Join-Path $InstallDirectory 'install-state.json'
    $pathOwned = $false
    if (Test-Path -LiteralPath $oldStatePath) {
        if ((Get-Item -LiteralPath $oldStatePath -Force).LinkType) { throw 'Installation metadata cannot be a link.' }
        $oldState = Get-Content -LiteralPath $oldStatePath -Raw | ConvertFrom-Json
        if ($oldState.directory -ine $InstallDirectory) { throw 'Existing installation metadata belongs to a different folder.' }
        $pathOwned = [bool]$oldState.pathAdded
    }
    $installed = Install-SolxFiles -BundleDirectory $PSScriptRoot -Directory $InstallDirectory -Manifest $manifest
    $filesInstalled = $true
    Say '  [+] Interpreter and bundled editor package verified.' Green
    if (Test-Path -LiteralPath $installed.BackupDirectory) { Say "  Previous files: $($installed.BackupDirectory)" DarkGray }
    $state = [ordered]@{ directory = $InstallDirectory; version = $manifest.version; pathAdded = $pathOwned; files = $manifest.files }
    $state | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $oldStatePath -Encoding UTF8

    Say '[4/5] Configure user PATH' Cyan
    $bin = Join-Path $InstallDirectory 'bin'
    $pathAvailable = $false
    if ($pathSelected) {
        $before = Get-SolxUserPath
        $after = Update-SolxPathValue -Value $before -Entry $bin
        if ($after -cne $before) {
            try {
                Set-SolxUserPath $after
                $state.pathAdded = $true
                $state | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $oldStatePath -Encoding UTF8
                Say "  [+] Added $bin to your user PATH. Open a new terminal." Green
                $pathAvailable = $true
            } catch { Say "  [!] PATH update incomplete: $($_.Exception.Message). Add $bin manually." Yellow }
        } else { $pathAvailable = $true; Say '  [+] PATH already contains this installation; no duplicate added.' Green }
    } else { Say "  [-] PATH unchanged. Run Solx using $bin\solx.exe" DarkGray }

    Say '[5/5] Configure Visual Studio Code' Cyan
    $vsix = Join-Path $InstallDirectory "editor/$($manifest.extensionName)-$($manifest.extensionVersion).vsix"
    if ($editorSelected) { Say '  Installing bundled language support (up to 120 seconds)...' DarkGray }
    $editor = Install-SolxVSCode -Selected $editorSelected -CodeRoot $code -Vsix $vsix -LogPath (Join-Path $InstallDirectory 'vscode-install.log') -ExtensionsDirectory $ExtensionsDirectory -UserDataDirectory $UserDataDirectory
    [IO.File]::WriteAllText((Join-Path $InstallDirectory 'editor-install-result.txt'), $editor.Message)
    $color = if ($editor.Status -eq 'Installed') { 'Green' } elseif ($editor.Status -eq 'Skipped') { 'DarkGray' } else { 'Yellow' }
    Say "  [$($editor.Status)] $($editor.Message)" $color
    Say ''
    Say '  Solx is ready.' Green
    Say "  Installed: $InstallDirectory"
    Say '  Open a new terminal, then:'
    if ($pathAvailable) { Say '    solx --version'; Say '    solx your-file.solx'; Say '    solx' }
    else { Say ('    & "' + (Join-Path $bin 'solx.exe') + '" your-file.solx') }
    Say '  VS Code: open a .solx file, then run Solx: Run Current File.'
    Say '  Docs: https://github.com/jinxware00/solx'
    Say ('  Uninstall: powershell -NoProfile -ExecutionPolicy Bypass -File "' + (Join-Path $InstallDirectory 'uninstall.ps1') + '"') DarkGray
    Say ''
    $finished = $true
    exit 0
} catch [OperationCanceledException] {
    $handled = $true
    Say '  [!] Installation cancelled. No installation changes were made.' Yellow
    exit 130
} catch {
    $handled = $true
    Say "  [x] $($_.Exception.Message)" Red
    if ($filesInstalled) { Say "  Solx files are installed at $InstallDirectory. Optional setup can be retried." Yellow }
    else { Say '  Solx was not installed. Any replaced files were restored; existing programs are preserved.' Yellow }
    exit 1
} finally {
    # PowerShell executes finally on Ctrl+C as well as normal cancellation.
    # Install-SolxFiles rolls back incomplete file replacement in its own finally.
    if (-not $finished -and -not $handled) {
        Write-Host 'Installation interrupted. Incomplete file replacement is rolled back; completed installation stages are preserved. Run the installer again to finish.'
    }
}
