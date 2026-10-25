# Changelog

## 0.1.0

Initial release of **Sol Language Support** under `jinxware00.solx-ls`,
packaged as `solx-ls-0.1.0.vsix`. This replaces the unpublished
`solx-language-support` packages while retaining their implemented features.
Requires VS Code 1.85+ and Solx 0.2 for running, diagnostics and Outline.

- Syntax highlighting, raw multiline strings, collection loops and snippets.
- Completion and hover for implemented keywords and standard-library APIs.
- Run/stop commands, interpreter discovery and source-checkout Cargo support.
- Live syntax checks for saved, unsaved and untitled buffers; optional name checks.
- Parser-derived Outline and Go to Symbol with shared concurrent requests,
  configuration refresh and independent cancellation.
- Windows temporary-buffer transport with bounded checks and shutdown cleanup.
- Workspace trust controls and clear interpreter/protocol errors.

Uninstall any legacy Solx extension before installing this new identity to
avoid duplicate providers. GitHub releases distribute the VSIX; Marketplace
publication is separate.
