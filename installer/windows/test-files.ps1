$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'Installer.psm1') -Force
$artifacts = Join-Path $PSScriptRoot ('.artifacts/files-' + [guid]::NewGuid().ToString('N'))
$bundle = Join-Path $artifacts 'bundle'
New-Item -ItemType Directory -Force -Path (Join-Path $bundle 'bin'),(Join-Path $bundle 'editor') | Out-Null
$files = [ordered]@{}
foreach ($file in @('bin/solx.exe','editor/solx-language-support-0.2.1.vsix','LICENSE','INSTALLING.md','uninstall.ps1','Installer.psm1')) {
    [IO.File]::WriteAllText((Join-Path $bundle $file), "fixture $file")
    $files[$file] = (Get-FileHash (Join-Path $bundle $file)).Hash
}
$manifest = @{ name = 'solx'; version = '0.1.0'; extensionVersion = '0.2.1'; files = $files }
$manifest | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $bundle 'bundle.json')
$script:passed = 0
function Check([string] $Name, [scriptblock] $Action) { & $Action; $script:passed++; Write-Output "PASS $Name" }
function Equal($Actual, $Expected) { if ($Actual -cne $Expected) { throw "Expected '$Expected', got '$Actual'" } }
function Reject([scriptblock] $Action, [string] $Pattern) {
    $errorCaught = $null
    try { & $Action | Out-Null } catch { $errorCaught = $_.Exception.Message }
    if (-not $errorCaught -or $errorCaught -notmatch $Pattern) { throw "Expected rejection matching $Pattern, got $errorCaught" }
}
Check 'PATH addition is idempotent and preserves literal variables and separators' {
    $path = ';%USERPROFILE%\Tools;;C:\Other;'
    Equal (Update-SolxPathValue $path 'C:\Solx\bin') ($path + 'C:\Solx\bin')
    Equal (Update-SolxPathValue 'C:\Other;"c:\solx\bin\";' 'C:\Solx\bin') 'C:\Other;"c:\solx\bin\";'
    Equal (Update-SolxPathValue '' 'C:\Solx\bin') 'C:\Solx\bin'
}
Check 'PATH removal preserves unrelated entries exactly' {
    Equal (Update-SolxPathValue ';C:\Solx\bin;;%X%\tools;C:\Other;' 'C:\Solx\bin' -Remove) ';;%X%\tools;C:\Other;'
    Equal (Update-SolxPathValue 'C:\Other;' 'C:\Solx\bin' -Remove) 'C:\Other;'
}
Check 'bundle validation rejects damaged files before installation' {
    $valid = Get-SolxBundle $bundle
    Equal $valid.version '0.1.0'
    $file = Join-Path $bundle 'LICENSE'
    $original = [IO.File]::ReadAllText($file)
    [IO.File]::WriteAllText($file, 'damaged')
    try { Reject { Get-SolxBundle $bundle } 'damaged' }
    finally { [IO.File]::WriteAllText($file, $original) }
}
Check 'unsafe bundle entries and installation paths are rejected' {
    $manifest.files['../escape'] = 'fake'
    $manifest | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $bundle 'bundle.json')
    try { Reject { Get-SolxBundle $bundle } 'file list' }
    finally { $manifest.files.Remove('../escape'); $manifest | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $bundle 'bundle.json') }
    Reject { Get-SolxInstallDirectory 'C:\' } 'drive root'
    Reject { Get-SolxInstallDirectory 'relative' } 'absolute'
    Reject { Get-SolxInstallDirectory 'C:\bad;path' } 'semicolons'
}
Check 'failed replacement restores previous files and removes partial new files' {
    $target = Join-Path $artifacts 'rollback'
    New-Item -ItemType Directory -Force -Path (Join-Path $target 'bin') | Out-Null
    [IO.File]::WriteAllText((Join-Path $target 'bin/solx.exe'), 'original interpreter')
    [IO.File]::WriteAllText((Join-Path $target 'my.solx'), 'user program')
    $counter = [ref]0
    $copy = { param($Source,$Destination) $counter.Value++; if ($counter.Value -eq 3) { throw 'simulated disk failure' }; Copy-Item -LiteralPath $Source -Destination $Destination }.GetNewClosure()
    Reject { Install-SolxFiles $bundle $target (Get-SolxBundle $bundle) -CopyFile $copy } 'simulated disk failure'
    Equal ([IO.File]::ReadAllText((Join-Path $target 'bin/solx.exe'))) 'original interpreter'
    Equal ([IO.File]::ReadAllText((Join-Path $target 'my.solx'))) 'user program'
    Equal (Test-Path (Join-Path $target 'editor/solx-language-support-0.2.1.vsix')) $false
}
Check 'successful upgrade retains backups and user-created files' {
    $target = Join-Path $artifacts 'rollback'
    $installed = Install-SolxFiles $bundle $target (Get-SolxBundle $bundle)
    Equal ([IO.File]::ReadAllText((Join-Path $installed.BackupDirectory 'bin/solx.exe'))) 'original interpreter'
    Equal ([IO.File]::ReadAllText((Join-Path $target 'my.solx'))) 'user program'
    Equal (Get-FileHash (Join-Path $target 'bin/solx.exe')).Hash $files['bin/solx.exe']
}
Write-Output "$passed installer file/PATH checks passed. Artifacts: $artifacts"
