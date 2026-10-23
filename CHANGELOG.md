# Changelog

## 0.2.0 — unreleased

- Collection `for ... in` loops snapshot entries, use deterministic object-key
  ordering and preserve fresh bindings for closures. `in` is now reserved.
- Optional cooperative execution deadlines and bounded runtime file/REPL source
  input. The new default input limit is 1 MiB and can be configured explicitly.
- Bounded literal string replacement and Unicode scalar conversion helpers,
  plus explicit UTF-8 byte length.
- Triple-double-quoted raw multiline Strings preserve braces, backslashes,
  whitespace and line endings. The REPL accepts continuation lines; ordinary
  quoted strings retain their escape rules. Interpolation remains planned.
- Callback `list.map`, `list.filter`, `list.fold` and stable `list.sorted` use
  shallow snapshots and share execution limits, scopes and error handling.
- Opt-in `check --names` performs conservative AST name analysis; independent
  `check --symbols --json` returns exact declaration ranges. Default checks
  remain syntax-only. Extension 0.4.0 source adds opt-in name diagnostics and
  Outline symbols.
- Cross-platform CI definition, installer source/VSIX content validation,
  expanded provenance and preservation of earlier archives.
- Deterministic generated-source checks and a reproducible local benchmark
  harness with source/binary hashes and equivalent workload checksums.

- `nil` is now stored by assignment rather than deleting a binding. See
  [migration notes](docs/NIL_MIGRATION.md).
- Exact signed 64-bit integers share the `Number` type with finite binary64
  decimals. Integer overflow, division by zero, nonfinite results, nonzero
  underflow to zero, and lossy integer-to-decimal arithmetic conversions error.
  Mixed numeric comparisons retain precision. See [numeric rules](docs/NUMBERS.md).
- The explicitly imported `json` module parses and serializes bounded JSON,
  preserving null/Nil and exact integers, with deterministic object keys.
- Existing structured errors, cycle collection, execution limits, cancellation,
  and non-executing syntax checking are included in this source version.

This is a source development version. Matching VSIX and installer validation is
pending; older local artifact reports do not validate these new source changes.
There is no claim of a published release or an updated normal installation.
