# Execution deadlines, source limits, and formatting budgets

Solx combines cooperative execution limits with bounded runtime source reads
and bounded language formatting.
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

Evaluation, loop, import, before/after parsing, completion, collection callback,
and formatting cancellation checkpoints also check the deadline. At or beyond
the configured duration they return
`RuntimeErrorKind::LimitExceeded(LimitKind::Deadline)`.
This is not catchable by Solx `try`/`catch`, just like other execution-budget
failures. Normal unwinding restores scopes and import state; completed effects
remain. Cancellation requests remain sticky until their host resets the token,
and an observed cancellation takes precedence over deadline expiration.

When the deadline is disabled, checkpoints perform no clock reads. The
implementation measures elapsed duration instead of adding an absolute deadline,
so even a host-provided `Duration::MAX` does not overflow a timestamp.

Checks are cooperative. Collection callbacks and the formatting renderer check
between units of their work. A single native operation without such a
checkpoint, filesystem read, lexer/parser pass, output write, collector pass,
or destruction already underway can finish before the next check. The deadline
does not preempt those operations, undo effects, or guarantee that a process
exits within the requested milliseconds.

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
to handle `Deadline`, `SourceBytes`, `FormatBytes`, and `FormatWork`.

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

## Language formatting limits

`str(value)`, `left .. right`, and `print(...)` use a fallible renderer with two
independent limits:

| Setting | Default | Scope |
| --- | --- | --- |
| `--max-format-bytes N` / `ExecutionLimits::max_format_bytes` | 1,048,576 | UTF-8 bytes of one conversion, concatenation result, or complete print line. |
| `--max-format-work N` / `ExecutionLimits::max_format_work` | 10,000,000 | Formatting traversal, copy, and comparison work across one public run. |

```sh
solx --max-format-bytes 65536 --max-format-work 2000000 program.solx
```

The byte limit counts the actual text, including collection punctuation, object
keys, and cycle markers. It counts both operands together for concatenation.
For `print`, it includes all arguments, intervening tabs, and the final LF byte.
The complete line is formatted before any of that call's output is written;
formatting failure emits no partial line. Earlier output and side effects from
evaluating the arguments remain. Output-device failures are a separate concern.

Zero bytes permit `str("")` and empty-string concatenation; `print()` needs one
byte for its newline. Work limits apply independently, so zero work rejects a
value conversion even when its output would be empty. Hosts may raise either
limit. `DEFAULT_MAX_FORMAT_BYTES` and `DEFAULT_MAX_FORMAT_WORK` expose the
defaults in `solx::limits`; there is no separate disable switch.

A fresh work allowance starts at each `run_source`, `run_file`, and `run_program`.
Functions, imported modules, callbacks, and catch handlers share it; each REPL
buffer starts a new allowance. It is separate from evaluation steps, and
`--unlimited-steps` does not disable it. Charges account for traversal entries,
string/key copying, output bytes, object-entry sorting, and rearrangement.
These are implementation work units, not CPU time, heap bytes, or a stable
instruction-count contract. Exact charges can change with the implementation
or collection traversal order.

Values keep their existing text: list order, complete rendered object-entry
ordering, Unicode strings, opaque function/module names, and `<cycle>` markers
are preserved. A shared acyclic value is rendered at each occurrence, and this
expanded output must fit the budgets. Lists and single-property object chains
append to one buffer without repeatedly copying their descendants. Branching
objects can require superlinear sorting/rearrangement work, which is charged.
Formatting does not mutate the values. A needed collection held under a mutable
Rust borrow produces a catchable `formatting: collection is mutably borrowed`
error; release the borrow before retrying.

Byte exhaustion returns `LimitExceeded(FormatBytes)` and work exhaustion returns
`LimitExceeded(FormatWork)`. Both bypass Solx `try`/`catch`, preserve completed
effects, and unwind through the usual scope/import restoration. Formatting
checkpoints also observe cancellation and optional deadlines, including during
long object-entry comparisons. Checks precede individual copies; an in-progress
copy or output write is not preempted.

### Compatibility and remaining scope

Previously unbounded language formatting can now fail. Larger intended output
requires explicit limits, and long runs may need a larger work allowance even
when every individual line is small. Rust struct literals specifying all
`ExecutionLimits` fields must add the two formatting fields or use
`..ExecutionLimits::default()`; exhaustive `LimitKind` matches need the new
variants. Language formatting of a mutably borrowed host collection now reports
an error instead of inserting `<borrowed>`.

Rust `Value::to_solx_string`, Display, and Debug retain their unbounded host
compatibility behavior, including `<borrowed>`. This limit is not a general
String allocation cap: literals, other string library operations, error
messages (`core.error`/`core.assert` accept Strings), and diagnostic rendering
have separate behavior. JSON and selected string APIs retain their own existing
size/depth rules and catchable library errors. GC, equality, destruction, and
native operations other than formatting do not consume formatting work. Total
heap size, process output, and hard cancellation latency remain outside these
guarantees.

Runtime value destruction itself is iterative, including captured scopes and
module exports. It still needs temporary worklist storage and completes
synchronously; the formatting budgets do not bound teardown or collector
pauses. See [MEMORY_MANAGEMENT.md](MEMORY_MANAGEMENT.md).

## Regression coverage and validation records

`tests/deadlines.rs` covers zero-time boundaries for every public entry point,
cancellation precedence, disabled and large budgets, and CLI argument handling.
Private control tests compare exact `Instant` values at the expiration boundary
and verify shared/reset budgets without sleeps. `tests/source_limits.rs` covers
UTF-8 byte boundaries, default and configurable limits, entry/import behavior,
noncatchable errors, retries, CLI execution and REPL recovery. Reader unit tests
use an endless synthetic source to verify that file reading stops after at most
the allowance plus one byte.

`tests/format_limits.rs` covers UTF-8 boundaries, combined concatenation/print
limits, zero budgets, shared/reset function and import work, uncatchable errors,
recovery, host borrows, cycles, and shared-graph expansion. Deep-chain cases run
in watchdog subprocesses with small output. Renderer units compare unusual-key
object ordering with host formatting and inject cancellation during long-prefix
sorting; control units check deadline/cancellation handling independently of
evaluation steps. These describe coverage, not a current pass count. Consult
[OVERNIGHT_PROGRESS.md](OVERNIGHT_PROGRESS.md) for integrated results and exact
artifact receipts before accepting a package.

```sh
cargo test --locked --lib --bin solx --test deadlines --test source_limits --test format_limits --test execution_limits --test limits_cli --test modules -- --test-threads=1
```
