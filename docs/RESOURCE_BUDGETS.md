# Execution deadlines and source-size budgets

Solx combines cooperative execution limits with bounded runtime source reads.
The existing step, call, evaluation, import and syntax-depth limits remain in
effect. These controls do not make the interpreter a sandbox or establish a
total memory or process-time bound.

## Optional elapsed-time deadline

Use `--timeout-ms` to limit elapsed time for each public execution:

```sh
solx --timeout-ms 1000 program.solx
solx --timeout-ms 250 --unlimited-steps -e "while true do end"
```

The deadline is disabled by default. `--unlimited-steps` disables only the step
budget; an explicitly requested deadline and other limits still apply. A zero
timeout expires immediately, before parsing or executing even an empty source
buffer. Invalid or negative CLI values are rejected.

Rust callers configure `ExecutionLimits::max_duration: Option<Duration>`:

```rust
use std::time::Duration;
use solx::interpreter::Interpreter;
use solx::limits::ExecutionLimits;

fn main() -> Result<(), solx::runtime::RuntimeError> {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_duration: Some(Duration::from_secs(1)),
        ..ExecutionLimits::default()
    })?;
    interpreter.run_source("global answer = 42")?;
    Ok(())
}
```

Each `run_source`, `run_file` or `run_program` starts a fresh monotonic `Instant`
timer. Imports and function calls share that timer; they cannot reset it.
File reading and parsing performed inside a public run count toward elapsed
time. REPL waiting and initial buffer parsing occur before `run_program` and do
not consume its execution deadline.

Existing evaluation, loop, import, before/after parsing, and completion
cancellation checkpoints also check the deadline. At or beyond the configured
duration they return `RuntimeErrorKind::LimitExceeded(LimitKind::Deadline)`.
This is not catchable by Solx `try`/`catch`, just like other execution-budget
failures. Normal unwinding restores scopes and import state; completed effects
remain. Cancellation requests remain sticky until their host resets the token,
and an observed cancellation takes precedence over deadline expiration.

When the deadline is disabled, checkpoints perform no clock reads. The
implementation measures elapsed duration instead of adding an absolute deadline,
so even a host-provided `Duration::MAX` does not overflow a timestamp.

Checks are cooperative. A native operation, filesystem read, lexer/parser pass,
collector pass, or destruction already underway is allowed to finish before the
next checkpoint. The deadline does not preempt those operations, undo effects,
or guarantee that a process exits within the requested milliseconds.

## Runtime source-size limit

Runtime source is limited to **1,048,576 UTF-8 bytes (1 MiB) per buffer or file**
by default. This applies to `run_source`, entry files, every uncached imported
file, `-e`, and accumulated REPL input. It is a new restriction on runtime inputs
that previously could be arbitrarily large.

The size includes comments, whitespace and line endings. It counts encoded
bytes, not characters, and is not a cumulative import budget. Successfully
cached modules do not reread their files. A host-supplied `run_program` AST has
no source byte buffer; its existing AST and execution limits apply.

Raise the limit explicitly for larger programs:

```sh
solx --max-source-bytes 4194304 program.solx
```

Rust callers set `ExecutionLimits::max_source_bytes`. There is no fixed upper
ceiling for host-selected limits; zero permits only empty source. The default is
available as `solx::limits::DEFAULT_MAX_SOURCE_BYTES`. Existing struct literals
that specify every `ExecutionLimits` field need the new fields, or can use
`..ExecutionLimits::default()`. Exhaustive Rust matches on `LimitKind` also need
to handle the new `Deadline` and `SourceBytes` variants.

Entry and module files are opened and read through `Read::take` for at most the
configured size plus one byte. Oversized files are rejected before UTF-8 decoding
or execution; a changing file cannot bypass this check by reporting a smaller
metadata length. A host string is checked before lexing. Oversized input returns
`RuntimeErrorKind::LimitExceeded(LimitKind::SourceBytes)` and bypasses Solx error
handlers. File errors retain the source path. Ordinary file-read and invalid
UTF-8 errors remain language errors; failed imports can be repaired and retried.

The REPL caps each read by the remaining buffer allowance plus one byte. If a
physical line is too large, it drains the rest through the newline without
retaining the discarded bytes, clears the accumulated program, and reports the
limit. A truncated suffix is never executed as a new command. Subsequent input
can run normally. Draining can still wait for input or a newline; it is not an
I/O timeout.

The separate `solx check` command retains its existing fixed 1 MiB input limit
and JSON error contract. Execution flags do not change that command's limit.

Source limits constrain input reads, not output size, token/AST allocations,
runtime collections, string growth, or memory retained by modules. Raising the
source limit can increase parser memory and time. External hosts remain
responsible for process-level controls where those are needed.

## Validation

`tests/deadlines.rs` covers zero-time boundaries for every public entry point,
cancellation precedence, disabled and large budgets, and CLI argument handling.
Private control tests compare exact `Instant` values at the expiration boundary
and verify shared/reset budgets without sleeps. `tests/source_limits.rs` covers
UTF-8 byte boundaries, default and configurable limits, entry/import behavior,
noncatchable errors, retries, CLI execution and REPL recovery. Reader unit tests
use an endless synthetic source to verify that file reading stops after at most
the allowance plus one byte.

```sh
cargo test --locked --lib --bin solx --test deadlines --test source_limits --test execution_limits --test limits_cli --test modules
```
