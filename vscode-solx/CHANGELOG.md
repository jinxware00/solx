# Changelog

## 0.3.1 (local package; unpublished)

- Avoid the measured Windows stdin startup delay by checking temporary buffer
  snapshots with the existing file-check command. Other platforms retain stdin.
- Remove snapshots after checks, failures, cancellation, and timeouts; prevent
  launches cancelled during snapshot creation and report cleanup failures.
- Wait for active checker cleanup during normal extension shutdown and validate
  that desktop tests leave no snapshots in their isolated temporary directory.
- Add snapshot lifecycle tests and a repeated-check desktop timing regression.
  The CLI, JSON schema, language behavior, and interpreter discovery are unchanged.

## 0.3.0 (local package; unpublished)

- Add live syntax diagnostics using the interpreter's non-executing JSON check
  command, including unsaved/untitled buffers and Unicode byte-range conversion.
- Add Check Current File, status reporting, debounce/timeout settings, trusted
  workspace gating, cancellation, and stale-result suppression.
- Keep checker failures separate from source errors; require the matching
  source-built interpreter rather than falling back to running the buffer.

- Add keyword completion and a snippet for the interpreter's new `try`/`catch`.
- Recognize handler bindings and separate try/catch scopes for API completion
  and hover. Requires the matching source-built interpreter.

## 0.2.1

- Configure the official `jinxware00` publisher and author profile.
- Derive a square, transparent Marketplace icon from the official PNG.
- Validate installed VSIX contents and run the packaged extension in VS Code.
- Handle cancellation while preparing a run and process-stop launch failures.
- Document the bundled Windows installer and manual installation.

## 0.2.0

- Rename the local package to Solx Language Support (`solx-language-support`).
- Add Run Current File and Stop Running File, executable/PATH discovery, and
  a Cargo mode that rebuilds source checkouts before execution.
- Add keyword/API completion, construct snippets, and API hover documentation.
- Add the official Solx logo as a 512 x 512 PNG extension icon.
- Preserve the grammar, editing configuration, and example from 0.1.0.
- Prepare metadata and packaging for manual Marketplace publication; no
  publisher is configured. Live diagnostics remain blocked by the CLI's
  lack of a non-executing parse-only interface.

## 0.1.0

- Initial syntax highlighting, editing configuration, example, preview setup,
  and 10 TextMate validation tests.
