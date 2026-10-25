<div align="center">
  <img src="https://raw.githubusercontent.com/jinxware00/solx/main/assets/logowtext.svg" alt="Solx Programming Language" width="350">
</div>

This is the source code repository for Solx. It contains the compiler,
standard library, and documentation.


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

[GitHub Releases](https://github.com/jinxware00/solx/releases) contains development builds of the Windows installer and the optional VS Code extension, `solx-ls-0.1.0.vsix`. Release assets include SHA-256 checksums and build provenance.

## Installing from the Source

Build with stable Rust using `cargo build --release --locked`, then run
`cargo run -- examples/functions.solx`. Running `solx` without arguments opens
the interactive console.

## Visual Studio Code

The Windows installer includes an optional **Sol Language Support** extension for Visual Studio Code. See [installation instructions](docs/INSTALLING.md), the
[extension guide](vscode-solx/README.md), and
[development and packaging](docs/DEVELOPMENT.md).

Licensed under [MIT](LICENSE).
