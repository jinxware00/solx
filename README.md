<div align="center">
  <img src="https://raw.githubusercontent.com/jinxware00/solx/main/assets/logowtext.svg" alt="Solx Programming Language" width="400">
</div>

# Solx

<img src="assets/logo.png" alt="Solx" width="120">

Solx is a small, Lua-inspired language implemented in Rust. It supports
functions and closures, lists and objects, loops, explicit module imports,
and `try`/`catch` with structured runtime errors.
It is in early development; see the [specification](docs/SPEC.md) for the
distinction between implemented behavior and planned features.
The [language review](docs/LANGUAGE_REVIEW.md) records current gaps, lessons
from the official Lua source, and the next development priorities.
Automatic [cycle collection](docs/MEMORY_MANAGEMENT.md) reclaims unreachable
closures and collections while preserving values held by Rust callers.
[Execution limits and Ctrl+C cancellation](docs/EXECUTION_LIMITS.md) stop
runaway evaluation and bound syntax, call, and import depth.
Use `solx check file.solx` for [non-executing syntax checks](docs/CHECKING.md),
also available as live diagnostics in the VS Code extension.

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
claimed. See [installation instructions](docs/INSTALLING.md), the
[extension guide](vscode-solx/README.md), and
[development and packaging](docs/DEVELOPMENT.md).

Licensed under [MIT](LICENSE).
