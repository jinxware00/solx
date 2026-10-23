# Checking syntax without execution

```sh
solx check program.solx
solx check --json program.solx
solx check --json --stdin
```

The last form reads UTF-8 source until stdin closes. The checker calls the same
lexer and parser used for execution, with the existing syntax-depth limits.
It never creates an interpreter, executes statements/calls, resolves imports,
or loads imported files. An infinite loop is valid syntax and completes its
check immediately. Undefined names, missing modules, type mismatches, and
runtime failures are outside this check.

Exactly one file or `--stdin` is required. Files and stdin are limited to
1,048,576 UTF-8 bytes (1 MiB). Invalid UTF-8, oversized input, and read failures
are operational failures. Use `--` before file paths starting with a dash.
The new `check` subcommand reserves that bare CLI word; a file literally named
`check` can still be executed as `solx ./check`.

Exit status is 0 for valid syntax, 1 for a lexer/parser/depth-limit diagnostic,
and 2 for input or command-line usage errors. Text mode prints a success line
or an error with the lexer/parser's existing 1-based line/column convention.
Correctly invoked JSON mode writes one JSON object and a newline to stdout,
with no logging on stderr. Clap usage/argument errors still use stderr rather
than the check schema. Cargo's own build messages, if used, also go to stderr.

## JSON schema version 1

For `var x = )` on stdin:

```json
{
  "schemaVersion": 1,
  "offsetEncoding": "utf-8",
  "source": "<stdin>",
  "ok": false,
  "diagnostics": [{
    "code": "parse-error",
    "severity": "error",
    "message": "expected expression, found RightParen",
    "range": { "start": 8, "end": 9 }
  }],
  "error": null
}
```

Object key ordering is not part of the protocol. `source` is the supplied
file path or `<stdin>`; it is a label, not an instruction to open another file.
Ranges are half-open UTF-8 **byte** offsets into the exact input buffer,
not character indexes or UTF-16 offsets. EOF errors have an empty range at
the buffer's end. Lexer errors cover the offending token, including consumed
parts of an unfinished string/escape. Parser errors use their offending token;
AST-depth errors may cover the containing statement.

`diagnostics` currently has zero or one item. Its code is `lex-error`,
`parse-error`, or `syntax-limit`; its severity is `error`. As in normal parsing,
the lexer scans the entire buffer first: a later lexical error takes precedence
over an earlier token sequence that would fail parsing. Error prose is useful
to humans but is not a machine-stable error identifier.

Valid input has `ok: true`, `diagnostics: []`, and `error: null`. Operational
input failures have `ok: false`, `diagnostics: []`, and
`error: {"code": "input-error", "message": "..."}` with exit 2. Clients should
validate schema version, encoding, exit status, and range boundaries. A missing
or unsupported response must not be interpreted as a clean source check.

Rust hosts can call `solx::check::check_source(&str)` and inspect its first
`CheckDiagnostic`, or call the existing parser to obtain an AST. The CLI's
input-size cap belongs to its reader; hosts choosing the library API own their
input-size policy. `LexError` now retains a `span`, and conversion to ParseError
preserves it. Hosts constructing LexError with struct literals must include the
new field; its existing constructor remains available.

## VS Code integration

Solx Language Support 0.3.0 sends the current buffer using
`check --json --stdin`, including unsaved and untitled documents. The configured
executable must support schema v1. Cargo mode rebuilds the configured checkout
before checks; no `target` binary is selected directly. An older installed
interpreter is reported as unavailable for checking, without an execution
fallback. No language parser is duplicated in JavaScript.

The extension checks only trusted local/untitled buffers up to 1 MiB. It
converts validated byte boundaries to UTF-16 offsets and lets VS Code map those
offsets to line positions, handling CRLF and Unicode. Checks are serialized,
debounced by 350 ms, and limited to 30 seconds by default. New edits and
configuration changes cancel older requests; document versions and request
identity prevent late results from replacing newer diagnostics. Close/dispose
clears diagnostics and cancels owned work. Interpreter/protocol/timeout failures
appear in the status bar and output channel, not as invented syntax errors.

Use **Solx: Check Current File** for an immediate check. Settings and limitations
are in [the extension README](../vscode-solx/README.md). These checks are syntax
diagnostics, not a type checker, language server, debugger, or sandbox.

The source changes and 0.3.0 VSIX are separate from earlier installer ZIPs and
installed binaries. Rebuild the interpreter to use this CLI. This milestone
does not publish to the Marketplace or rebuild the installer bundle.

### Windows validation limitation

On the development machine, stdin checks launched outside the command sandbox
incurred roughly 26 seconds of Windows process-startup latency. This occurred
both in desktop VS Code (1.85.2 and 1.143.1) and in standalone Node tests outside
the sandbox. Inside the sandbox, the same checks completed in under a second.
File checks and ordinary execution were substantially faster in the probes.
The cause has not been established; changing argument order or pipe options
did not resolve it. No such workarounds are included in the extension.

This delays diagnostics on this host and needs investigation before release.
The test harness allows ten minutes for the complete desktop suite and records
individual check durations; the production check timeout remains 30 seconds.
See [extension validation](../vscode-solx/VALIDATION.md) for observed results.
