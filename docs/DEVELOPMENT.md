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
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test-build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/build.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/test-install.ps1 -CodeRoot 'path\to\Microsoft VS Code' -SkipBuild
```

The build checks the VSIX identity, the successful packaged-host receipt, and
every packaged file against current source bytes. It rebuilds the interpreter,
then creates `dist/solx-0.2.0-windows-x64.zip` plus a JSON provenance/hash record. The ZIP
contains the terminal scripts, executable, VSIX, official PNG, MIT license,
installation guide, and checksums. Scripts download nothing. The standalone
VSIX follows the extension version, currently
`vscode-solx/solx-language-support-0.4.0.vsix`. Earlier ZIPs retain their older
contents; rebuilding a standalone VSIX does not update them. The schema-v2
provenance records source/payload hashes and tool versions. Replacing a bundle
preserves its previous bytes and provenance under `dist/history` first.
See [CI and build validation](CI.md) for automated checks and their limits.

`test.ps1` covers discovery and CLI argument/failure handling. `test-files.ps1`
covers PATH edits, checksum validation, rollback, and backups. `test-install.ps1`
checks the archive hash, every extracted payload hash, source hashes, package
versions and the packaged-host receipt before executing the bundle. It checks
the installed CLI version and exact-integer/nil/JSON behavior, then exercises
the actual terminal scripts: selected/skipped extension, missing VS Code/CLI,
nonzero exit, interactive prompts and decline/quit/EOF cancellation, upgrades
with backups, rollback after a real destination-file conflict, damaged bundles,
and uninstall cancellation, unsafe metadata and preservation of user files.
It writes results after every scenario and retains failure output. It uses
`-AddToPath No` and isolated VS Code data directories; user PATH, the normal
Solx executable, and normal editor settings/extension registration are checked
for changes even after a failed scenario. Pass `-SkipBuild` to validate an
existing current-source archive without recompiling it. The CI builds once
and then uses this option for its extracted-bundle checks.
Terminal scenarios have a 180-second process watchdog; interpreter version and
semantic checks have 30-second watchdogs. Process cleanup also runs after an
exception or interruption. Output-pipe draining is bounded. Run validation
serially with an outer process-tree memory/time limit on resource-constrained
hosts; the script watchdog alone is not a total host-memory bound.

These tests exercise the complete ZIP and real interpreter/VS Code CLI, but
the upgrade predecessor is a local fixture. They do not claim actual PATH
registry/broadcast, VM reboot, locked-system-directory or forced-power-loss
coverage. Those require a disposable Windows VM. See the current
[installer validation record](../installer/windows/VALIDATION.md) for observed
results and remaining checks.

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
