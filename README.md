<div align="center">
  <img src="https://raw.githubusercontent.com/jinxware00/solx/main/assets/logowtext.svg" alt="Solx Programming Language" width="350">
</div>

This repository contains the Solx interpreter, standard library,
VS Code extension, and documentation.

## What is Solx?

- Solx is a lightweight language written in Rust.
- It is currently in the early stages of development, projected to release in December 2026 / January 2027

## Example

```solx
var name = "Solx" -- define a variable

fn greet(player) -- making a function
    print("Hello, " .. player .. "!")
end

greet(name) -- calling a function
```

## Downloads

[Solx v0.2.0](https://github.com/jinxware00/solx/releases/tag/v0.2.0) is available as a development release.

| Download | Includes |
| --- | --- |
| [Windows x64 installer](https://github.com/jinxware00/solx/releases/download/v0.2.0/solx-0.2.0-windows-x64.zip) | Solx 0.2.0, terminal installer, and optional VS Code extension |
| [solx-ls-0.1.0.vsix](https://github.com/jinxware00/solx/releases/download/v0.2.0/solx-ls-0.1.0.vsix) | Sol Language Support 0.1.0 for VS Code |
| [SHA256SUMS](https://github.com/jinxware00/solx/releases/download/v0.2.0/SHA256SUMS) | Checksums for the release downloads |

Extract the Windows ZIP, open PowerShell in that folder, and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

The installer can add Solx to your user PATH and install the bundled extension.
After adding it to PATH, open a new terminal and run `solx your-file.solx`, or
`solx` to start the interactive console. See [installation instructions](docs/INSTALLING.md)
for requirements and options. The release also includes a build provenance record.

## Installing from the Source

Install a stable Rust toolchain (CI uses Rust 1.99.0), then run from a checkout
of this repository:

```sh
cargo build --release --locked
cargo run --locked -- examples/functions.solx
```

Run `cargo run --locked` to start the interactive console from source. CI checks
the runtime on Windows, Linux, and macOS; the packaged installer targets Windows x64.

## Visual Studio Code

**Sol Language Support** provides syntax highlighting, completions, hover,
live diagnostics, Outline symbols, and run/stop commands in VS Code 1.85 or newer.
Install it through the Windows installer, or download `solx-ls-0.1.0.vsix` and
choose **Extensions: Install from VSIX...** in VS Code. Running and checking
programs requires the Solx interpreter.

See the [extension guide](vscode-solx/README.md),
[language specification](docs/SPEC.md), and
[development and packaging guide](docs/DEVELOPMENT.md).

Licensed under [MIT](LICENSE).
