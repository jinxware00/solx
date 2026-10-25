# Small serial probes; each child tree has at most 128 MiB committed memory.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$guard = Join-Path $PSScriptRoot 'guarded-run.ps1'
$runRoot = Join-Path $root ('target/validation/guard-probes-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($runRoot) | Out-Null
$checks = 0

function Probe([string]$Name, [string[]]$CommandArguments, [int]$Timeout = 10, [int]$Memory = 128) {
    $directory = Join-Path $runRoot $Name
    & $guard -Executable node.exe -ArgumentsJson (ConvertTo-Json -InputObject $CommandArguments -Compress) `
        -WorkingDirectory $root -MemoryMiB $Memory -TimeoutSeconds $Timeout -MaxProcesses 8 -MaxLogMiB 1 -LogDirectory $directory | Out-Host
    $receipt = Get-Content -LiteralPath (Join-Path $directory 'result.json') -Raw | ConvertFrom-Json
    if ($receipt.status -eq 'guard-failed') { throw "$Name guard failed: $($receipt.error)" }
    return @{ Receipt = $receipt; Directory = $directory }
}
function Require([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}

$special = @('space value', 'quote"value', 'trailing\', '', '$literal`', 'caf' + [char]0xE9)
$probe = Probe 'quoting' (@('-e', 'process.stdout.write(JSON.stringify(process.argv.slice(1)))') + $special)
Require ($probe.Receipt.status -eq 'passed' -and $probe.Receipt.result.CommandExitCode -eq 0) 'Successful child was not recorded as successful.'
$actual = Get-Content -LiteralPath (Join-Path $probe.Directory 'stdout.log') -Raw -Encoding UTF8 | ConvertFrom-Json
Require (($actual | ConvertTo-Json -Compress) -ceq ($special | ConvertTo-Json -Compress)) 'Native argument quoting changed arguments.'

$probe = Probe 'failure' @('-e', 'process.exit(7)')
Require ($probe.Receipt.status -eq 'command-failed' -and $probe.Receipt.result.ExitCode -eq 7) 'Nonzero child status was lost.'

$probe = Probe 'timeout' @('-e', 'setInterval(() => {}, 1000)') 2
Require ($probe.Receipt.status -eq 'timeout' -and $probe.Receipt.result.ExitCode -eq 124) 'Infinite child did not time out.'

# Detach from libuv's own lifetime job so this exercises the outer job's cleanup.
$probe = Probe 'descendant' @('-e', 'const c=require("node:child_process").spawn(process.execPath,["-e","process.stdout.write(String(process.pid)); setInterval(()=>{},1000)"],{stdio:["ignore","pipe","inherit"],windowsHide:true,detached:true}); c.stdout.once("data",data=>{process.stdout.write(data); c.stdout.destroy(); c.unref();}); c.on("error",()=>process.exit(3));') 2
Require ($probe.Receipt.status -eq 'passed' -and $probe.Receipt.result.TotalProcesses -ge 2 -and $probe.Receipt.result.RemainingProcessesAtCommandExit -ge 1) 'Detached descendant was not observed at successful parent exit.'
$childPid = [int](Get-Content -LiteralPath (Join-Path $probe.Directory 'stdout.log') -Raw)
Require ($null -eq (Get-Process -Id $childPid -ErrorAction SilentlyContinue)) 'Owned descendant survived job termination.'

$probe = Probe 'output' @('-e', 'process.stdout.write("x".repeat(2*1024*1024))')
Require ($probe.Receipt.status -eq 'output-limit' -and $probe.Receipt.result.ExitCode -eq 125) 'Excessive log did not fail the command.'
Require ((Get-Item -LiteralPath (Join-Path $probe.Directory 'stdout.log')).Length -eq 1MB) 'Captured output exceeded its configured bound.'

$probe = Probe 'memory' @('-e', 'Buffer.alloc(192*1024*1024,1); process.stdout.write("unexpected allocation");') 10 96
Require ($probe.Receipt.status -eq 'command-failed') 'Allocation exceeding the job cap succeeded.'
Require ($probe.Receipt.result.ConfiguredJobMemoryBytes -eq 96MB) 'The OS-reported job memory cap differs from the requested cap.'
$allocationError = Get-Content -LiteralPath (Join-Path $probe.Directory 'stderr.log') -Raw
Require ($allocationError.Contains('ERR_MEMORY_ALLOCATION_FAILED')) 'Allocation probe failed for an unrelated reason.'

Write-Output "$checks guard checks passed; receipts: $runRoot"
