# Windows validation resource guard

`guarded-run.ps1` launches one explicitly supplied executable and its descendants
inside a Windows Job Object. Use this launcher serially: independent invocations
do not share a combined resource budget. It does not isolate kernel drivers or
prevent a kernel crash caused by a driver bug.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/guarded-run.ps1 `
  -Executable cargo.exe -ArgumentsJson '["test","--locked","-j","1","--","--test-threads=1"]' `
  -MemoryMiB 1024 -TimeoutSeconds 300
```

The default limits are 1 GiB aggregate committed job memory, 32 active processes,
five minutes elapsed time, and 8 MiB for each stdout/stderr log. Memory can be
raised explicitly to at most 2 GiB. Arguments are a JSON array of strings and
use native Windows argument quoting; there is no command-shell interpolation.
For a PowerShell script, explicitly launch `powershell.exe` with `-NoProfile`
and its arguments. stdin is NUL, so interactive prompts receive EOF.

The child starts suspended. The guard assigns its job and limits before any
child code runs; an assignment failure terminates the child instead of falling
back to unrestricted execution. Only stdout/stderr pipe handles and stdin NUL
are inherited. Every descendant stays in the job; breakaway is not enabled.
When the selected command exits, the guard records its exit code and terminates
any remaining descendants before returning. This reaps detached children and
compiler/helper lifetime services even after a successful command. The command
must await its own tests; a fire-and-forget launcher is not a validation command.
A timeout, log overflow or wrapper error also terminates remaining descendants.
Closing the job is a final process-tree cleanup backstop.

Receipts and bounded logs default to a new ignored directory under
`target/validation/`. `-LogDirectory` may select a different new directory;
existing directories are rejected to preserve earlier evidence. `result.json`
records the command, configured bounds, result and Windows' **peak job memory**
counter, which is not RSS or system-wide peak memory. On this host that counter
also increased for a rejected allocation beyond the cap; it must not be treated
as proof that the allocation succeeded. The queried configured job cap is also
recorded. Allocation failures caused by the
cap may appear as an ordinary nonzero command exit; the guard does not invent a
memory-limit diagnosis from that exit code. Output overflow has exit 125 and a
wall timeout has exit 124; successful completion has exit 0.

The job cap covers launched child processes, not the PowerShell guard process,
the OS kernel, antivirus, existing editor instances or other applications. The
wall clock begins before child creation but after compiling the small native
interop helper. This is a local validation aid, not a sandbox or a machine-wide
memory guarantee. No persistent OS, antivirus, installation or profile settings
are changed.

Validated with `scripts/test-guard.ps1`: 11 assertions over successful execution,
native quoting (including Unicode/empty arguments), nonzero exits, timeout,
detached descendant cleanup, bounded logs and rejected over-cap allocation.
The probes run serially with at most a 128 MiB job cap and ten-second timeout.
Node's own Windows lifetime job kills ordinary children when their parent exits;
the descendant probe explicitly detaches from that job to exercise this guard.
PowerShell 5 needs direct assignment of the parsed JSON array, since an array
subexpression otherwise nests it. Successful receipts require a confirmed zero
root exit code; remaining descendants are recorded and terminated before return.
