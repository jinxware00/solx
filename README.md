# Solx

<img src="assets/logo.png" alt="Solx" width="120">

Solx is a small, Lua-inspired language implemented in Rust. It supports
functions and closures, lists and objects, numeric and collection loops,
explicit module imports, and `try`/`catch` with structured runtime errors.
Version 0.2 adds storable `nil`, exact checked 64-bit integers within `Number`,
and an explicitly imported [JSON module](docs/JSON.md). Current source also
includes [raw multiline strings](docs/RAW_STRINGS.md),
[snapshot collection iteration](docs/ITERATION.md),
[map/filter/fold and custom sorting](docs/COLLECTION_APIS.md), and
[bounded string utilities](docs/STRINGS.md). Read the
[nil migration](docs/NIL_MIGRATION.md) and [numeric rules](docs/NUMBERS.md)
when updating programs from 0.1.
It is in early development; see the [specification](docs/SPEC.md) for the
distinction between implemented behavior and planned features.
The [language review](docs/LANGUAGE_REVIEW.md) records current gaps, lessons
from the official Lua source, and the next development priorities.
Automatic [cycle collection](docs/MEMORY_MANAGEMENT.md) reclaims unreachable
closures and collections while preserving values held by Rust callers.
[Execution limits and Ctrl+C cancellation](docs/EXECUTION_LIMITS.md) stop
runaway evaluation and bound syntax, call, and import depth.
[Optional deadlines and source limits](docs/RESOURCE_BUDGETS.md) add
`--timeout-ms` and a configurable 1 MiB runtime input limit. Deadlines are
cooperative; they do not preempt native operations or I/O.
Use `solx check file.solx` for [non-executing syntax checks](docs/CHECKING.md),
also available as live diagnostics in the VS Code extension. Opt in to
[name analysis and declaration symbols](docs/ANALYSIS.md) with
`solx check --names --symbols --json file.solx`.
Multiple returns, `pcall`, interpolation, a bytecode VM and package tooling
remain unimplemented; see the [roadmap](docs/ROADMAP.md).

## Example

```solx
var name = "Solx" -- define a variable

fn greet(player) -- making a function
    print("Hello, " .. player .. "!")
end

greet(name) -- calling a function
```

## Get started

Build with stable Rust using `cargo build --release --locked`, then run
`cargo run -- examples/functions.solx`. Running `solx` without arguments opens
the interactive console.

The Windows installer includes an optional **Solx Language Support** extension
for VS Code. Packages currently require a local build; no published release is
claimed. Matching validation of the current runtime, extension 0.4.0 VSIX and
installer is pending; earlier artifact reports apply to their recorded source
versions. See [installation instructions](docs/INSTALLING.md), the
[extension guide](vscode-solx/README.md), and
[development and packaging](docs/DEVELOPMENT.md).

Licensed under [MIT](LICENSE).
