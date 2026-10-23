# Static name analysis and declaration symbols

`solx check` still performs syntax checking by default. Two opt-in flags extend
the same non-executing pipeline:

```sh
solx check --names program.solx
solx check --names --json program.solx
solx check --symbols --json program.solx
solx check --names --symbols --json --stdin
```

`--names` reports identifier uses with no declaration in the analyzed lexical
scopes or file globals. `--symbols` returns declaration symbols and requires
`--json`. The flags work independently: requesting symbols alone does not enable
name diagnostics. The CLI returns 0 for a clean result, 1 for source diagnostics,
and 2 for input/option errors, following the existing check command.

Both features use the real lexer and parser. They traverse the validated AST and
associate its identifiers with lexer tokens in source order. They do not parse
expressions with regexes, execute code, read imported modules, inspect the
filesystem for names, or infer runtime types or call arities. The existing check
input limit of 1 MiB and syntax-depth limits remain in force.

## Name rules

The analyzer recognizes `print`, `str` and `num`. Imports declare their explicit
alias or parser-derived name in their block. Other standard-library module names
require imports, as they do at runtime. Because an isolated buffer may be an
imported module, `exports` is conservatively treated as available in every buffer.
This avoids diagnosing a module's implicit binding; it also means using `exports`
in an entry file will not receive an undefined-name diagnostic.

Immediate `var`, `const`, function and import declarations are predeclared for
each lexical block. This permits a closure to capture a local that is declared
later, before the closure eventually executes. All `global` declarations are
collected throughout the file, including nested branches, loops, handlers,
functions and anonymous-function bodies.

Function parameters belong to the function body. Branches, loop bodies, protected
`try` bodies and handlers have separate local scopes. The catch name belongs to
its handler. Numeric and collection loop names belong to the loop body; loop
header expressions are analyzed in the enclosing scope. Sibling blocks and code
after a block do not inherit its locals. Captured enclosing scopes remain visible
to nested functions.

Assignment targets are identifier uses: `missing = 1` is reported unless a
declaration exists. Property names and literal object keys are not uses. For
`object.field`, only `object` is analyzed; for `object[index]`, both expressions
are analyzed. Type-annotation names are not variable uses. Comments, ordinary
strings and raw multiline strings contribute no name uses.

These are conservative declaration checks, not definite-assignment or control-flow
analysis. Predeclaration can miss use-before-definition, self-initialization and
conditional initialization errors. A declaration in a function or branch that
never executes can make a file-global name appear available. An imported module's
members and an object's properties are not validated. Unreachable code is still
analyzed. A standalone buffer has no knowledge of bindings supplied by an external
host or an earlier REPL run. No type or function-arity guarantees are inferred.

At most 100 undefined-name diagnostics are emitted, in source order. Additional
uses are omitted, while symbol collection continues. This cap follows the editor
diagnostic protocol's limit.

## JSON schema

The existing schema version remains 1, with `offsetEncoding: "utf-8"`, `source`,
`ok`, `diagnostics`, and `error` unchanged. Name diagnostics use code
`undefined-name` and severity `error`. Their ranges select the exact identifier
token using half-open UTF-8 byte offsets.

When `--symbols` is present, an additional `symbols` array contains flat entries:

```json
{
  "name": "calculate",
  "kind": "function",
  "range": { "start": 0, "end": 35 },
  "selectionRange": { "start": 3, "end": 12 }
}
```

Offsets depend on the actual input. `range` is the parser's full `Located`
declaration statement span. `selectionRange` is the exact declaration identifier
or explicit import-alias token, contained in that range. An implicit import such
as `import "math"` has no identifier token; its selection range is the complete
string-specifier token, including quotes. This avoids fabricating a name range
inside a possibly escaped string.

Kinds are `function`, `variable`, `constant`, or `module`. The array contains
named function statements, `var`/`global`/`const` statements and imports at all
nesting levels, in source order. A variable initialized with an anonymous function
remains a variable declaration. Parameters, loop-header bindings, catch bindings,
object properties and anonymous functions have no separate symbol entries.

A lexer/parser failure stops analysis and returns the existing first syntax
diagnostic, with no partial name diagnostics or symbols. Input errors retain the
existing `input-error` object. If symbols were requested, both failure cases
include `symbols: []`. Without the symbols flag, the `symbols` field is absent.

## Rust API and validation

`solx::analysis::analyze_source(source, AnalysisOptions { names, symbols })`
returns an `AnalysisReport` or the existing `CheckDiagnostic` for a syntax
failure. The report contains `diagnostics` and `symbols`; each `DeclarationSymbol`
has a `SymbolKind`, full statement `range`, and `selection_range`. The existing
`check_source` and `json_report` APIs retain their behavior.
`check::json_analysis_report` serializes multiple diagnostics and optional symbols
using the shared schema. CLI input-size enforcement remains separate from these
library APIs, as it is for `check_source`.

`tests/analysis.rs` checks scope boundaries, shadowing, later captures, conditional
globals, both loop forms, assignments, property/key exclusions, raw strings,
Unicode byte ranges, declaration spans, malformed input, diagnostic caps, CLI
options and absence of execution/import loading. The existing check CLI suite
guards compatibility.

```sh
cargo test --locked --test analysis --test check_cli
```
