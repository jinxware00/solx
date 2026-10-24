# Runtime reliability audit

**Follow-up:** the historical findings and validation below are preserved.
Automatic cycle collection is now implemented; see
[MEMORY_MANAGEMENT.md](MEMORY_MANAGEMENT.md) for the current ownership contract,
algorithm, tests, and remaining limitations. The earlier leak probes have been
updated to assert reclamation. [Execution and syntax limits plus cancellation](EXECUTION_LIMITS.md)
are also now implemented. Runtime-value destruction is now iterative, including
lists, objects, closure scopes, and module exports; see the 2026-10-24 follow-up
below. Arbitrary host ASTs, native-operation budgets, and synchronous GC pauses
still require separate consideration. The audit below records
the earlier baseline, rather than the current implementation of these guards.

## Follow-up: recursive runtime destruction (2026-10-24)

A rebuilt debug CLI running `var x=nil for i=1,20000 do x=[x] end x=nil` crashed
inside a watched subprocess with stack-overflow status `3221225725`. The source
is shallow and remains within default execution limits: dropping the last list
owner recursively released `Rc -> RefCell -> Vec<Value> -> Rc` descendants.
Parsing/evaluation depth guards cannot prevent that ownership path.

`src/runtime/destruction.rs` now processes uniquely owned list, object, function,
scope, and module nodes iteratively with `Rc::try_unwrap`. `Value::drop` transfers
the last owner into that worklist before releasing its payload. Shared nodes
remain unchanged; cycles still use the reference-count-aware collector. This
uses safe Rust and does not add registry roots, reference leaks, unsafe aliasing,
or delayed acyclic reclamation. The public Rc payload types remain unchanged,
but Rust consumers must borrow enum patterns instead of moving non-Copy payloads
from a Value with Drop. The migration and algorithm invariants are documented in
[MEMORY_MANAGEMENT.md](MEMORY_MANAGEMENT.md#iterative-destruction-2026-10-24).

The failing CLI case passes after the fix. Watched subprocess regressions verify
100,000-level list/mixed-object graphs and 20,000 alternating function/module
layers with scopes on a requested 256 KiB stack. They assert weak-handle expiry,
shared-root preservation, ordinary and cyclic collection, mutations/removal,
host raw-Rc teardown, borrowed-container behavior, and cleanup after returns,
errors, execution limits, and cancellation. The first integrated post-fix run
passed 407 tests and strict Clippy; subsequent integrated receipts, including the
additional cancellation test, are maintained in
[OVERNIGHT_PROGRESS.md](OVERNIGHT_PROGRESS.md).

This closes the demonstrated runtime-value crash; it does not make destruction
interruptible, bound heap allocation or GC pauses, or protect arbitrary manually
constructed Rust ASTs. Syntax/evaluation limits and remaining resource gaps are
documented independently. The older findings and test counts below are retained
as historical audit evidence.

This audit covers the existing tree-walking interpreter. It introduces no
collector, unsafe code, dependencies, bytecode execution, or new language
syntax. The baseline suite contained 137 passing tests. Verification of
this change must use the current tree, not that baseline result.

## Confirmed findings and focused changes

| Finding | Evidence | Disposition |
| --- | --- | --- |
| Collection formatting recursively followed every child; derived Value Debug and PartialEq recursively visited Rc contents. | A list can append itself, and an object can assign itself to a property. `str`, `print`, `..`, equality, and `list.find` reached these implementations. | Fixed with iterative traversal and path/pair identity sets; specified in SPEC 9.4. |
| AST statements lost lexer spans; runtime errors had only a message. Function/module wrappers rebuilt that message and no-file calls lost call context. | Parser output, execute, evaluate_call, run_source, and execute_module. | Parser retains statement spans; errors carry an original message, location, and call/import contexts. |
| Import-base identity and diagnostic identity need different lifetimes. | Interactive source after run_file uses the file directory for imports but its line numbers refer to a new buffer. | Separate diagnostic_file from source_file in captured environments. Definition positions survive escaped functions; interactive positions use `<source>`. |
| Strong reference cycles survive removal of all external roots. | Weak-reference probes in runtime/value.rs upgrade after Interpreter/value/module roots are dropped. | Confirmed leaks, deliberately not claimed fixed by traversal safety. |
| Recursive execution, parsing, AST operations, and destruction can exhaust the Rust stack. | Recursive call chains below, with no depth checks or trampoline. | Remaining limitation; diagnostics improve recoverable errors, not stack exhaustion. |

The collection tests cover direct list/object cycles, indirect mixed
graphs, unequal cycles, equivalent graphs with different cycle lengths,
acyclic sharing, NaN, empty collections, object ordering, borrowed Debug,
and language-level conversion/search. A 2,000-level acyclic graph tests
the explicit work stacks; test teardown holds every node and clears links
so it does not accidentally test recursive destruction. No performance
claim is made: rendering can allocate large strings and repeatedly copy
completed nested output, and repeated acyclic sharing can expand output.

## Ownership graph and actual retention

Environment owns a Vec of strong Rc scope maps plus a strong Rc global
map. Environment clones share those maps. Scope bindings own Values;
UserFunction owns its captured Environment and its AST body. Lists and
objects own child Values. ModuleValue owns its export object; exported
functions capture an environment whose local `exports` binding owns that
same object. The loader strongly owns every successful cached namespace.

Confirmed examples:

- `list -> list`, or `object -> list -> object`: dropping user bindings
  cannot release the internal strong edges.
- `scope -> function -> captured scope`: this occurs even for
  `global f = fn() return 1 end`, with no recursion in the body. A nested
  named function can retain an otherwise unreachable completed call scope.
- `exports -> function -> captured scope -> exports`: the namespace Rc
  itself can be released while its export graph remains alive.

Weak-reference probes prove the first, second, and export-cycle cases;
they explicitly break their own test edges afterward. Acyclic graphs and
functions whose captured environment does not own them are released in
control tests. Scope-sharing tests already prove that escaped closures
must keep their captured values alive and observe later mutations.

Cache retention is intentional reachability, not itself a leak. A mutable
export or closure that is still reachable from a host Value is also live.
Imports that fail are not cached, but may leave unreachable cycles or
publish a still-live closure into an already successful dependency. The
new failed-module escape test demonstrates the latter. Cache snapshots
and cycles must not be conflated when measuring retained memory.

## Collection strategy recommendation

| Strategy | Benefit | Trade-off for this runtime |
| --- | --- | --- |
| Current reference counting | Simple ownership and immediate reclamation of acyclic values. | Cannot collect the confirmed cycles. Weak closure environments would invalidate legitimate escaped closures. |
| Reference counting plus cycle detection | Could retain existing Rc interfaces. | Requires tracking all container/environment edges, external roots, and mutations; partial treatment of just functions misses mixed cycles. Trial deletion is substantial collector work. |
| Tracing collection | Matches mutable cyclic language graphs and reclaims all unreachable cycles. | Needs an explicit heap/root model, collection safe points, and allocation/pause measurements. Every evaluator temporary, closure, scope, cache entry, and host-held value must be represented correctly. |
| Per-interpreter arena | Simple safe handles and bulk teardown; cycle edges need not own allocations. | A long-running REPL retains dead allocations until teardown. Values currently may outlive Interpreter through the public Rust API; borrowed lifetime handles or explicit owned roots would change that API. Arena allocation alone is not reclamation during execution. |

Keep the existing ownership model for this focused change. For a later
collection milestone, prefer a small safe tracing heap backed by checked
generational indices, with explicit root handles for Rust embedders. An
arena can store that heap, while mark/sweep provides reclamation. Specify
root lifetime and collection safe points before replacing Rc edges; test
escaped closures, modules, temporary arguments, error unwinding, and
cyclic mutations under forced collection. Measure allocation rates,
retained heap, and pauses on long-running closure and collection workloads
before deciding on tuning or claiming improvements.

Do not clear captured scopes when a function returns, an import fails, or
an Interpreter drops: public Values and shared dependencies can retain
valid closures. Do not weaken all capture edges. No incomplete collector
or scope-clearing workaround is included here.

## Rust stack and execution limits

The following are visible recursive paths, not measured crash thresholds:

- `evaluate -> evaluate_call -> execute_statements -> execute -> evaluate`
  grows with user calls, including mutual recursion. There are no tail calls.
- Pratt parsing recurses for parentheses, nested literals, unary operators,
  right-associative operators, function bodies, and nested statements.
- Evaluation recurses through expression trees and nested blocks. A long
  left-associative expression can build a deep AST even if its parser loop
  itself uses little stack.
- `Function::clone`, derived AST Debug/PartialEq, and Box/Vec AST destruction
  recurse through syntax. Rc destruction of a deep acyclic value graph can
  likewise recursively release descendants.
- An acyclic chain of distinct file imports recursively parses/executes
  modules. Circular-import detection does not bound that chain's depth.

No arbitrary default limit was added: it would restrict existing programs
without a portable guarantee on host-thread stack size. A future limit
design needs parser nesting **and AST height** checks, evaluator/call/block
and module-load budgets, restoration on every error, and consideration of
clone/drop paths. A call counter alone would not prevent all stack
overflows. Nonterminating while loops and allocation/output exhaustion
also need independent fuel/resource policies. Stack overflow is not a
recoverable RuntimeError in this implementation. Tests here use finite
inputs; they do not deliberately overflow a thread or run unbounded loops.

## Module loader conclusions

- Canonical absolute paths define cache identity. Existing tests cover
  aliases/normalized paths; a Unix-only test covers symlink aliases and is
  not executed on Windows. Hard links are distinct paths/identities.
- The loading stack detects direct, indirect, and entry-file cycles and
  is removed after read, parse, and initialization failures. Failed
  initialization can be retried; successful dependencies remain cached.
- Cached modules intentionally stay stale after source edits. The new
  snapshot test also checks deletion, restoration, and a fresh Interpreter.
  Every reimport must still canonicalize the requested path. Neither
  identity rules nor hot-reload semantics were silently changed.
- Imported environments have separate globals and fresh builtins. Returned
  functions resolve later imports from their defining source_file. The
  missing-import/repair test exercises an escaped nested closure.
- Successful side effects before failure are preserved. A new test retains
  a closure through a successful dependency and retries the failed parent.
- Explicit run_file is not transactional: after resolution/read/cycle
  checks, its selected import base persists even on a parse/runtime error.
  Imports restore their caller context. SPEC now makes this distinction
  explicit rather than implying rollback of all state.

No additional cache-corruption or isolation bug was reproduced. Source
locations and structured error propagation are the loader-facing fixes.
File resolution/read still has the usual filesystem race window, caching
does not validate content hashes, and imports are not a security sandbox.

## Diagnostic scope and compatibility

Runtime locations identify the start of the failing **statement**, not
the precise operator or nested call expression. Lexical/parse errors retain
their token positions. Call and import frames preserve the original error
location and unwind innermost first. Source excerpts and expression spans
remain future work.

Parser-produced ASTs now use `Stmt::Located`; Rust clients can inspect or
mutate the enclosed statement via `unlocated`/`unlocated_mut`. Manually
constructed bare statements still execute. `RuntimeError.message` is the
undecorated original message; Rust callers needing file/call context should
use its structured fields or Display. Syntax-shape tests and CLI diagnostic
assertions were updated while retaining their original behavioral checks.

## Verification and changed files

Verified on Windows for this change:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Exit 0. |
| `cargo test` | Exit 0; 159 passed, 0 failed, 0 ignored. |
| `cargo clippy --all-targets --all-features -- -D warnings` | Exit 0. |
| `cargo build --release` | Exit 0; optimized release build completed. |
| `git diff --check` | Exit 0; no whitespace errors. |

The 22 added tests comprise eight collection tests, four ownership probes,
four diagnostic tests, and six module/CLI tests. Existing syntax and CLI
tests retain their behavioral assertions with updated AST/diagnostic
expectations. The Unix-only symlink test was not compiled on Windows.
Cargo emitted `warn: could not canonicalize path <user-profile>`;
Git emitted LF-to-CRLF working-copy notices. Neither made these commands
fail. During development, two test invocations exited 1 on diagnostic
expectations/import-origin context; those issues were corrected before
the successful full run above.

Files changed by this task (19):

- `docs/SPEC.md`, `docs/RUNTIME_AUDIT.md` (new).
- `src/runtime/collection_ops.rs` (new), `src/runtime/value.rs`,
  `src/runtime/environment.rs`, `src/runtime/error.rs`, `src/runtime/mod.rs`.
- `src/parser/ast.rs`, `src/parser/mod.rs`, `src/interpreter/eval.rs`,
  `src/modules.rs`.
- `tests/cyclic_values.rs` (new), `tests/diagnostics.rs` (new),
  `tests/modules.rs`, `tests/functions_cli.rs`, `tests/function_syntax.rs`,
  `tests/import_syntax.rs`, `tests/loop_syntax.rs`, `tests/loops.rs`.

The final diff was reviewed. Pre-existing changes to `assets/logo.svg` and
the untracked `assets/logo.png` were preserved; both asset hashes and the
Cargo.toml/Cargo.lock hashes match the audit start. No commit or push was
performed.

## Follow-up: host-held RefCell borrows (2026-10-23)

This follow-up starts after the collection-equality fix. It audits every direct
`borrow()`/`borrow_mut()` call in production `src/`, and preserves the existing
ownership model. There are **24 production sites**: 19 host-triggerable access
panics reproduced before the changes below, and five private scope-map accesses
left unchanged. Four additional calls inside value.rs's `#[cfg(test)]` ownership
probes are test construction/cleanup, not production access paths.

Classification uses the actual API: public List/Object Rc payloads allow a host
to keep a Ref/RefMut while calling the interpreter again. Module export maps are
private fields, but `exports.exposed = exports` can publish the same map as an
ordinary Object. Private Environment maps contain a different, private Binding
type and are not exposed by `get`, `get_global`, or Environment cloning.

The table identifies original call sites by function/branch, not unstable line
numbers. Test names refer to `tests/borrow_conflicts.rs` except the module test.
H means a **demonstrated host-triggerable panic**, now an ordinary RuntimeError.
S means **safe under current invariants**, unchanged; it is not a claim about
future callbacks, custom destructors or private-map exposure.

| Original direct borrow occurrence | Class and evidence / disposition |
| --- | --- |
| eval.rs `evaluate`, Length List read | H: `list_length` |
| eval.rs `evaluate`, Length Object read | H: `object_length` |
| eval.rs `read_index`, List read | H: `list_index` |
| eval.rs `read_index`, Object read | H: `object_index` |
| eval.rs `read_property`, Object read | H: `object_property` |
| eval.rs `assign_target`, Property write | H: `property_assignment` |
| eval.rs `assign_target`, List index write | H: `list_assignment` |
| eval.rs `assign_target`, Object index write | H: `object_assignment` |
| stdlib/collections.rs `list.len` read | H: `native_list_length` |
| stdlib/collections.rs `object.has` read | H: `native_object_has` |
| stdlib/collections.rs `append` write | H: `native_append` |
| stdlib/collections.rs `remove_item` length read | H: `native_remove_length_read` |
| stdlib/collections.rs `remove_item` removal write | H: `native_remove` with shared host borrow |
| stdlib/collections.rs `find` read | H: `native_list_find` |
| stdlib/collections.rs `sort` write | H: `native_sort` |
| stdlib/collections.rs `keys` read | H: `native_object_keys` |
| stdlib/collections.rs `remove_property` write | H: `native_object_remove` |
| stdlib/strings.rs `join` read | H: `native_string_join` |
| value.rs `ModuleValue::get` exports read | H: `tests/modules.rs::host_borrow_of_exposed_exports_reports_errors_without_losing_the_cache` |
| environment.rs `define`, target write | S: private map, type validation before guard, no user code or callbacks under guard |
| environment.rs `get`, lexical scope read | S: private map; clones an owned Value, exposes no scope borrow |
| environment.rs `get`, globals read | S: same invariant as lexical read |
| environment.rs `assign`, scope write | S: private map; const/type checks and edge replacement/removal do not reenter evaluation |
| environment.rs `get_global`, globals read | S: clone-only access, no map/guard escapes |

No interpreter-only borrow panic was reproduced. Arguments/RHS/index expressions
finish evaluating before collection mutation guards begin. Native sorting uses
only its validated scalar comparator. Search may compare cyclic elements while
holding a shared list borrow; further shared reads are legal. Append can clone
the target Value into itself without reborrowing its contents. New cyclic
mutation/search and shared-read tests exercise these cases. This is source-backed
coverage, not an exhaustive proof for all possible future execution paths.

The four test-only borrows in value.rs (`items` self-edge insertion, retained-list
cleanup, exports-function insertion, retained-exports cleanup) have test-local
guards with no conflicting access; their existing tests still run in the suite.
`ModuleLoader` uses a plain HashMap/Vec, not RefCell. Cache hits clone namespaces
without borrowing exports, and the new test verifies that distinction.

### Implemented access policy and error restoration

`collection_ops::{read_collection,write_collection}` are crate-private helpers
used only at the 19 demonstrated sites. A read conflict returns
`<operation>: collection is mutably borrowed`; a write conflict returns
`<operation>: collection is already borrowed`. Operations include native qualified
names and evaluator descriptions such as `list indexing` or `module export access`.
No missing value, empty container or successful skipped mutation is fabricated.
Guards retain their existing lifetimes. Type/arity/index validation order is kept,
including list.remove's separate length read before index validation and write.

All mutation tests exercise both shared and mutable host guards, check unchanged
contents on failure and successful mutation after release. Read tests check
specific errors, no partial declaration and correct results after release.
Additional assertions cover nested calls/loop scope restoration, source positions
and call contexts; module cache reuse; failed module initialization and successful
retry after releasing the host borrow. Previously these panics bypassed the
Result-based restoration code. The fix lets that existing code execute normally.
It does not add universal panic recovery or roll back earlier successful effects.

### Equality and formatting: deliberate infallible behavior

The completed equality fix remains unchanged: a comparison needing contents of
a mutably borrowed collection returns false, even for aliases. `!=` therefore
returns true for that comparison. Releasing the guard restores structural
comparison; NaN remains unequal even inside the same collection. The policy is
documented on `Value`'s PartialEq implementation and covered by the six previously
added cyclic-value regressions. Formatting/Debug retain the existing `<borrowed>`
placeholder and cycle markers. Their fallible borrows are already handled; they
are not replaced by error-returning helpers because their APIs do not return a
RuntimeError. Native inputs that require actual collection contents do return errors.

A compatible additive `Value::try_equal` returning a result could distinguish
unavailable contents from inequality without changing PartialEq. It is feasible
without a new interpreter, but would require deliberate propagation through the
iterative comparator and a separate caller contract. No demonstrated new equality
defect requires that API in this patch. Returning true by Rc identity is not a
substitute: it would skip NaN/child comparison. Blocking/retrying while the same
thread holds RefMut cannot resolve the borrow. Keep the tested fallback for now.

The directly affected GC comparison assertion and its explanatory text now require
temporary false, not the old panic. The old ROOTING_GAPS equality note is qualified
with the subsequent fix. No other GC model, benchmark output or migration design
is changed; historical command results stay historical.

Unresolved risks: hosts can still panic by directly misusing raw Rc/RefCell APIs;
borrow guards can be deliberately leaked; future native callbacks/private-map
exposure would require another audit. OOM, stack exhaustion, recursive drop and
unreachable Rc cycles are outside this change. These fixes do not establish a
collector, execution budget or arbitrary panic/unwind safety.

### Follow-up verification

Commands ran from the repository root. For the GC-only commands below, `$M`
means `experiments/gc-prototype/comparison/Cargo.toml` and `$T` means
`target/borrow-hardening-20261023-144831-627/comparison`. These are path
abbreviations, not new configuration. All Cargo commands were offline/locked.

| Command | Actual outcome |
| --- | --- |
| `cargo test --offline --locked --test borrow_conflicts` before fix | Exit 1: 2 passed, 19 panicked. Includes 18 distinct exposed collection sites plus nested-call regression. |
| `cargo test --offline --locked --test modules host_borrow_of_exposed_exports` before fix | Exit 1: 1 panic at the nineteenth site, ModuleValue::get. |
| `cargo test --manifest-path $M --offline --locked --target-dir $T --test audit public_rc_borrows_can_make_equality_panic_without_corrupting_values` before expectation update | Exit 1: stale `result.is_err()` assertion failed because equality no longer panics. |
| `rustfmt --edition 2024 src/runtime/collection_ops.rs src/runtime/mod.rs src/runtime/value.rs src/interpreter/eval.rs src/stdlib/collections.rs src/stdlib/strings.rs tests/borrow_conflicts.rs tests/modules.rs experiments/gc-prototype/comparison/tests/audit.rs` | Exit 0. A later `rustfmt --edition 2024 tests/modules.rs` also exited 0 after adding retry assertions. |
| `cargo test --offline --locked --test borrow_conflicts --test modules --test cyclic_values --test diagnostics --test stdlib` after fix | Exit 0: 86 passed, zero failed. |
| `cargo test --offline --locked` | Exit 0: 187 passed, zero failed/ignored, including the final module-retry assertions. |
| `cargo fmt --check` | Exit 0. |
| `cargo clippy --offline --locked --all-targets --all-features -- -D warnings` | Exit 0. |
| `cargo test --manifest-path $M --offline --locked --target-dir $T --test audit public_rc_borrows_make_equality_temporarily_false_without_corrupting_values` | Exit 0: 1 passed, 6 unrelated tests filtered out. No benchmark suite run. |
| `cargo clippy --manifest-path $M --offline --locked --target-dir $T --test audit -- -D warnings` | Exit 0. |
| `rustfmt --edition 2024 --check experiments/gc-prototype/comparison/tests/audit.rs` | Exit 0. |
| `git --no-optional-locks diff --check` | Exit 0. New untracked text files also checked separately for trailing whitespace. |

One patch attempt failed to match a test block after rustfmt expanded it; no
changes were applied by that attempt. Reading the current block and reapplying
the intended edit succeeded. No post-fix test, format or Clippy failure occurred.
Existing Rust toolchain canonicalization warnings and Git LF-to-CRLF notices did
not affect successful exit codes. No release/benchmark performance claim is made.

The pre-edit snapshot covered 179 tracked/nonignored files. Hash comparison found
only the 11 intended existing files changed; `tests/borrow_conflicts.rs` is the
only new nonignored file. All 168 unrelated hashes, HEAD, the Git index, and
unrelated Git status entries match the baseline. The prior equality body, all
original module test content, and this audit's original text are preserved
byte-for-byte. Scoped diffs were reviewed against pre-edit copies. No unsafe Rust,
dependency changes, commits, pushes or Git cleanup operations were introduced.
