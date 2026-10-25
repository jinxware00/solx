$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VSCode.psm1') -Force
$artifacts = Join-Path $PSScriptRoot ('.artifacts/tests-' + [guid]::NewGuid().ToString('N'))
$fake = Join-Path $artifacts 'Code space & percent% directory'
New-Item -ItemType Directory -Force -Path (Join-Path $fake 'resources/app/out') | Out-Null
$fake = [IO.Path]::GetFullPath($fake)
[IO.File]::WriteAllText((Join-Path $fake 'resources/app/package.json'), '{"version":"1.85.2"}')
[IO.File]::WriteAllText((Join-Path $fake 'resources/app/out/cli.js'), '// CLI fixture')
$record = Join-Path $artifacts 'arguments.txt'
$log = Join-Path $artifacts 'cli.log'
$vsix = Join-Path $artifacts "extension space & apostrophe'.vsix"
[IO.File]::WriteAllText($vsix, 'fixture')
$source = @'
using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
public class CliFixture {
    public static int Main(string[] args) {
        if (args.Length == 1 && args[0] == "--hold-pipes") {
            Thread.Sleep(10000);
            return 0;
        }
        File.WriteAllLines(Environment.GetEnvironmentVariable("SOLX_TEST_ARGUMENTS"), args);
        Console.WriteLine("fixture output");
        Console.Error.WriteLine("fixture error");
        if (Environment.GetEnvironmentVariable("ELECTRON_RUN_AS_NODE") != "1") return 99;
        string pipeChild = Environment.GetEnvironmentVariable("SOLX_TEST_PIPE_CHILD");
        if (!String.IsNullOrEmpty(pipeChild)) {
            var info = new ProcessStartInfo(Process.GetCurrentProcess().MainModule.FileName, "--hold-pipes");
            info.UseShellExecute = false;
            info.CreateNoWindow = true;
            using (var child = Process.Start(info)) {
                File.WriteAllLines(pipeChild, new[] { child.Id.ToString(), child.StartTime.ToUniversalTime().Ticks.ToString() });
            }
        }
        Thread.Sleep(int.Parse(Environment.GetEnvironmentVariable("SOLX_TEST_SLEEP") ?? "0"));
        return int.Parse(Environment.GetEnvironmentVariable("SOLX_TEST_EXIT") ?? "0");
    }
}
'@
Add-Type -TypeDefinition $source -OutputAssembly (Join-Path $fake 'Code.exe') -OutputType ConsoleApplication
$saved = @{}
foreach ($name in @('SOLX_TEST_ARGUMENTS','SOLX_TEST_SLEEP','SOLX_TEST_EXIT','SOLX_TEST_PIPE_CHILD')) { $saved[$name] = [Environment]::GetEnvironmentVariable($name) }
$env:SOLX_TEST_ARGUMENTS = $record
$env:SOLX_TEST_SLEEP = '0'
$env:SOLX_TEST_EXIT = '0'
$env:SOLX_TEST_PIPE_CHILD = ''
$script:passed = @()
function Check([string] $Name, [scriptblock] $Action) {
    & $Action
    $script:passed += $Name
    Write-Output "PASS $Name"
}
function Equal($Actual, $Expected) { if ($Actual -cne $Expected) { throw "Expected '$Expected', received '$Actual'." } }
function Run-Install { Install-SolxVSCode -Selected $true -CodeRoot $fake -Vsix $vsix -LogPath $log @args }
try {
    Check 'version-specific VS Code layout follows the official CLI wrapper' {
        $modern = Join-Path $artifacts 'modern Code'
        New-Item -ItemType Directory -Force -Path (Join-Path $modern 'bin'),(Join-Path $modern 'abc123/resources/app/out') | Out-Null
        Copy-Item -LiteralPath (Join-Path $fake 'Code.exe') -Destination (Join-Path $modern 'Code.exe')
        [IO.File]::WriteAllText((Join-Path $modern 'bin/code.cmd'), '"%~dp0..\Code.exe" "%~dp0..\abc123\resources\app\out\cli.js" %*')
        [IO.File]::WriteAllText((Join-Path $modern 'abc123/resources/app/out/cli.js'), '// CLI fixture')
        [IO.File]::WriteAllText((Join-Path $modern 'abc123/resources/app/package.json'), '{"version":"1.143.0"}')
        Equal (Get-SolxVSCode -Candidates @($modern)) $modern
        $layout = Get-SolxVSCodeLayout $modern
        Equal $layout.EnableElectronFlag $false
        Equal $layout.Cli (Join-Path $modern 'abc123\resources\app\out\cli.js')
        [IO.File]::WriteAllText((Join-Path $modern 'bin/code.cmd'), '"%~dp0..\Code.exe" "%~dp0..\..\resources\app\out\cli.js" %*')
        Equal (Get-SolxVSCode -Candidates @($modern)) ''
    }
    Check 'supported CLI discovery including spaces' { Equal (Get-SolxVSCode -Candidates @($fake)) $fake }
    Check 'skip never launches an extension installer' {
        Equal (Install-SolxVSCode -Selected $false -CodeRoot $fake -Vsix $vsix).Status 'Skipped'
        Equal (Test-Path $record) $false
    }
    Check 'missing VS Code is nonfatal and gives manual instructions' {
        $result = Install-SolxVSCode -Selected $true -CodeRoot (Join-Path $artifacts 'missing') -Vsix $vsix
        Equal $result.Status 'Unavailable'
        if ($result.Message -notmatch 'Solx remains installed.*Install from VSIX') { throw 'Manual recovery instructions missing' }
    }
    Check 'missing CLI is detected' {
        $cli = Join-Path $fake 'resources/app/out/cli.js'
        Move-Item -LiteralPath $cli -Destination ($cli + '.test-backup')
        try { Equal (Get-SolxVSCode -Candidates @($fake)) '' }
        finally { Move-Item -LiteralPath ($cli + '.test-backup') -Destination $cli }
    }
    Check 'unsupported old VS Code is rejected' {
        $manifest = Join-Path $fake 'resources/app/package.json'
        [IO.File]::WriteAllText($manifest, '{"version":"1.84.0"}')
        try { Equal (Get-SolxVSCode -Candidates @($fake)) '' }
        finally { [IO.File]::WriteAllText($manifest, '{"version":"1.85.2"}') }
    }
    Check 'quoted arguments arrive unchanged without a shell' {
        $extensions = Join-Path $artifacts 'extensions & spaces'
        $userData = Join-Path $artifacts 'user-data'
        Equal (Run-Install -ExtensionsDirectory $extensions -UserDataDirectory $userData).Status 'Installed'
        $arguments = [IO.File]::ReadAllLines($record)
        Equal $arguments.Count 9
        Equal $arguments[0] (Join-Path $fake 'resources/app/out/cli.js')
        Equal $arguments[1] '--ms-enable-electron-run-as-node'
        Equal $arguments[3] $vsix
        Equal $arguments[6] $extensions
        Equal $arguments[8] $userData
        if ((Get-Content $log -Raw) -notmatch 'fixture output[\s\S]*fixture error') { throw 'Output not captured' }
    }
    Check 'nonzero CLI exit preserves a useful error and log' {
        $env:SOLX_TEST_EXIT = '17'
        try {
            $result = Run-Install
            Equal $result.Status 'Failed'
            if ($result.Message -notmatch 'code 17') { throw 'Exit code missing' }
        } finally { $env:SOLX_TEST_EXIT = '0' }
    }
    Check 'missing bundled package is handled' {
        Equal (Install-SolxVSCode -Selected $true -CodeRoot $fake -Vsix (Join-Path $artifacts 'missing.vsix')).Status 'Failed'
    }
    Check 'launch failures do not escape the helper' {
        $exe = Join-Path $fake 'Code.exe'
        Move-Item -LiteralPath $exe -Destination ($exe + '.test-backup')
        try {
            [IO.File]::WriteAllText($exe, 'invalid executable fixture')
            Equal (Run-Install).Status 'Failed'
        } finally {
            # Exact test fixture only; no recursive operations or installed files.
            Remove-Item -LiteralPath $exe
            Move-Item -LiteralPath ($exe + '.test-backup') -Destination $exe
        }
    }
    Check 'timeout terminates only the owned CLI process' {
        $env:SOLX_TEST_SLEEP = '10000'
        try {
            $result = Run-Install -TimeoutSeconds 1
            Equal $result.Status 'Failed'
            if ($result.Message -notmatch 'timed out') { throw 'Timeout not reported' }
        } finally { $env:SOLX_TEST_SLEEP = '0' }
    }
    Check 'inherited output pipes cannot outlive the CLI deadline' {
        $childRecord = Join-Path $artifacts 'pipe-child.txt'
        $env:SOLX_TEST_PIPE_CHILD = $childRecord
        $timer = [Diagnostics.Stopwatch]::StartNew()
        try {
            $result = Run-Install -TimeoutSeconds 1
            Equal $result.Status 'Failed'
            if ($result.Message -notmatch 'timed out waiting for CLI output') { throw 'Output-drain timeout not reported' }
            if ($timer.ElapsedMilliseconds -ge 5000) { throw 'Output-drain deadline was not enforced' }
        } finally {
            $env:SOLX_TEST_PIPE_CHILD = ''
            # This fixture deliberately outlives its parent. Verify both PID and
            # start time before terminating it; never select an editor by name.
            if (Test-Path -LiteralPath $childRecord) {
                $identity = [IO.File]::ReadAllLines($childRecord)
                $owned = $null
                try { $owned = [Diagnostics.Process]::GetProcessById([int]$identity[0]) }
                catch [ArgumentException] { }
                if ($owned) {
                    try {
                        if (-not $owned.HasExited -and $owned.StartTime.ToUniversalTime().Ticks -eq [long]$identity[1]) {
                            $owned.Kill()
                            if (-not $owned.WaitForExit(5000)) { throw 'Fixture child failed to exit' }
                        }
                    } finally { $owned.Dispose() }
                }
            }
        }
    }
    Check 'Windows argument quoting round trips quotes and trailing slashes' {
        $values = @('', 'plain', 'spaces and & shell % characters', 'a"quoted"value', 'C:\trailing space\')
        $info = New-Object Diagnostics.ProcessStartInfo
        $info.FileName = Join-Path $fake 'Code.exe'
        $info.Arguments = ($values | ForEach-Object { ConvertTo-WindowsArgument $_ }) -join ' '
        $info.UseShellExecute = $false; $info.CreateNoWindow = $true
        $info.EnvironmentVariables['ELECTRON_RUN_AS_NODE'] = '1'
        $child = [Diagnostics.Process]::Start($info)
        try { $child.WaitForExit(); Equal $child.ExitCode 0 } finally { $child.Dispose() }
        Equal ([IO.File]::ReadAllLines($record) -join '|') ($values -join '|')
    }
    @{ passed = $passed; success = $true } | ConvertTo-Json | Set-Content (Join-Path $artifacts 'results.json')
    Write-Output "$($passed.Count) installer helper checks passed. Artifacts: $artifacts"
} finally {
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name]) }
}
