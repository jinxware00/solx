# Development and packaging

The language specification is [SPEC.md](SPEC.md). Keep runtime changes separate
from packaging work. Interpreter and extension versions come from `Cargo.toml`
and `vscode-solx/package.json`, respectively.

## Checks

From the repository root, with stable Rust and the platform's compiler tools:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

For the extension, use Node.js 22+ and npm:

```sh
cd vscode-solx
npm ci
npm test
npm run test:interpreter
npm run package
npm run test:package -- 1.85.2
```

The last command installs the VSIX into an isolated directory, compares every
packaged file with source, and opens a desktop Extension Development Host
loading that copy. The first run downloads the specified official VS Code build.
Omit the version to use installed VS Code, or set `VSCODE_EXECUTABLE`. Nothing
is installed into your normal VS Code profile. F5 and `npm run test:host` still
provide source previews. Output is kept in ignored `.test-artifacts` folders.

## Terminal installer

The audit found only a manually installed `bin/solx.exe`, with no installer
source or release workflow in the repository/history. `installer/windows`
packages that existing Cargo executable and follows its per-user directory
layout. Windows PowerShell supplies the terminal interface, file installation,
and uninstall commands. No graphical installer framework is required.

Build on Windows using Windows PowerShell 5.1 and the x64 MSVC Rust target.
After the VSIX passes `test:package`, run from the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test-files.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test-install.ps1 -CodeRoot 'path\to\Microsoft VS Code'
```

The build checks the VSIX identity and the hash recorded by its successful
packaged host test, rebuilds the interpreter, then creates
`dist/solx-0.1.0-windows-x64.zip` plus a JSON provenance/hash record. The ZIP
contains the terminal scripts, executable, VSIX, official PNG, MIT license,
installation guide, and checksums. Scripts download nothing. The standalone
VSIX remains `vscode-solx/solx-language-support-0.2.1.vsix`.

`test.ps1` covers discovery and CLI argument/failure handling. `test-files.ps1`
covers PATH edits, checksum validation, rollback, and backups. `test-install.ps1`
extracts the ZIP and tests the actual terminal scripts: selected/skipped
extension, missing VS Code/CLI, nonzero exit, interactive prompts/cancellation,
and uninstall preserving modified files. It uses `-AddToPath No` and isolated
VS Code data directories; normal installations are preserved.

For a manual terminal check, extract the ZIP and run `install.ps1` with a
temporary `-InstallDirectory` and `-AddToPath No`. Decline editor integration,
or supply isolated `-ExtensionsDirectory` and `-UserDataDirectory` paths.
Check prompt defaults, narrow-terminal wrapping, colour/plain output, and Ctrl+C.

Packages, build caches, and test results are ignored by Git. Source, branding,
licenses, examples, tests, and lockfiles are retained. Build from a clean tree
before distribution: the provenance record flags uncommitted runtime changes.
The ZIP hash checks corruption, not publisher authenticity. Signing scripts
and a fresh Windows VM test of real PATH changes remain release checks.
Verify Marketplace publisher ownership and availability of
`jinxware00.solx-language-support` before manual publication. No publishing
or account changes are performed by these scripts.
