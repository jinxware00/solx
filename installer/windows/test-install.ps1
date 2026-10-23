param([Parameter(Mandatory=$true)][string] $CodeRoot)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'VSCode.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'Installer.psm1') -Force
if (-not (Get-SolxVSCode -Candidates @($CodeRoot))) { throw 'Pass a supported desktop VS Code root for isolated installation.' }
& (Join-Path $PSScriptRoot 'build.ps1')
$version = [regex]::Match((Get-Content (Join-Path $PSScriptRoot '../../Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$artifacts = Join-Path $PSScriptRoot ('.artifacts/terminal-' + [guid]::NewGuid().ToString('N'))
$bundle = Join-Path $artifacts 'bundle'
Expand-Archive -LiteralPath (Join-Path $PSScriptRoot "../../dist/solx-$version-windows-x64.zip") -DestinationPath $bundle
$manifest = Get-SolxBundle $bundle
$originalPath = Get-SolxUserPath
$installedExe = Join-Path $env:LOCALAPPDATA 'Solx/bin/solx.exe'
$originalExeHash = if (Test-Path $installedExe) { (Get-FileHash $installedExe).Hash } else { '' }
$badCode = Join-Path $artifacts 'failing Code'
New-Item -ItemType Directory -Force -Path (Join-Path $badCode 'resources/app/out') | Out-Null
[IO.File]::WriteAllText((Join-Path $badCode 'resources/app/package.json'), '{"version":"1.85.2"}')
[IO.File]::WriteAllText((Join-Path $badCode 'resources/app/out/cli.js'), '// fixture')
Add-Type -TypeDefinition 'public class FailingCli { public static int Main(string[] args) { System.Console.Error.WriteLine("expected installation failure"); return 42; } }' -OutputAssembly (Join-Path $badCode 'Code.exe') -OutputType ConsoleApplication
$missingCli = Join-Path $artifacts 'missing CLI'
New-Item -ItemType Directory -Force -Path (Join-Path $missingCli 'resources/app') | Out-Null
Copy-Item (Join-Path $badCode 'Code.exe') (Join-Path $missingCli 'Code.exe')
Copy-Item (Join-Path $badCode 'resources/app/package.json') (Join-Path $missingCli 'resources/app/package.json')
function Run-Terminal([string[]] $Arguments, [string] $InputText = '') {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
    $info.Arguments = (@('-NoProfile','-ExecutionPolicy','Bypass','-File') + $Arguments | ForEach-Object { ConvertTo-WindowsArgument $_ }) -join ' '
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true; $info.RedirectStandardInput = $true
    $process = [Diagnostics.Process]::Start($info)
    try {
        $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
        $process.StandardInput.Write($InputText); $process.StandardInput.Close()
        if (-not $process.WaitForExit(180000)) { $process.Kill(); throw 'Terminal installer timed out.' }
        return @{ Code = $process.ExitCode; Output = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult() }
    } finally { $process.Dispose() }
}
$cases = @(
    @{ Name = 'skipped'; Choice = 'No'; Root = $CodeRoot; Expected = 'skipped' },
    @{ Name = 'selected'; Choice = 'Yes'; Root = $CodeRoot; Expected = 'installed for VS Code' },
    @{ Name = 'missing-code'; Choice = 'Yes'; Root = (Join-Path $artifacts 'absent'); Expected = 'were not found' },
    @{ Name = 'missing-cli'; Choice = 'Yes'; Root = $missingCli; Expected = 'were not found' },
    @{ Name = 'failed-cli'; Choice = 'Yes'; Root = $badCode; Expected = 'code 42' }
)
$results = @()
foreach ($case in $cases) {
    $destination = Join-Path $artifacts ($case.Name + ' space & path')
    $result = Run-Terminal @((Join-Path $bundle 'install.ps1'), '-Yes','-NoColor','-AddToPath','No',
        '-InstallDirectory',$destination,'-VSCode',$case.Choice,'-CodeRoot',$case.Root,
        '-ExtensionsDirectory',(Join-Path $destination 'test-extensions'),'-UserDataDirectory',(Join-Path $destination 'test-user-data'))
    [IO.File]::WriteAllText((Join-Path $artifacts ($case.Name + '.log')), $result.Output)
    if ($result.Code -ne 0 -or $result.Output -notmatch $case.Expected -or $result.Output -notmatch '\[5/5\]') { throw "Installer $($case.Name) failed: $($result.Output)" }
    if ($result.Output.Contains([char]27)) { throw 'Plain-text mode emitted terminal escape codes.' }
    $exe = Join-Path $destination 'bin/solx.exe'
    $output = & $exe -e 'print(42)'
    if ($LASTEXITCODE -ne 0 -or $output -ne '42') { throw 'Installed interpreter does not work.' }
    if ((Get-FileHash $exe).Hash -ne $manifest.files.'bin/solx.exe') { throw 'Interpreter payload mismatch' }
    if ($case.Name -eq 'selected') {
        $package = Get-Content (Join-Path $destination "test-extensions/jinxware00.solx-language-support-$($manifest.extensionVersion)/package.json") -Raw | ConvertFrom-Json
        if ($package.version -ne $manifest.extensionVersion) { throw 'Installed extension version mismatch' }
    } elseif (Test-Path (Join-Path $destination 'test-extensions')) { throw 'Unexpected extension installation' }
    $results += @{ name = $case.Name; success = $true }
    Write-Output "PASS terminal installer: $($case.Name)"
}
$cancelTarget = Join-Path $artifacts 'cancelled'
$cancel = Run-Terminal @((Join-Path $bundle 'install.ps1'),'-InstallDirectory',$cancelTarget,'-AddToPath','No','-VSCode','No','-NoColor') "n`n"
if ($cancel.Code -ne 130 -or (Test-Path $cancelTarget)) { throw "Cancellation failed: $($cancel.Output)" }
$results += @{ name = 'interactive cancellation'; success = $true }
Write-Output 'PASS terminal installer: interactive cancellation'
$promptTarget = Join-Path $artifacts 'interactive'
$prompt = Run-Terminal @((Join-Path $bundle 'install.ps1'),'-InstallDirectory',$promptTarget,'-AddToPath','No','-NoColor','-CodeRoot',$CodeRoot) "n`ny`n"
if ($prompt.Code -ne 0 -or $prompt.Output -notmatch 'Install Solx for Visual Studio Code\?' -or $prompt.Output -notmatch '\[Skipped\]') { throw "Interactive prompts failed: $($prompt.Output)" }
[IO.File]::WriteAllText((Join-Path $artifacts 'interactive.log'), $prompt.Output)
$results += @{ name = 'interactive skip and confirmation'; success = $true }
Write-Output 'PASS terminal installer: interactive skip and confirmation'
$userFile = Join-Path $promptTarget 'my-program.solx'
[IO.File]::WriteAllText($userFile, 'print(42)')
[IO.File]::AppendAllText((Join-Path $promptTarget 'INSTALLING.md'), 'user edits')
$uninstall = Run-Terminal @((Join-Path $promptTarget 'uninstall.ps1'), '-Yes')
if ($uninstall.Code -ne 0 -or (Test-Path (Join-Path $promptTarget 'bin/solx.exe')) -or
    -not (Test-Path $userFile) -or -not (Test-Path (Join-Path $promptTarget 'INSTALLING.md'))) { throw "Safe uninstall failed: $($uninstall.Output)" }
$results += @{ name = 'uninstall preserves user and modified files'; success = $true }
Write-Output 'PASS terminal installer: safe uninstall'
if ((Get-SolxUserPath) -cne $originalPath) { throw 'Host user PATH changed.' }
$finalExeHash = if (Test-Path $installedExe) { (Get-FileHash $installedExe).Hash } else { '' }
if ($originalExeHash -ne $finalExeHash) { throw 'Host Solx executable changed.' }
$results | ConvertTo-Json | Set-Content (Join-Path $artifacts 'results.json')
Write-Output "All eight terminal installer paths passed; host PATH and interpreter preserved. Artifacts: $artifacts"
