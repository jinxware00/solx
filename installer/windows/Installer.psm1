Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-SolxBundle {
    param([string] $Directory)
    $manifest = Get-Content -LiteralPath (Join-Path $Directory 'bundle.json') -Raw | ConvertFrom-Json
    if ($manifest.name -ne 'solx' -or $manifest.version -notmatch '^\d+\.\d+\.\d+$' -or
        $manifest.extensionName -cne 'solx-ls' -or $manifest.extensionPublisher -cne 'jinxware00' -or
        $manifest.extensionVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid Solx bundle metadata.' }
    $expected = @('bin/solx.exe', "editor/$($manifest.extensionName)-$($manifest.extensionVersion).vsix",
        'LICENSE', 'INSTALLING.md', 'uninstall.ps1', 'Installer.psm1')
    $actual = @($manifest.files.PSObject.Properties.Name)
    if (@(Compare-Object $expected $actual).Count -ne 0) { throw 'Unexpected bundle file list.' }
    foreach ($file in $expected) {
        $source = Join-Path $Directory $file
        if (-not (Test-Path -LiteralPath $source -PathType Leaf) -or
            (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $manifest.files.$file) {
            throw "Bundle is incomplete or damaged: $file. Extract a fresh installer archive."
        }
    }
    return $manifest
}

function Get-SolxInstallDirectory {
    param([string] $Directory)
    if (-not [IO.Path]::IsPathRooted($Directory) -or $Directory -match '[;\r\n\x00]') {
        throw 'Choose an absolute installation path without semicolons or line breaks.'
    }
    $full = [IO.Path]::GetFullPath($Directory).TrimEnd('\')
    if ($full.TrimEnd('\') -eq [IO.Path]::GetPathRoot($full).TrimEnd('\')) { throw 'Choose a folder, not a drive root.' }
    # Keep file operations within the selected directory; do not follow junctions.
    $current = $full
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if ($item.LinkType) { throw 'Choose an installation path without junctions or symbolic links.' }
            if (-not $item.PSIsContainer) { throw "Installation path is not a directory: $current" }
        }
        $parent = Split-Path -Parent $current
        if ($parent -eq $current) { break }
        $current = $parent
    }
    return $full
}

function Update-SolxPathValue {
    param([AllowNull()][AllowEmptyString()][string] $Value, [string] $Entry, [switch] $Remove)
    $parts = @($Value -split ';')
    $matching = @($parts | Where-Object { $_.Trim().Trim('"').TrimEnd('\') -ieq $Entry.TrimEnd('\') })
    if ($Remove) { return (($parts | Where-Object { $_.Trim().Trim('"').TrimEnd('\') -ine $Entry.TrimEnd('\') }) -join ';') }
    if ($matching.Count) { return $Value }
    if (-not $Value) { return $Entry }
    if ($Value.EndsWith(';')) { return $Value + $Entry }
    return $Value + ';' + $Entry
}

function Set-SolxUserPath {
    param([string] $Value)
    # Preserve literal %VARIABLE% entries instead of expanding and rewriting them.
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try { $key.SetValue('Path', $Value, [Microsoft.Win32.RegistryValueKind]::ExpandString) }
    finally { $key.Dispose() }
    try {
        if (-not ('Solx.EnvironmentBroadcast' -as [type])) {
            Add-Type 'namespace Solx { public class EnvironmentBroadcast { [System.Runtime.InteropServices.DllImport("user32.dll", CharSet=System.Runtime.InteropServices.CharSet.Unicode)] public static extern System.IntPtr SendMessageTimeout(System.IntPtr window, uint message, System.UIntPtr parameter, string text, uint flags, uint timeout, out System.UIntPtr result); } }'
        }
        $result = [UIntPtr]::Zero
        [void][Solx.EnvironmentBroadcast]::SendMessageTimeout([IntPtr]65535, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$result)
    } catch { Write-Warning 'PATH was saved. Sign out and back in if new terminals do not receive it.' }
}

function Get-SolxUserPath {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    if (-not $key) { return '' }
    try { return [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $key.Dispose() }
}

function Install-SolxFiles {
    param([string] $BundleDirectory, [string] $Directory, $Manifest,
        [scriptblock] $CopyFile = { param($Source, $Destination) Copy-Item -LiteralPath $Source -Destination $Destination })
    $Directory = Get-SolxInstallDirectory $Directory
    if ($Directory -ieq [IO.Path]::GetFullPath($BundleDirectory).TrimEnd('\')) { throw 'Install into a different folder from the extracted bundle.' }
    $backup = Join-Path $Directory ('.solx-backups/' + [guid]::NewGuid().ToString('N'))
    $written = New-Object 'System.Collections.Generic.List[object]'
    $committed = $false
    try {
        foreach ($file in $Manifest.files.PSObject.Properties.Name) {
            $target = Join-Path $Directory $file
            [void](Get-SolxInstallDirectory (Split-Path -Parent $target))
            if (Test-Path -LiteralPath $target -PathType Container) { throw "Cannot replace a directory with a file: $target" }
            if ((Test-Path -LiteralPath $target) -and (Get-Item -LiteralPath $target -Force).LinkType) { throw "Cannot replace a link: $target" }
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
            $previous = $null
            if (Test-Path -LiteralPath $target) {
                $previous = Join-Path $backup $file
                [void](Get-SolxInstallDirectory (Split-Path -Parent $previous))
                New-Item -ItemType Directory -Force -Path (Split-Path -Parent $previous) | Out-Null
                # Copy before replacement; a locked executable is never moved away.
                Copy-Item -LiteralPath $target -Destination $previous
            }
            $written.Add(@{ Target = $target; Previous = $previous })
            & $CopyFile (Join-Path $BundleDirectory $file) $target
            if ((Get-FileHash -LiteralPath $target).Hash -ne $Manifest.files.$file) { throw "File verification failed: $file" }
        }
        $committed = $true
        return @{ Directory = $Directory; BackupDirectory = $backup }
    } finally {
        if (-not $committed) {
            for ($i = $written.Count - 1; $i -ge 0; $i--) {
                $entry = $written[$i]
                if ($entry.Previous) { Copy-Item -LiteralPath $entry.Previous -Destination $entry.Target -ErrorAction Continue }
                elseif (Test-Path -LiteralPath $entry.Target) { Remove-Item -LiteralPath $entry.Target -ErrorAction Continue }
            }
        }
    }
}

Export-ModuleMember -Function Get-SolxBundle, Get-SolxInstallDirectory, Update-SolxPathValue, Get-SolxUserPath, Set-SolxUserPath, Install-SolxFiles
