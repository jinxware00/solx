# Solx Language Support 0.2.1 validation

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
