# Solx Language Support validation

## 0.3.1 — Windows checker startup and snapshot cleanup

Validated on Windows x64 on 2026-10-23, based on commit `828d447`.
This extension update is unpublished and was validated before committing.

The 26-second delay reproduced in a minimal Rust stdin reader before its
`main` function began, including when stdin was an already-written file handle.
The underlying Windows/environment cause is still unknown. File checks of
temporary buffers completed in 0.396–0.493 seconds in the same unsandboxed
environment. Version 0.3.1 uses that existing CLI interface on Windows;
other platforms retain stdin. No Rust, parser, runtime, or JSON-schema changes
were made.

Each Windows check writes the exact UTF-8 buffer into a fresh temporary
directory. Exclusive creation prevents overwriting another file. Cleanup covers
completion, errors, cancellation, timeouts, and normal shutdown. Cancellation
during creation prevents launching; obsolete requests still report cleanup
failures. The README explains the temporary disk writes and the possibility of
leftovers after a hard crash or denied cleanup.

Results:

- `npm test`: **57 passed**, including 11 additional tests for Windows input
  arguments, snapshot bytes/permissions/lifecycle, partial-write failures,
  cleanup failures, cancellation, timeouts, and awaited deactivation.
- `npm run test:interpreter`, outside the sandbox: **6 passed**. Its four real
  buffer checks took about **1.98 seconds total**, compared with about 103.85
  seconds in the earlier stdin run. Snapshot directories were removed and the
  saved document remained unchanged.
- `npm run package` passed with its 57-test prepublish check.
- Isolated installation and packaged-file comparisons passed in **VS Code
  1.85.2 and 1.143.1**. All **15 automated desktop host checks passed in each**,
  including live errors, dirty/untitled documents, non-execution, run/stop, and
  repeated-check timing. Both closed with **zero temporary snapshots remaining**.
- `git diff --check` passed. Rust checks were not rerun because this update
  changes no Rust source or dependencies; the previous 261-test result is below.

Final repeated-check timings, including any queued checks:

| VS Code | First request | Second request | Third request |
| --- | ---: | ---: | ---: |
| 1.85.2 | 5,213 ms | 828 ms | 844 ms |
| 1.143.1 | 5,338 ms | 896 ms | 849 ms |

The first request waited for other open documents. Each request must finish in
under ten seconds in the regression test; this is not a universal performance
guarantee. The production timeout stays at 30 seconds. The desktop checks are
automated tests in the actual editor, not a manual visual walkthrough.

The initial 0.3.1 host tests exposed a shutdown cleanup race: subscription
disposal alone did not wait for the active checker. Async deactivation fixed
it. The three leftovers from those runs were matched against the test fixtures
and removed. The final harness isolates its temporary directory and fails if
any check snapshots survive shutdown.

Final artifact: `vscode-solx/solx-language-support-0.3.1.vsix`, **109,258 bytes**;
SHA-256 `3c0fde511ed08590c23c25f328ef2354bc70b80995e3ae7365339683f64a2932`.
It includes all seven runtime modules and the unchanged icon, grammar, snippets,
example, and license. Tests and probe artifacts are excluded.

Detailed results are under this extension directory:

- `.test-artifacts/host-Jzz2UC/` — final VS Code 1.85.2 run.
- `.test-artifacts/host-Fxinjz/` — final VS Code 1.143.1 run.
- `.test-artifacts/package-validation.json` — matching VSIX hash and zero-leak
  receipt from the final run.

Files changed for this update:

```text
docs/CHECKING.md
docs/DEVELOPMENT.md
vscode-solx/CHANGELOG.md
vscode-solx/README.md
vscode-solx/VALIDATION.md
vscode-solx/package.json
vscode-solx/package-lock.json
vscode-solx/scripts/test-host.cjs
vscode-solx/src/checker.cjs
vscode-solx/src/diagnostics.cjs
vscode-solx/src/extension.cjs
vscode-solx/src/runner.cjs
vscode-solx/src/snapshot.cjs (new)
vscode-solx/test/checker.test.cjs
vscode-solx/test/diagnostics.test.cjs
vscode-solx/test/extension.test.cjs
vscode-solx/test/host/index.cjs
vscode-solx/test/interpreter.cjs
vscode-solx/test/snapshot.test.cjs (new)
```

The original logos, unrelated logo edits, and `experiments/` remain untouched.
Earlier VSIX/installer bundles are preserved; the installer was not rebuilt,
and no normal VS Code profile or installed interpreter was updated.

## 0.3.0 — syntax checking and diagnostics

Validated on Windows x64 on 2026-10-23. This package is unpublished.

- Before these changes: 254 Rust tests and 35 extension tests passed.
- Final `cargo fmt --all -- --check` passed.
- Final `cargo test --locked --quiet`: **261 passed**, including seven new CLI
  tests for non-execution, file/stdin input, JSON failures, Unicode byte ranges,
  EOF, malformed syntax, and syntax-depth limits.
- `cargo clippy --all-targets --locked -- -D warnings` passed.
- `cargo build --release --locked` passed. Release smoke checks covered valid
  file syntax, Unicode/CRLF lexer ranges, a non-executed infinite loop, check
  help, and existing `-e` execution (`42`).
- `npm test`: **46 passed**. New tests cover protocol validation, stdin delivery,
  UTF-8/UTF-16 conversion, debounce, cancellation, stale results, document and
  configuration lifecycle, timeouts, missing/old executables, and failures.
- `npm run test:interpreter`: **6 passed** against the real CLI, both inside and
  outside the command sandbox. Existing snippet, library, and runner checks
  continue to pass. The added buffer check does not run code or imports.
- `npm run package` passed, including its 46-test prepublish check. The VSIX
  contains the icon, README, grammar, snippets, example, license, changelog,
  configuration, and all six runtime JavaScript modules. No runtime npm
  dependencies, tests, personal machine paths, or development artifacts are shipped.
- `npm run test:package -- 1.85.2`: isolated installation and byte comparisons
  of the shipped files passed; **all 14 automated desktop Extension Development
  Host checks passed against the installed VSIX**. These exercised actual
  providers, Problems entries, unsaved/untitled buffers, clearing diagnostics,
  missing interpreters, Cargo rebuilding, and running/stopping programs.
  This is automated GUI-host validation, not a manual visual walkthrough.

Artifacts (repository-relative):

- `vscode-solx/solx-language-support-0.3.0.vsix` — 107,199 bytes;
  SHA-256 `3f2f48a82ea8e89126582aacb8894d15ea68558f338ec8408754557b3566e3a8`.
- `target/release/solx.exe` — current source-built CLI; SHA-256
  `6b92a91810be2bbcb946c579ccb086675f45c2c12bba173c134f4c1732b9d744`.
- `.test-artifacts/host-VTqPBh/` within this extension directory contains the
  successful install log, packaged file/hash list, host log, and per-check
  results/timings. `.test-artifacts/package-validation.json` records its receipt.

### Remaining limitations

The first desktop attempts in VS Code 1.85.2 and 1.143.1 reached the former
120-second suite timeout. Probes found roughly 26 seconds of Windows process
startup latency per stdin check outside the command sandbox, also reproduced
with standalone Node outside that sandbox. Inside it, checks completed in
under a second. The cause is unresolved. Argument order, an alternate stdin
selector, detached processes, and overlapped pipes did not solve it; those
experiments are not part of the extension or CLI.

The final harness permits 600 seconds for all checks, records per-check timing,
and waits up to 35 seconds for automatic diagnostics (the production check
timeout plus debounce/scheduling). All 14 checks then passed in VS Code 1.85.2;
the full 1.143.1 suite was not rerun. The three diagnostic host checks took
approximately 104, 53, and 54 seconds, including multiple queued requests.
Responsive checking on this Windows host remains a release concern: profile
the startup delay in a normal Windows session and a fresh VM before publishing.

Checking reports the first lexer/parser/depth-limit error only, with a 1 MiB
UTF-8 input limit. It does not resolve imports or check names/types. Old Solx
binaries need rebuilding; there is no execution fallback. At validation time,
no installer bundle was rebuilt, installed interpreter replaced, Marketplace
release published, or Git commit/push made. Earlier ZIPs and VSIX files are preserved. Publisher
ownership and identifier availability still require manual verification.

### Files changed for this milestone

New files:

```text
docs/CHECKING.md
src/check.rs
tests/check_cli.rs
vscode-solx/src/checker.cjs
vscode-solx/src/diagnostics.cjs
vscode-solx/test/checker.test.cjs
vscode-solx/test/diagnostics.test.cjs
```

Updated files:

```text
Cargo.toml
Cargo.lock
README.md
docs/DEVELOPMENT.md
docs/LANGUAGE_REVIEW.md
docs/SPEC.md
src/lib.rs
src/main.rs
src/lexer/mod.rs
src/lexer/token.rs
src/parser/mod.rs
vscode-solx/package.json
vscode-solx/package-lock.json
vscode-solx/README.md
vscode-solx/CHANGELOG.md
vscode-solx/VALIDATION.md
vscode-solx/scripts/test-host.cjs
vscode-solx/src/extension.cjs
vscode-solx/src/runner.cjs
vscode-solx/test/extension.test.cjs
vscode-solx/test/interpreter.cjs
vscode-solx/test/host/index.cjs
```

The official icon and grammar are reused without changes. Pre-existing changes
to `assets/logo.svg`, untracked `assets/logowtext.svg`, and `experiments/` are
outside this milestone and were preserved. Validation was performed before staging.

## 0.2.1 — historical validation

Validated on Windows x64 with Node.js 24.19.0 on 2026-10-23.

- Baseline: 29/30 extension tests passed; the icon size test failed and the old
  SVG renderer had a syntax error. Both were corrected using the official PNG.
- Final automated suite: 34 tests passed, covering grammar, editing resources,
  completions, hover, run/cancel behavior, metadata, assets, and VS Code layouts.
- Five interpreter integration tests passed, including real lexer/parser/runtime
  failures and all 11 snippet defaults. They also passed against the committed
  runtime in a separate clean source snapshot.
- The VSIX installed successfully into isolated directories. All shipped files
  were checked against source; development files and dependencies were excluded.
- All 11 packaged Extension Development Host checks passed in desktop VS Code
  1.85.2. This exercised real completion/hover providers, snippets, run errors,
  Cargo rebuilding, and process stopping.
- Publisher is `jinxware00`. The icon is a visually inspected, transparent
  512 x 512 PNG derived from `assets/logo.png`; both original logos are preserved.

Live diagnostics, debugging, formatting, and interactive stdin are not supplied.
Accurate edit-time diagnostics need a non-executing structured CLI interface.
Marketplace publisher ownership and identifier availability still need manual
verification. Nothing was published.

See [repository validation](../docs/VALIDATION.md) for the terminal installer,
preservation audit, changed files, package paths, and remaining release checks.
