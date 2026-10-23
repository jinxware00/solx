# Changelog

## 0.2.0 — unreleased

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

This is a source development version. Local artifacts are validated separately;
there is no claim of a published release or an updated normal installation.
