# Run one Windows validation command with limits inherited by its process tree.
# This is a resource guard, not a security boundary or protection from driver bugs.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string]$Executable,
    [string]$ArgumentsJson = '[]',
    [string]$WorkingDirectory = (Get-Location).Path,
    [ValidateRange(1, 2048)][int]$MemoryMiB = 1024,
    [ValidateRange(1, 3600)][int]$TimeoutSeconds = 300,
    [ValidateRange(1, 128)][int]$MaxProcesses = 32,
    [ValidateRange(1, 128)][int]$MaxLogMiB = 8,
    [string]$LogDirectory
)

$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'guarded-run.ps1 requires Windows Job Objects.'
}
if (-not $ArgumentsJson.TrimStart().StartsWith('[')) {
    throw 'ArgumentsJson must be a JSON array of strings.'
}
$commandArguments = ConvertFrom-Json -InputObject $ArgumentsJson
if ($commandArguments -isnot [Array]) {
    throw 'ArgumentsJson must be a JSON array of strings.'
}
foreach ($argument in $commandArguments) {
    if ($argument -isnot [string] -or $argument.IndexOf([char]0) -ge 0) {
        throw 'Each ArgumentsJson item must be a String without NUL characters.'
    }
}
$application = Get-Command -Name $Executable -CommandType Application -ErrorAction Stop | Select-Object -First 1
$applicationPath = [IO.Path]::GetFullPath($application.Source)
if ([IO.Path]::GetExtension($applicationPath) -ine '.exe') {
    throw 'Executable must resolve to an .exe; invoke script interpreters explicitly.'
}
$commandDirectory = (Resolve-Path -LiteralPath $WorkingDirectory -ErrorAction Stop).ProviderPath
if (-not (Test-Path -LiteralPath $commandDirectory -PathType Container)) {
    throw 'WorkingDirectory must be a filesystem directory.'
}
if (-not $LogDirectory) {
    $runName = '{0}-{1}' -f [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss'), [Guid]::NewGuid().ToString('N').Substring(0, 8)
    $LogDirectory = Join-Path (Join-Path (Split-Path $PSScriptRoot -Parent) 'target/validation') $runName
}
$LogDirectory = [IO.Path]::GetFullPath($LogDirectory)
if (Test-Path -LiteralPath $LogDirectory) {
    throw 'LogDirectory already exists. Use a new directory to preserve earlier receipts.'
}
[IO.Directory]::CreateDirectory($LogDirectory) | Out-Null
$receiptPath = Join-Path $LogDirectory 'result.json'
$receipt = [ordered]@{
    schemaVersion = 1
    status = 'starting'
    startedAt = [DateTime]::UtcNow.ToString('o')
    finishedAt = $null
    executable = $applicationPath
    arguments = @($commandArguments)
    workingDirectory = $commandDirectory
    limits = [ordered]@{ memoryMiB = $MemoryMiB; timeoutSeconds = $TimeoutSeconds; maxProcesses = $MaxProcesses; maxLogMiBPerStream = $MaxLogMiB }
    result = $null
    error = $null
}
[IO.File]::WriteAllText($receiptPath, ($receipt | ConvertTo-Json -Depth 8))

# Use only whitelisted pipe/NUL handles in the child. The executable is created
# suspended, assigned to the bounded job, and then resumed. Assignment failures
# terminate the suspended child; they never retry without the job's limits.
$guardSource = @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.Win32.SafeHandles;

namespace SolxValidation {
    public sealed class GuardResult {
        public int ExitCode;
        public uint? CommandExitCode;
        public string Status;
        public long ElapsedMilliseconds;
        public ulong PeakJobMemoryBytes;
        public ulong ConfiguredJobMemoryBytes;
        public uint TotalProcesses;
        public uint RemainingProcessesAtCommandExit;
        public bool StandardOutputTruncated;
        public bool StandardErrorTruncated;
    }

    public static class Guard {
        const uint CREATE_SUSPENDED = 0x00000004;
        const uint EXTENDED_STARTUPINFO_PRESENT = 0x00080000;
        const uint CREATE_NO_WINDOW = 0x08000000;
        const uint JOB_OBJECT_LIMIT_ACTIVE_PROCESS = 0x00000008;
        const uint JOB_OBJECT_LIMIT_JOB_MEMORY = 0x00000200;
        const uint JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION = 0x00000400;
        const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
        const uint WAIT_OBJECT_0 = 0;
        static readonly IntPtr INVALID_HANDLE_VALUE = new IntPtr(-1);

        [StructLayout(LayoutKind.Sequential)] struct SecurityAttributes {
            public int Length; public IntPtr SecurityDescriptor; public int InheritHandle;
        }
        [StructLayout(LayoutKind.Sequential)] struct StartupInfo {
            public int Size; public IntPtr Reserved; public IntPtr Desktop; public IntPtr Title;
            public uint X, Y, XSize, YSize, XCountChars, YCountChars, FillAttribute, Flags;
            public ushort ShowWindow, ReservedBytes;
            public IntPtr ReservedData, StandardInput, StandardOutput, StandardError;
        }
        [StructLayout(LayoutKind.Sequential)] struct StartupInfoEx {
            public StartupInfo StartupInfo; public IntPtr AttributeList;
        }
        [StructLayout(LayoutKind.Sequential)] struct ProcessInformation {
            public IntPtr Process, Thread; public uint ProcessId, ThreadId;
        }
        [StructLayout(LayoutKind.Sequential)] struct BasicLimitInformation {
            public long PerProcessUserTimeLimit, PerJobUserTimeLimit;
            public uint LimitFlags;
            public UIntPtr MinimumWorkingSetSize, MaximumWorkingSetSize;
            public uint ActiveProcessLimit;
            public UIntPtr Affinity;
            public uint PriorityClass, SchedulingClass;
        }
        [StructLayout(LayoutKind.Sequential)] struct IoCounters {
            public ulong ReadOperationCount, WriteOperationCount, OtherOperationCount;
            public ulong ReadTransferCount, WriteTransferCount, OtherTransferCount;
        }
        [StructLayout(LayoutKind.Sequential)] struct ExtendedLimitInformation {
            public BasicLimitInformation BasicLimitInformation;
            public IoCounters IoInfo;
            public UIntPtr ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed;
        }
        [StructLayout(LayoutKind.Sequential)] struct BasicAccountingInformation {
            public long TotalUserTime, TotalKernelTime, ThisPeriodTotalUserTime, ThisPeriodTotalKernelTime;
            public uint TotalPageFaultCount, TotalProcesses, ActiveProcesses, TotalTerminatedProcesses;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern IntPtr CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool SetInformationJobObject(IntPtr job, int infoClass, ref ExtendedLimitInformation info, uint length);
        [DllImport("kernel32.dll", EntryPoint = "QueryInformationJobObject", SetLastError = true)]
        static extern bool QueryLimits(IntPtr job, int infoClass, out ExtendedLimitInformation info, uint length, IntPtr returned);
        [DllImport("kernel32.dll", EntryPoint = "QueryInformationJobObject", SetLastError = true)]
        static extern bool QueryAccounting(IntPtr job, int infoClass, out BasicAccountingInformation info, uint length, IntPtr returned);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool TerminateJobObject(IntPtr job, uint code);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool TerminateProcess(IntPtr process, uint code);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool CloseHandle(IntPtr handle);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool CreatePipe(out IntPtr read, out IntPtr write, ref SecurityAttributes attributes, uint size);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern IntPtr CreateFile(string name, uint access, uint share, ref SecurityAttributes attributes, uint creation, uint flags, IntPtr template);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, int flags, ref IntPtr size);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, IntPtr attribute, IntPtr value, IntPtr size, IntPtr previous, IntPtr returned);
        [DllImport("kernel32.dll")]
        static extern void DeleteProcThreadAttributeList(IntPtr list);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern bool CreateProcess(string application, StringBuilder commandLine, IntPtr processAttributes, IntPtr threadAttributes,
            bool inheritHandles, uint flags, IntPtr environment, string directory, ref StartupInfoEx startup, out ProcessInformation process);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern uint ResumeThread(IntPtr thread);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool GetExitCodeProcess(IntPtr process, out uint code);

        static void Check(bool success, string operation) {
            if (!success) throw new Win32Exception(Marshal.GetLastWin32Error(), operation);
        }
        static void Close(ref IntPtr handle) {
            if (handle != IntPtr.Zero && handle != INVALID_HANDLE_VALUE) CloseHandle(handle);
            handle = IntPtr.Zero;
        }
        static string Quote(string value) {
            StringBuilder result = new StringBuilder("\"");
            int slashes = 0;
            foreach (char c in value) {
                if (c == '\\') { slashes++; continue; }
                if (c == '"') result.Append('\\', slashes * 2 + 1);
                else result.Append('\\', slashes);
                result.Append(c); slashes = 0;
            }
            return result.Append('\\', slashes * 2).Append('"').ToString();
        }
        static Task Capture(IntPtr handle, string path, long maximum, Action exceeded) {
            return Task.Factory.StartNew(delegate {
                using (SafeFileHandle owned = new SafeFileHandle(handle, true))
                using (FileStream input = new FileStream(owned, FileAccess.Read, 8192, false))
                using (FileStream output = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.Read)) {
                    byte[] buffer = new byte[8192];
                    long total = 0;
                    int count;
                    while ((count = input.Read(buffer, 0, buffer.Length)) != 0) {
                        int keep = (int)Math.Min(count, maximum - total);
                        if (keep > 0) { output.Write(buffer, 0, keep); total += keep; }
                        if (keep != count) exceeded();
                    }
                }
            }, CancellationToken.None, TaskCreationOptions.LongRunning, TaskScheduler.Default);
        }

        public static GuardResult Run(string application, string[] arguments, string directory, ulong memoryBytes,
            int timeoutSeconds, uint maximumProcesses, long maximumLogBytes, string stdoutPath, string stderrPath) {
            IntPtr job = IntPtr.Zero, stdoutRead = IntPtr.Zero, stdoutWrite = IntPtr.Zero;
            IntPtr stderrRead = IntPtr.Zero, stderrWrite = IntPtr.Zero, stdin = IntPtr.Zero;
            IntPtr attributes = IntPtr.Zero, handles = IntPtr.Zero;
            bool attributesInitialized = false, assigned = false;
            ProcessInformation process = new ProcessInformation();
            Task stdout = null, stderr = null;
            int stdoutExceeded = 0, stderrExceeded = 0;
            GuardResult result = new GuardResult { Status = "failed", ExitCode = 1 };
            Stopwatch elapsed = Stopwatch.StartNew();
            try {
                job = CreateJobObject(IntPtr.Zero, null);
                Check(job != IntPtr.Zero, "CreateJobObject");
                ExtendedLimitInformation limits = new ExtendedLimitInformation();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_JOB_MEMORY | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                    | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                limits.BasicLimitInformation.ActiveProcessLimit = maximumProcesses;
                limits.JobMemoryLimit = new UIntPtr(memoryBytes);
                Check(SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf(typeof(ExtendedLimitInformation))), "SetInformationJobObject");

                SecurityAttributes inherit = new SecurityAttributes { Length = Marshal.SizeOf(typeof(SecurityAttributes)), InheritHandle = 1 };
                Check(CreatePipe(out stdoutRead, out stdoutWrite, ref inherit, 0), "CreatePipe(stdout)");
                Check(SetHandleInformation(stdoutRead, 1, 0), "Protect stdout reader");
                Check(CreatePipe(out stderrRead, out stderrWrite, ref inherit, 0), "CreatePipe(stderr)");
                Check(SetHandleInformation(stderrRead, 1, 0), "Protect stderr reader");
                stdin = CreateFile("NUL", 0x80000000, 3, ref inherit, 3, 0x80, IntPtr.Zero);
                Check(stdin != INVALID_HANDLE_VALUE, "Open NUL input");

                IntPtr attributeBytes = IntPtr.Zero;
                InitializeProcThreadAttributeList(IntPtr.Zero, 1, 0, ref attributeBytes);
                attributes = Marshal.AllocHGlobal(attributeBytes);
                Check(InitializeProcThreadAttributeList(attributes, 1, 0, ref attributeBytes), "Initialize handle allowlist");
                attributesInitialized = true;
                handles = Marshal.AllocHGlobal(IntPtr.Size * 3);
                Marshal.WriteIntPtr(handles, 0, stdin);
                Marshal.WriteIntPtr(handles, IntPtr.Size, stdoutWrite);
                Marshal.WriteIntPtr(handles, IntPtr.Size * 2, stderrWrite);
                Check(UpdateProcThreadAttribute(attributes, 0, new IntPtr(0x20002), handles, new IntPtr(IntPtr.Size * 3), IntPtr.Zero, IntPtr.Zero), "Set child handle allowlist");
                StartupInfoEx startup = new StartupInfoEx();
                startup.StartupInfo.Size = Marshal.SizeOf(typeof(StartupInfoEx));
                startup.StartupInfo.Flags = 0x101;
                startup.StartupInfo.StandardInput = stdin;
                startup.StartupInfo.StandardOutput = stdoutWrite;
                startup.StartupInfo.StandardError = stderrWrite;
                startup.AttributeList = attributes;
                StringBuilder command = new StringBuilder(Quote(application));
                foreach (string argument in arguments) command.Append(' ').Append(Quote(argument));
                if (command.Length > 32766) throw new ArgumentException("Windows command line exceeds 32766 characters.");
                Check(CreateProcess(application, command, IntPtr.Zero, IntPtr.Zero, true,
                    CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
                    IntPtr.Zero, directory, ref startup, out process), "CreateProcess suspended");
                Close(ref stdin); Close(ref stdoutWrite); Close(ref stderrWrite);
                Check(AssignProcessToJobObject(job, process.Process), "AssignProcessToJobObject; refusing unguarded execution");
                assigned = true;
                stdout = Capture(stdoutRead, stdoutPath, maximumLogBytes, delegate { Interlocked.Exchange(ref stdoutExceeded, 1); });
                stdoutRead = IntPtr.Zero;
                stderr = Capture(stderrRead, stderrPath, maximumLogBytes, delegate { Interlocked.Exchange(ref stderrExceeded, 1); });
                stderrRead = IntPtr.Zero;
                Check(ResumeThread(process.Thread) != uint.MaxValue, "ResumeThread");
                Close(ref process.Thread);
                while (true) {
                    BasicAccountingInformation accounting;
                    Check(QueryAccounting(job, 1, out accounting, (uint)Marshal.SizeOf(typeof(BasicAccountingInformation)), IntPtr.Zero), "Query job activity");
                    result.TotalProcesses = accounting.TotalProcesses;
                    uint wait = WaitForSingleObject(process.Process, 0);
                    Check(wait != uint.MaxValue, "WaitForSingleObject");
                    if (wait == WAIT_OBJECT_0) {
                        uint exit;
                        Check(GetExitCodeProcess(process.Process, out exit), "GetExitCodeProcess");
                        result.CommandExitCode = exit;
                    }
                    if (Volatile.Read(ref stdoutExceeded) != 0 || Volatile.Read(ref stderrExceeded) != 0) {
                        result.Status = "output-limit"; result.ExitCode = 125; break;
                    }
                    if (stdout.IsFaulted || stderr.IsFaulted) throw new IOException("A bounded log reader failed.",
                        (stdout.Exception ?? stderr.Exception).GetBaseException());
                    if (result.CommandExitCode.HasValue) {
                        // The selected command owns its result. Reap services and
                        // detached children when it ends, rather than waiting for
                        // compiler/helper lifetime processes until the wall timeout.
                        result.RemainingProcessesAtCommandExit = accounting.ActiveProcesses;
                        result.ExitCode = unchecked((int)result.CommandExitCode.Value);
                        result.Status = result.ExitCode == 0 ? "passed" : "command-failed";
                        break;
                    }
                    if (elapsed.Elapsed.TotalSeconds >= timeoutSeconds) {
                        result.Status = "timeout"; result.ExitCode = 124; break;
                    }
                    Thread.Sleep(25);
                }
                Check(TerminateJobObject(job, (uint)Math.Max(1, result.ExitCode)), "Terminate remaining validation processes");
                if (!Task.WaitAll(new[] { stdout, stderr }, 5000)) throw new TimeoutException("Validation log pipes did not close after job termination.");
                result.StandardOutputTruncated = Volatile.Read(ref stdoutExceeded) != 0;
                result.StandardErrorTruncated = Volatile.Read(ref stderrExceeded) != 0;
                if (result.StandardOutputTruncated || result.StandardErrorTruncated) {
                    result.Status = "output-limit"; result.ExitCode = 125;
                }
                ExtendedLimitInformation finalLimits;
                Check(QueryLimits(job, 9, out finalLimits, (uint)Marshal.SizeOf(typeof(ExtendedLimitInformation)), IntPtr.Zero), "Query peak committed memory");
                result.PeakJobMemoryBytes = finalLimits.PeakJobMemoryUsed.ToUInt64();
                result.ConfiguredJobMemoryBytes = finalLimits.JobMemoryLimit.ToUInt64();
                result.ElapsedMilliseconds = elapsed.ElapsedMilliseconds;
                return result;
            } finally {
                if (process.Process != IntPtr.Zero && !assigned) TerminateProcess(process.Process, 1);
                // Closing the job is the final cleanup backstop, including on errors.
                Close(ref job);
                Close(ref process.Thread); Close(ref process.Process);
                Close(ref stdin); Close(ref stdoutWrite); Close(ref stderrWrite);
                Close(ref stdoutRead); Close(ref stderrRead);
                if (attributesInitialized) DeleteProcThreadAttributeList(attributes);
                if (attributes != IntPtr.Zero) Marshal.FreeHGlobal(attributes);
                if (handles != IntPtr.Zero) Marshal.FreeHGlobal(handles);
            }
        }
    }
}
'@

$wrapperExit = 1
try {
    if (-not ('SolxValidation.Guard' -as [type])) {
        Add-Type -TypeDefinition $guardSource -Language CSharp
    }
    $run = [SolxValidation.Guard]::Run($applicationPath, [string[]]$commandArguments, $commandDirectory,
        [uint64]$MemoryMiB * 1MB, $TimeoutSeconds, [uint32]$MaxProcesses, [long]$MaxLogMiB * 1MB,
        (Join-Path $LogDirectory 'stdout.log'), (Join-Path $LogDirectory 'stderr.log'))
    $receipt.status = $run.Status
    $receipt.result = $run
    $wrapperExit = $run.ExitCode
} catch {
    $receipt.status = 'guard-failed'
    $receipt.error = $_.Exception.GetBaseException().Message
    Write-Warning $receipt.error
} finally {
    $receipt.finishedAt = [DateTime]::UtcNow.ToString('o')
    [IO.File]::WriteAllText($receiptPath, ($receipt | ConvertTo-Json -Depth 8))
    Write-Output ('Validation {0}; receipt: {1}' -f $receipt.status, $receiptPath)
}
exit $wrapperExit
