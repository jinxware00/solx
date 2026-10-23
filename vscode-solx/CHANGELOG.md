# Changelog

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
