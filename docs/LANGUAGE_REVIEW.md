# Language review and next priorities

Solx is not feature-complete. The current implementation supports lexical
variables, binding constants and runtime variable annotations, functions and
closures, numeric/collection/while loops, separate lists and objects, explicit
cached modules, and a standard library with bounded JSON, string utilities and
callback collection operations. Structured runtime errors, automatic cycle
collection, iterative runtime graph destruction, raw multiline strings, exact
integers and storable nil are implemented. [Execution controls](RESOURCE_BUDGETS.md)
include optional deadlines, source limits, and language formatting byte/work
budgets; [static analysis](ANALYSIS.md) adds optional name checks and
declaration symbols. These changes do not establish performance or reliability
superiority over Lua or Luau.

The matching runtime, VSIX and installer have passed local Windows validation;
see [the current record](OVERNIGHT_PROGRESS.md) for exact versions and receipts.
Historical results below describe the original try/catch milestone, not
validation of every current feature or support on other operating systems.

## Official Lua source reviewed

Reviewed on 2026-10-23 at commit
[`8cb93bada561977eb370819e39d7d644e5fd0a3f`](https://github.com/lua/lua/tree/8cb93bada561977eb370819e39d7d644e5fd0a3f).
The official repository describes itself as Lua development code, mirrored
irregularly; this snapshot is not a claim about a particular stable release.

- [`lbaselib.c`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/lbaselib.c#L454):
  `finishpcall`, `luaB_pcall`, and `luaB_xpcall` provide protected calls,
  status/results, and an optional message handler.
- [`ldo.c`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/ldo.c#L1078):
  `luaD_pcall` restores execution state after failure. Solx applies that
  principle through its existing Rust Result and explicit scope restoration.
- [`manual/manual.of`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/manual/manual.of#L344):
  error objects and message handlers; the same manual documents numeric
  subtypes, byte strings, multiple results, collection, and resource closing.
- [`testes/errors.lua`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/testes/errors.lua)
  and [`testes/attrib.lua`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/testes/attrib.lua#L160):
  regression checks include error messages, malformed source, and protected
  module loading with preserved side effects.
- [`lgc.c`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/lgc.c),
  [`lvm.c`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/lvm.c),
  and [`lutf8lib.c`](https://github.com/lua/lua/blob/8cb93bada561977eb370819e39d7d644e5fd0a3f/lutf8lib.c):
  reference points for tracing collection, bytecode execution/tail calls, and
  the distinction between byte strings and Unicode operations. These systems
  were surveyed, not ported or benchmarked.

No Lua source was copied into Solx, no Lua dependency was added, and Lua's
test suite was not run. The implementation and tests here are written for
Solx's existing semantics. Luau was not audited in this change; differences
should be verified against its implementation before making comparisons.

## Implemented here: structured error recovery

Lua already has useful protected execution. Solx's existing specification
also planned `try`/`catch`, so the focused addition is direct block syntax
with a consistent error record, without depending on unimplemented multiple
returns or requiring callers to parse formatted error strings.

```solx
try
    var score = num("invalid")
    print(score)
catch err
    print(err.message)
    if err.location ~= nil then
        print(err.location.file, err.location.line, err.location.column)
    end
end
print("Program continues")
```

`err.trace` contains the existing innermost-first call/import contexts.
Handlers run in their own scope; completed side effects remain. Return and
loop-control jumps pass through normally. Handler failures propagate outward.
The current source buffer must parse before execution; an importer can catch
an imported buffer's ordinary load/parse failure. Interpreter limit failures and
cooperative cancellation bypass handlers. Full rules and limitations are in
[SPEC section 12](SPEC.md#12-error-handling).

The new tests exercise real parser/runtime failures, source positions,
handler failures, scope restoration, captured locals, argument side effects,
loop/return unwinding, failed-import retry, host borrow errors, CLI exit codes,
and multiline REPL input. VS Code source support adds a snippet and keywords
and preserves API shadowing inside handlers. No diagnostics server is added.

These are unreleased source milestones. Build the current code with Cargo to
use them; an earlier ZIP/VSIX test receipt does not validate new source changes.

## What should make Solx better

Some useful distinctions already exist: undeclared assignment is rejected;
lists and objects have separate representations; list length counts stored
entries including nil; string APIs consistently use Unicode scalar indexes;
and file globals are isolated. These choices need clear rules and regression
tests more than extra syntax. Unicode scalars are still not user-perceived
grapheme clusters. Version 0.2 makes nil storable and adds exact checked integer
arithmetic; see [NIL_MIGRATION.md](NIL_MIGRATION.md) and [NUMBERS.md](NUMBERS.md).

| Priority | Improvement | Why and acceptance criteria |
| --- | --- | --- |
| 1 | Profile and harden memory management | Cycle collection and iterative runtime graph destruction preserve existing Rc host roots, closures, and module exports. Measure retained memory and pause distributions with bounded workloads; add allocation budgets and evaluate future heap representations. See [MEMORY_MANAGEMENT.md](MEMORY_MANAGEMENT.md). |
| 2 | Move toward an explicit VM stack and broader resource budgets | Depth/step limits, cancellation, optional monotonic deadlines, bounded source reads, and language formatting byte/work limits are implemented. Other native operations, diagnostic output, allocation, GC, and teardown still lack general budgets. Profile these before choosing an explicit stack/bytecode VM and tail calls. See [RESOURCE_BUDGETS.md](RESOURCE_BUDGETS.md). |
| 3 | Extend the settled Number and nil foundations | Exact i64 integers, checked arithmetic, exact mixed comparisons, and storable nil are implemented. Optional/union annotations and multiple-result rules remain open; retain explicit migration notes for further compatibility changes. |
| 4 | Better errors and editor checking | Syntax checks, optional conservative name analysis and declaration symbols are implemented; see [CHECKING.md](CHECKING.md). Next add expression ranges, error categories/causes, structured rethrow, stronger name/signature analysis and navigation. Existing variable annotations check at runtime. |
| 5 | Build on practical iteration and library APIs | List/object snapshot iteration and map/filter/fold/custom sorting are implemented with explicit mutation, callback-failure and budget rules. Expand APIs only with concrete use cases; see [ITERATION.md](ITERATION.md) and [COLLECTION_APIS.md](COLLECTION_APIS.md). |
| 6 | Resource handling and I/O | Design deterministic cleanup (`defer`/resource scopes) alongside explicit file/OS capabilities. Define cleanup failures and cancellation before adding filesystem/network/process APIs. Lua's closing mechanism is a useful reference; arbitrary ambient access is not required for Solx. |
| 7 | Strings, results, and packages | Raw multiline strings, literal replacement and scalar/byte conversions are implemented. Specify interpolation escaping and multiple results; add grapheme operations if needed. A bounded local manifest/entry validator can begin the proposed package tooling without downloads or execution hooks. |

Coroutines/async, metaprogramming, classes, and a JIT require concrete use
cases and measurements. They should not displace memory correctness, precise
semantics, diagnostics, or a useful standard library. Nothing here claims to
fix every issue in Lua or Luau.

The formatting limits are a compatibility restriction: large conversions,
concatenations, or accumulated formatting work can fail with a noncatchable
resource error. Hosts can configure the limits. Host Display/Debug retain their
unbounded APIs; they are not a fallback for bounded script execution. Runtime
destruction is iterative, while evaluator/parser and function AST paths remain
recursive. These changes do not establish a security sandbox or a hard memory
or latency guarantee. Current integration results are recorded separately in
[OVERNIGHT_PROGRESS.md](OVERNIGHT_PROGRESS.md).

## Historical validation and change scope: try/catch milestone

These figures record the original try/catch change only. They are not current
suite totals or final package validation. Baseline: 187 Rust tests and 34
extension tests passed before that change. Observed afterward on Windows:

| Command | Observed result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --locked --quiet` | 209 tests passed, including 22 new error-handling tests |
| `cargo clippy --all-targets --locked -- -D warnings` | Passed |
| `npm test` in `vscode-solx` | 35 tests passed |
| `npm run test:interpreter` in `vscode-solx` | 5 tests passed, including execution of every snippet default |
| `git diff --check` | Passed |

The CLI/REPL tests launch the real rebuilt interpreter. No GUI extension-host,
installer, or packaging tests were rerun for this source-only change; previous
package validation does not validate these new sources. Tests run on Windows
do not establish cross-platform compatibility. Rustup emitted a host-path
canonicalization warning; the checks themselves completed successfully.

Files added or edited for this change:

- Language implementation: `src/parser/ast.rs`, `src/parser/mod.rs`,
  `src/interpreter/eval.rs`, `src/runtime/error.rs`.
- Tests/example: `tests/error_handling.rs`, `tests/error_handling_syntax.rs`,
  `tests/error_handling_cli.rs`, `examples/error_handling.solx`.
- Documentation: `README.md`, `docs/SPEC.md`, `docs/LANGUAGE_REVIEW.md`.
- Editor support: `vscode-solx/src/catalog.cjs`, `vscode-solx/src/language.cjs`,
  `vscode-solx/data/snippets.json`, `vscode-solx/test/language.test.cjs`,
  `vscode-solx/README.md`, `vscode-solx/CHANGELOG.md`.

The parser, interpreter, error type, and specification already contained local
work; additions were made on top of that work. Unrelated existing changes and
assets were preserved. No dependencies or version numbers changed. At the end
of implementation validation, Git's index and HEAD were unchanged; staging
and committing were left for a separate user-authorized step.
