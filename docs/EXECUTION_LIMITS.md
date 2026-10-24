# Execution limits and cancellation

The tree-walking interpreter stops excessive syntax nesting, deep ASTs,
recursive execution, import chains, and runaway evaluation with structured
errors. Current source also provides optional deadlines, bounded source reads,
and formatting byte/work budgets. Rebuild to use source changes; package
validation applies only to the source and artifact hashes in its receipt.

## Defaults and counting

| Limit | Default | What it counts |
| --- | --- | --- |
| Parser nesting | 64 | Active statement and expression parser entries, combined. |
| AST depth | 128 | Nodes along a syntax-tree path, including source-location wrappers. |
| Evaluation depth | 64 | Active statement and expression evaluations, across functions and imports. |
| Function call depth | 32 | Active user-function calls, including user functions invoked by native helpers. |
| Import depth | 16 | Active uncached imports, including built-ins; the entry file is excluded. |
| Evaluation steps | 10,000,000 | Executed statements, evaluated expressions, loop iterations, and explicit collection work described below. |
| Elapsed deadline | Disabled | Monotonic elapsed time per public run, checked cooperatively. |
| Source bytes | 1,048,576 | UTF-8 bytes per runtime source buffer or file. |
| Formatted bytes | 1,048,576 | UTF-8 bytes per `str`, concatenation, or complete `print` line. |
| Formatting work | 10,000,000 | Formatting traversal, copy, and comparison work per public run. |

The five depth defaults are also conservative ceilings for this recursive
implementation. Hosts can lower them, but cannot disable or raise them. They
are not language grammar changes or a portable proof about every host stack.
The combined evaluation limit may stop a program before either its individual
call or import limit is reached. These limits intentionally reject some large
programs that previously ran without a guard.

A fresh step budget starts at each public `run_source`, `run_file`, or
`run_program` call. Imported files, called functions, and catch handlers share
that budget; they cannot reset it. Empty loops consume steps, including numeric
loops with empty bodies or `continue`. Short-circuited expressions are not
evaluated or charged. `Located` metadata wrappers do not add execution steps.
An empty program needs no steps; a zero budget rejects its first evaluation.
This counts interpreter work, not bytecode instructions, CPU time, or bytes.

Collection iteration charges snapshot entries and iterations. Callback list
operations charge snapshot copies, visits, merge movements, and normal callback
evaluation. Native callbacks can invoke Solx functions; they share the caller's
step, depth, deadline, and formatting controls. See [COLLECTION_APIS.md](COLLECTION_APIS.md)
for their detailed counting and mutation rules. Formatting work uses a separate
counter and does not consume additional evaluation steps.

## Command line

```sh
solx --max-steps 50000 program.solx
solx --max-call-depth 12 --max-import-depth 8 program.solx
solx --timeout-ms 1000 --max-format-bytes 65536 program.solx
solx --unlimited-steps program.solx
```

`--unlimited-steps` disables only the step budget and conflicts with an explicit
`--max-steps`. Depth, source, formatting, cancellation, and any configured
deadline remain active. Use `solx --help` for the options; invalid depths fail
before executing user code. [RESOURCE_BUDGETS.md](RESOURCE_BUDGETS.md) defines
`--timeout-ms`, `--max-source-bytes`, `--max-format-bytes`, and `--max-format-work`.

Ctrl+C requests cooperative cancellation. On Windows, Solx explicitly enables
Ctrl+C delivery after registering its handler, including when its launcher
passed down an ignored Ctrl+C flag. Ctrl+Break uses that handler too. An
interrupted file or `-e` command prints an error and exits with status 1.

Each REPL buffer gets a fresh budget. After interruption, the REPL clears the
cancel request and accepts another command. Completed state changes remain;
the buffer is not replayed. At an idle input prompt, some consoles require
Enter before the pending read completes and the cancelled buffer is discarded.
Use `quit` or `exit` to leave the REPL.

## Rust embedding

```rust
use solx::interpreter::Interpreter;
use solx::limits::ExecutionLimits;
use solx::runtime::error::RuntimeErrorKind;

let mut interpreter = Interpreter::with_limits(ExecutionLimits {
    max_steps: Some(50_000),
    max_call_depth: 12,
    ..ExecutionLimits::default()
}).unwrap();
let token = interpreter.cancellation_token();
// Send a clone to a controller thread, which may call token.cancel().
token.cancel();
let error = interpreter.run_source("while true do end").unwrap_err();
assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
// After the interrupted run and its controller have ended:
token.reset();
interpreter.run_source("global answer = 42").unwrap();
```

`ExecutionLimits::max_steps = None` disables the step budget for an embedder.
`ExecutionLimits::parse` holds `ParseLimits`; standalone parsers use
`parse_with_limits(source, limits)` or `parse(source)` with defaults. Invalid
configuration returns an error rather than silently clamping it or panicking.
`Interpreter::limits()` returns the current configuration, which is fixed for
that interpreter's lifetime.

The interpreter remains single-threaded and does not become Send. The cloned
`CancellationToken` alone is thread-safe. Cancellation is sticky across host
runs until the host resets the token. Do not reset it while the interrupted run
or another controller is still active.

`RuntimeError.kind` distinguishes `Language`, `LimitExceeded(LimitKind)`, and
`Cancelled`; `is_catchable()` is true only for language errors. `ParseError.limit`
is `Some(LimitKind)` for parser resource failures, otherwise None. Rust callers
constructing either error with struct literals need to supply these metadata
fields. The optional `RuntimeError.location` is boxed
to keep diagnostic data off recursive Result stack frames; Rust callers
constructing that field need `Some(Box::new(location))`. Location contents,
error messages/traces, and Solx catch-object fields are unchanged.

## Interruption and restoration

Interpreter limit failures and cancellation bypass Solx `try`/`catch`. A script
therefore cannot catch exhaustion and resume a runaway loop. Ordinary syntax errors in
imported modules, user-raised errors, and other language failures retain their
existing catch behavior. Imported parser-limit failures preserve their kind.
Library validation errors, including JSON's separate size/depth checks, remain
catchable language errors.

Scopes, call depth, evaluation depth, import loading state, and diagnostic
context are restored during unwinding. Completed assignments and mutations
remain, and escaped closures keep their captured locals. An interrupted module
is not cached and can be retried later. Successful dependencies stay cached,
including when a later statement in the entry file exhausts its budget.

Parser nesting is checked before recursive descent. An iterative AST-depth
check runs as each expression/statement is assembled, including left-associated
and postfix chains. This keeps rejected partial ASTs bounded too. Host-supplied
Programs are validated before any execution or function-body cloning. The host
still owns and must safely dispose of a rejected, manually built deep AST.

## Limits of this protection

Cancellation and optional deadlines are checked at evaluation/loop checkpoints,
before imports, around parsing, during collection snapshot/callback work, and
between formatting traversal/copy/comparison operations. They do not preempt a
lexer/parser pass, filesystem operation, output write, synchronous GC pass, or
other native work already underway. A single uninterrupted native operation
can still perform substantial work.

Language formatting has per-operation byte and per-run work limits. These do
not cover all string construction, diagnostics, Rust host Display/Debug, or
total process output. There is no total heap/allocation cap, hard process
timeout, filesystem sandbox, or out-of-memory recovery.

Runtime value destruction uses an explicit worklist for lists, objects,
functions, captured scopes, and module exports; shared values remain intact and
cycles still use the collector. Teardown is synchronous and has no work or
memory budget. Parsing, evaluation, and function AST clone/drop retain recursive
paths. Manually built deep host ASTs and unusually small host thread stacks
still need care; no Rust stack overflow is caught after it happens. See
[MEMORY_MANAGEMENT.md](MEMORY_MANAGEMENT.md) for the ownership contract and Rust
API migration. An explicit execution stack, broader native/allocation budgets,
and memory/latency profiling remain future work.

## Historical validation: initial execution-limit milestone

These are the original Windows results, not current suite totals or validation
of later deadlines, source limits, formatting, or iterative destruction.
Current integration outcomes belong in [OVERNIGHT_PROGRESS.md](OVERNIGHT_PROGRESS.md).

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --locked --quiet` | 254 passed (229 baseline + 25 new tests) |
| `cargo clippy --all-targets --locked -- -D warnings` | Passed |
| `cargo test --release --locked --test execution_limits --test parse_limits --test limits_cli` | All 25 passed with the optimized interpreter |
| Extension `npm test` | 35 passed |
| Extension `npm run test:interpreter` | 5 passed against rebuilt source |
| `cargo build --release --locked --bin solx` | Passed; refreshed `target/release/solx.exe` |
| Release CLI: `examples/error_handling.solx` and `--help` | Passed |
| `git diff --check` | Passed |

That milestone's tests cover parser boundaries and positions, malformed partial trees,
deep left/postfix chains, wide shallow programs, host AST rejection, exact step
accounting, empty loops, recursion, cancellation, scope/closure restoration,
shared import budgets, cache behavior, failed-import retry, and REPL recovery.
Adversarial sources also run in real CLI subprocesses with watchdogs, including
combined import/parser/function/AST-clone depth, to detect stack aborts or hangs.

On Windows, an actual terminal session running `--unlimited-steps` was stopped
with Ctrl+C and exited with status 1 and `execution cancelled`. A separate REPL
session was interrupted, printed `true` for its retained counter being positive,
then printed `42` and quit with status 0. This exercised the Windows inherited
Ctrl+C flag fix, beyond the automated token tests. Other OS signal paths have
not been exercised here. This milestone does not validate new installer or
extension packages.

Changed files: `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/limits.rs`,
`src/main.rs`, `src/parser/mod.rs`, `src/parser/depth.rs`,
`src/interpreter/mod.rs`, `src/interpreter/eval.rs`,
`src/interpreter/control.rs`, `src/runtime/error.rs`,
`tests/parse_limits.rs`, `tests/execution_limits.rs`, `tests/limits_cli.rs`,
`README.md`, `docs/SPEC.md`, `docs/LANGUAGE_REVIEW.md`,
`docs/MEMORY_MANAGEMENT.md`, `docs/RUNTIME_AUDIT.md`, and this document.
Ctrl+C support uses pinned `ctrlc` 3.5.2; the Windows enablement fix directly
uses the already-present `windows-sys` 0.61.2. Platform dependencies are locked
in Cargo.lock. No interpreter/extension version changed. Branding, experiments,
installer files, and existing packages were preserved; no commit or push was
made during this implementation milestone.
