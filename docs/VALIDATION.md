# Repository and terminal installer validation

## Current validated milestone - 2026-10-25

Runtime 0.2.0, extension 0.4.0, and the complete Windows bundle now have matching
local validation. The unchanged runtime passed 421 Rust tests, formatting and
strict Clippy on October 24. Extension validation passed 77 units, 8 interpreter
integrations and 18 packaged checks on each of VS Code 1.85.2 and 1.143.1.
October 25 checks passed 32 installer helper/file/build assertions, 18 final
extracted-bundle scenarios, 11 resource-guard probes, and Actionlint. Benchmarks
retain 621 validated launches and 21 phase samples.

See [the progress record](OVERNIGHT_PROGRESS.md) for current artifact hashes,
source commits, exact receipts and remaining environment-specific checks. The
following older report is retained as historical evidence; its versions and
test counts are not the current package.

## Historical validation - 2026-10-23

Validation date: 2026-10-23, Windows x64, Windows PowerShell 5.1, Node.js 24.19.0.

## Completed work

The existing extension was retained and polished, including its grammar,
completions, snippets, hover, Run Current File, and Stop Running File. Version
0.2.1 configures `jinxware00`, fixes cancellation during launch preparation and
taskkill launch failure handling, derives its icon from the official PNG, and
validates installed VSIX contents. The obsolete broken SVG renderer and its
unused resvg dependency were replaced with a local PNG resizing script.

The installer runs entirely in the terminal. It has a Solx ASCII header,
numbered stages, coloured/plain status output, location/PATH/editor prompts,
confirmation, cancellation, backups, checksum checks, file rollback, and a
terminal uninstaller. It bundles the tested VSIX and downloads nothing during
installation. Both traditional and version-directory VS Code CLI layouts are
recognized without invoking a shell. Optional editor failures preserve Solx.
There is no graphical installer source, executable, or framework dependency.

Root documentation now uses working concatenation syntax. Redundant ignore
rules were removed, and installer/extension generated output is scoped to
ignored directories. Existing research, runtime changes, tests, and both
original logos were preserved. Older local extension packages were retained
as local artifacts; no generated packages enter the commit.

## Observed results

| Check | Result |
| --- | --- |
| Working-tree Rust formatting | `cargo fmt --all -- --check` passed |
| Working-tree Rust tests | 187 passed |
| Working-tree Clippy | `cargo clippy --all-targets --locked -- -D warnings` passed |
| Clean source snapshot | Rust formatting, 137 committed-runtime tests, and Clippy passed |
| Extension automated suite | 34 passed; baseline was 29/30 with an invalid icon size |
| Real interpreter integration | 5 passed on the working runtime and clean committed runtime |
| VSIX packaging/content verification | Passed; 15 archive entries, no runtime npm dependencies |
| Packaged desktop host | 11 checks passed on VS Code 1.85.2 |
| Installer CLI helper | 12 passed, including old/new CLI layouts, quoting, missing CLI, exit errors, and timeout |
| Installer files and PATH logic | 6 passed, including checksums, rollback, backups, and preservation of PATH text |
| Actual terminal installer scripts | 8 scenarios passed using extracted ZIPs and isolated VS Code directories |
| Terminal session | Installer launched, prompts accepted, skipped extension installation completed, all five stages displayed |
| Terminal cancellation | Ctrl+C at confirmation exited without creating the destination folder |
| Host preservation | Existing installed Solx binary and user PATH unchanged during isolated installation tests |

The clean snapshot combined the original committed runtime with this task's
extension files; it excluded the unrelated local runtime work. Fresh `npm ci`
succeeded there and reported zero known dependency vulnerabilities.

## Outputs and limits

- Extension: `vscode-solx/solx-language-support-0.2.1.vsix`.
- Terminal installer: `dist/solx-0.1.0-windows-x64.zip`.
- Installer provenance and SHA256: `dist/solx-0.1.0-windows-x64.zip.json`.
- Detailed local test logs: ignored installer `.artifacts` and extension
  `.test-artifacts` directories.

The local installer contains the current working-tree runtime, including
pre-existing uncommitted runtime changes; the provenance file flags this.
Those unrelated changes are excluded from the task commit. Build from a clean
checkout before distributing a release.

Actual PATH registry changes and environment broadcasts were not exercised on
the host: installation tests explicitly skipped PATH writes. The PATH text
transformations were tested separately. A fresh Windows VM should cover PATH
updates, restricted permissions, restart behavior, and interruptions during
real disk failures. The installer scripts are unsigned. Non-Windows desktop
extension behavior and Marketplace rendering are not verified. Publisher
ownership and name availability still require Marketplace verification.

## Task file inventory

Root and documentation: `.gitignore`, `README.md`, `assets/logo.png` (added
unchanged), `docs/INSTALLING.md`, `docs/DEVELOPMENT.md`, `docs/VALIDATION.md`.

Installer: `installer/windows/build.ps1`, `install.ps1`, `uninstall.ps1`,
`Installer.psm1`, `VSCode.psm1`, `test.ps1`, `test-files.ps1`, `test-install.ps1`.

The previously untracked extension is included as a complete working unit:
`vscode-solx/.gitignore`, `.vscode/launch.json`, `CHANGELOG.md`, `LICENSE`,
`README.md`, `VALIDATION.md`, `package.json`, `package-lock.json`,
`language-configuration.json`, `data/snippets.json`, `examples/hello.solx`,
`images/icon.png`, `syntaxes/solx.tmLanguage.json`, `scripts/render-icon.ps1`,
`scripts/test-host.cjs`, `scripts/vscode-cli.cjs`, `src/catalog.cjs`,
`src/extension.cjs`, `src/language.cjs`, `src/runner.cjs`, `test/cli.test.cjs`,
`test/extension.test.cjs`, `test/grammar.test.cjs`, `test/host/index.cjs`,
`test/interpreter.cjs`, `test/language.test.cjs`, `test/metadata.test.cjs`,
`test/runner.test.cjs`.

The old extension renderer `scripts/render-icon.cjs` was replaced. No runtime,
specification, experiment, or original SVG changes belong to this task.
