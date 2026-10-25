Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-SolxVSCodeLayout {
    param([string] $Directory)
    if (-not $Directory -or -not [IO.Path]::IsPathRooted($Directory)) { return $null }
    $root = [IO.Path]::GetFullPath($Directory).TrimEnd('\')
    $relative = 'resources\app\out\cli.js'
    $enableFlag = $true
    $wrapper = Join-Path $root 'bin/code.cmd'
    if (Test-Path -LiteralPath $wrapper -PathType Leaf) {
        # Read only the known relative CLI path from Microsoft's launcher. Never
        # execute or expand its batch text. New VS Code installs use a hash folder.
        $text = Get-Content -LiteralPath $wrapper -Raw
        $match = [regex]::Match($text, '"%~dp0\.\.\\((?:[A-Za-z0-9._-]+\\)*resources\\app\\out\\cli\.js)"')
        if (-not $match.Success) { return $null }
        $relative = $match.Groups[1].Value
        $enableFlag = $text.Contains('--ms-enable-electron-run-as-node')
    }
    $cli = [IO.Path]::GetFullPath((Join-Path $root $relative))
    if (-not $cli.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { return $null }
    $exe = Join-Path $root 'Code.exe'
    $manifest = Join-Path (Split-Path -Parent (Split-Path -Parent $cli)) 'package.json'
    if (-not ((Test-Path -LiteralPath $exe -PathType Leaf) -and (Test-Path -LiteralPath $cli -PathType Leaf) -and
        (Test-Path -LiteralPath $manifest -PathType Leaf))) { return $null }
    try {
        $version = [version]((Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json).version)
        if ($version -lt [version]'1.85.0') { return $null }
    } catch { return $null }
    return @{ Root = $root; Executable = $exe; Cli = $cli; EnableElectronFlag = $enableFlag }
}

function Get-SolxVSCode {
    param([string[]] $Candidates)
    if (-not $PSBoundParameters.ContainsKey('Candidates')) {
        $Candidates = @(
            (Join-Path $env:LOCALAPPDATA 'Programs\Microsoft VS Code'),
            (Join-Path $env:ProgramFiles 'Microsoft VS Code')
        )
        if ($env:ProgramW6432) { $Candidates += Join-Path $env:ProgramW6432 'Microsoft VS Code' }
        foreach ($key in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\Code.exe',
                           'HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths\Code.exe')) {
            if (Test-Path $key) {
                $exe = (Get-Item $key).GetValue('')
                if ($exe) { $Candidates += Split-Path -Parent $exe }
            }
        }
        foreach ($directory in ($env:PATH -split ';')) {
            $directory = $directory.Trim('"')
            if ([IO.Path]::IsPathRooted($directory) -and (Test-Path -LiteralPath (Join-Path $directory 'code.cmd'))) {
                $Candidates += Split-Path -Parent $directory
            }
        }
    }
    foreach ($directory in $Candidates) {
        $layout = Get-SolxVSCodeLayout $directory
        if ($layout) { return $layout.Root }
    }
    return ''
}

function ConvertTo-WindowsArgument {
    param([AllowEmptyString()][string] $Value)
    # Windows CommandLineToArgvW / C runtime quoting, without cmd.exe.
    if ($Value.IndexOf([char]0) -ge 0) { throw 'Arguments cannot contain NUL.' }
    return '"' + ([regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1')) + '"'
}

function Stop-SolxCLIProcess {
    param([Diagnostics.Process] $Process)
    try {
        if ($Process.HasExited) { return }
        $info = New-Object Diagnostics.ProcessStartInfo
        $info.FileName = Join-Path $env:SystemRoot 'System32\taskkill.exe'
        $info.Arguments = '/PID ' + $Process.Id + ' /T /F'
        $info.UseShellExecute = $false; $info.CreateNoWindow = $true
        $killer = [Diagnostics.Process]::Start($info)
        try { [void]$killer.WaitForExit(5000) } finally { $killer.Dispose() }
        if (-not $Process.HasExited) { $Process.Kill() }
    } catch { try { if (-not $Process.HasExited) { $Process.Kill() } } catch {} }
}

function Install-SolxVSCode {
    param(
        [bool] $Selected,
        [string] $CodeRoot,
        [string] $Vsix,
        [string] $LogPath,
        [string] $ExtensionsDirectory,
        [string] $UserDataDirectory,
        [ValidateRange(1, 600)][int] $TimeoutSeconds = 120
    )
    if (-not $Selected) { return @{ Status = 'Skipped'; Message = 'VS Code extension skipped.' } }
    $manual = 'Solx remains installed. In VS Code, use Extensions: Install from VSIX and select ' + $Vsix + '. Restart VS Code after installation.'
    $layout = Get-SolxVSCodeLayout $CodeRoot
    if (-not $layout) {
        return @{ Status = 'Unavailable'; Message = 'Supported desktop VS Code (1.85+) and its CLI were not found. ' + $manual }
    }
    if (-not (Test-Path -LiteralPath $Vsix -PathType Leaf)) {
        return @{ Status = 'Failed'; Message = 'The bundled extension package is missing. ' + $manual }
    }
    $arguments = @($layout.Cli)
    if ($layout.EnableElectronFlag) { $arguments += '--ms-enable-electron-run-as-node' }
    $arguments += @('--install-extension', $Vsix, '--force')
    if ($ExtensionsDirectory) { $arguments += @('--extensions-dir', $ExtensionsDirectory) }
    if ($UserDataDirectory) { $arguments += @('--user-data-dir', $UserDataDirectory) }
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $layout.Executable
    $info.Arguments = ($arguments | ForEach-Object { ConvertTo-WindowsArgument $_ }) -join ' '
    $info.WorkingDirectory = $CodeRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables['ELECTRON_RUN_AS_NODE'] = '1'
    $info.EnvironmentVariables.Remove('VSCODE_DEV')
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $info
    $started = $false
    try {
        if (-not $process.Start()) { throw 'VS Code CLI did not start.' }
        $started = $true
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $timeoutMilliseconds = $TimeoutSeconds * 1000
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($timeoutMilliseconds)) {
            # Terminate only this CLI's process tree, never an existing editor.
            Stop-SolxCLIProcess $process
            return @{ Status = 'Failed'; Message = 'VS Code extension installation timed out. ' + $manual }
        }
        # Descendants can inherit these pipes after the CLI parent exits. Keep
        # output draining within the same deadline as process completion.
        foreach ($reader in @($stdout, $stderr)) {
            $remaining = [Math]::Max(0, $timeoutMilliseconds - [int]$timer.ElapsedMilliseconds)
            if (-not $reader.Wait($remaining)) {
                return @{ Status = 'Failed'; Message = 'VS Code extension installation timed out waiting for CLI output to close. ' + $manual }
            }
        }
        $log = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
        if ($LogPath) { [IO.File]::WriteAllText($LogPath, $log) }
        if ($process.ExitCode -ne 0) {
            return @{ Status = 'Failed'; Message = 'VS Code CLI exited with code ' + $process.ExitCode + '. See ' + $LogPath + '. ' + $manual }
        }
        return @{ Status = 'Installed'; Message = 'Solx Language Support installed for VS Code. Restart VS Code to pick up the Solx PATH.' }
    } catch {
        if ($LogPath) { [IO.File]::WriteAllText($LogPath, $_.Exception.Message) }
        return @{ Status = 'Failed'; Message = 'Could not start or complete the VS Code CLI. See ' + $LogPath + '. ' + $manual }
    } finally {
        # Also reached when PowerShell is interrupted with Ctrl+C.
        if ($started) { Stop-SolxCLIProcess $process }
        $process.Dispose()
    }
}

Export-ModuleMember -Function Get-SolxVSCode, Get-SolxVSCodeLayout, Install-SolxVSCode, ConvertTo-WindowsArgument
