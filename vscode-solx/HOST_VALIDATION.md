# Packaged editor validation

## Sol Language Support 0.1.0 - 2026-10-25

Extension identity: `jinxware00.solx-ls`. Display name: **Sol Language Support**.
The current package is `solx-ls-0.1.0.vsix`, 113,047 bytes, SHA-256:

```text
1e1b5e107621b7a05b3ded0bb0bfcce43d49f6a89b60595ad86bfd800e307460
```

Packaging passed all **77 unit tests**. The packaged extension then passed all
**18 desktop checks on VS Code 1.85.2**, using an isolated editor profile and
extension directory. Installed manifest fields and shipped content matched the
source. No temporary Solx snapshots remained after shutdown.

| Evidence | Local receipt |
| --- | --- |
| Packaging and prepublish tests | `target/validation/release-ls-package/` |
| Bounded desktop run | `target/validation/release-ls-host-1852/` |
| Per-check results and installed bytes | `vscode-solx/.test-artifacts/host-cDfCqh/` |
| Successful package hash receipt | `vscode-solx/.test-artifacts/package-validation.json` |

The desktop run completed in 36,789 ms with 659,750,912 peak committed job bytes
under a 1,536 MiB cap. Repeated warm syntax checks took 1,751 / 1,752 / 1,757 ms.
These are observed timings, not a general performance guarantee.

The subsequent local VS Code 1.143.1 attempt was interrupted by a host restart
and has no completed result. It is not a pass. Additional desktop execution is
deferred to hosted CI, which requires both pinned versions before publication.

## Coverage

The host suite exercises activation, language association, completion and hover,
negative contexts, snippet insertion, Cargo-backed run/stop commands, parser
errors with Unicode/CRLF ranges, unsaved/untitled diagnostics, non-execution,
configuration changes, repeated checks, Outline declarations and unchanged-buffer
disable/re-enable, optional name analysis, raw strings, missing interpreters,
and cleanup. Outline unit regressions cover shared requests, independent
cancellation, provider replacement, trust refresh and manual-check priority.

The harness retains bounded logs, enforces a command deadline and stops only
its own editor process tree. Windows tests use disposable profile, extension,
workspace and temporary directories. Local executable workloads require the
[resource guard](../scripts/GUARDED_VALIDATION.md); its limits cannot prevent
operating-system or driver faults. Normal profiles and installations are not
test targets.

## Historical compatibility evidence

Before the rename, `solx-language-support` 0.4.0 passed 77 unit tests, 8 real
interpreter integrations and 18 packaged checks on both VS Code 1.85.2 and
1.143.1. The legacy VSIX was 113,951 bytes, SHA-256
`5bd6c016cb9ae88a8f8e13a6cd83573695c4d049d95d22c90714c40a32e375ab`.
Receipts are under `target/validation/resume-*` and the isolated host directories
`host-QgpObQ` and `host-z2sCHN`. These results validate those earlier bytes, not
the renamed package. Older milestone reports are in [VALIDATION.md](VALIDATION.md).

Linux/macOS desktop hosts, remote hosts, Marketplace publication and fresh-machine
Windows PATH integration remain outside the local evidence. The release workflow
provides fresh Rust and non-desktop extension checks on all three operating
systems; consult the actual Actions results for release coverage.
