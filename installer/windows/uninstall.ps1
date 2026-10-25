param([switch] $Yes)
$ErrorActionPreference = 'Stop'
try {
    Import-Module (Join-Path $PSScriptRoot 'Installer.psm1') -Force
    $directory = Get-SolxInstallDirectory $PSScriptRoot
    $state = Get-Content -LiteralPath (Join-Path $directory 'install-state.json') -Raw | ConvertFrom-Json
    if ($state.directory -ine $directory) { throw 'Installation metadata does not match this directory.' }
    # Permit only current Solx payload names and its historical extension name.
    # This preserves safe removal of metadata written before the rename.
    $allowedFile = '^(bin/solx\.exe|editor/(solx-ls|solx-language-support)-\d+\.\d+\.\d+\.vsix|LICENSE|INSTALLING\.md|uninstall\.ps1|Installer\.psm1)$'
    foreach ($file in $state.files.PSObject.Properties) {
        if ($file.Name -notmatch $allowedFile) { throw 'Unsafe installation metadata; refusing to remove files.' }
    }
    Write-Host "Solx / Uninstall from $directory"
    Write-Host 'Modified files, backups, your programs, and the VS Code extension will be preserved.'
    if (-not $Yes -and (Read-Host 'Remove installed Solx files? [y/N]') -notmatch '^(y|yes)$') { Write-Host 'Cancelled.'; exit 130 }
    if ($state.pathAdded) {
        $path = Get-SolxUserPath
        Set-SolxUserPath (Update-SolxPathValue -Value $path -Entry (Join-Path $directory 'bin') -Remove)
        Write-Host 'Removed the PATH entry added by this installer.'
    }
    foreach ($file in $state.files.PSObject.Properties) {
        if ($file.Name -notmatch $allowedFile) { throw 'Unsafe installation metadata; refusing to remove files.' }
        $target = Join-Path $directory $file.Name
        [void](Get-SolxInstallDirectory (Split-Path -Parent $target))
        if (Test-Path -LiteralPath $target -PathType Leaf) {
            if ((Get-Item -LiteralPath $target -Force).LinkType) { throw "Refusing to remove a link: $target" }
            if ((Get-FileHash -LiteralPath $target).Hash -eq $file.Value) { Remove-Item -LiteralPath $target; Write-Host "Removed $($file.Name)" }
            else { Write-Host "Preserved modified file: $($file.Name)" }
        }
    }
    Remove-Item -LiteralPath (Join-Path $directory 'install-state.json')
    Write-Host 'Solx uninstalled. Remaining files and folders were preserved. Open a new terminal.'
    exit 0
} catch { Write-Host "Uninstall could not finish: $($_.Exception.Message)"; exit 1 }
