# Solx Language Specification

**Status:** Working specification\
**File extension:** `.solx`\
**Implementation language:** Rust\
**Purpose:** A beginner-friendly, Lua-inspired language designed for
high execution speed and low memory usage.

This document is the authoritative record of decisions made so far.
Confirmed decisions are separated from unresolved design questions.
Examples illustrate intended syntax; where a rule is unresolved, the
example is not a final guarantee.

## 1. Vision and goals

Solx aims to feel familiar to Lua programmers while making selected
language behaviours clearer and supporting modern runtime implementation
techniques.

Priorities: 1. High execution speed. 2. Low memory usage. 3. Readable,
approachable syntax. 4. Useful diagnostics and predictable rules. 5. A
compact runtime that can be benchmarked and improved.

The ambition is to outperform Lua significantly on defined workloads,
but no speedup is guaranteed until measurements support it. Benchmarks
should compare Solx with standard Lua and, where practical, LuaJIT.
Record execution time, startup time, and memory use on equivalent
workloads.

## 2. Implementation strategy

-   Implement Solx in Rust.
-   Begin with a working interpreter and tests.
-   Move toward bytecode execution / a virtual machine after the
    language basics work.
-   Consider a JIT only later, if profiling shows it is worth the
    complexity.
-   Use a compact internal value representation from the beginning,
    measuring trade-offs rather than assuming a particular
    representation is automatically faster.
-   Keep strings, lists, objects, and closures memory-safe.
-   Use automatic garbage collection. The current runtime supplements
    reference counting with graph-based cycle collection (section 9.5).
    Future collector choices need measurements of overhead and pause times.

A tree-walking interpreter is a practical starting point, not the
assumed final performance architecture.

## 3. Syntax and general style

Solx follows Lua-style syntax where possible: - `if`, `elseif`, `else`,
`then`, and `end` - `for`, `while`, `do`, `break`, and `continue` - `fn`
for function declarations and anonymous functions - No mandatory
semicolons - Braces `{}` are used for object literals, not as
replacements for Lua's block syntax.

Example:

``` lua
if age >= 18 then
    print("Adult")
elseif age >= 13 then
    print("Teenager")
else
    print("Child")
end
```

## 4. Variables and scope

### 4.1 Variable declarations

-   `var` declares a variable.
-   Assigning to a declared variable updates it.
-   Assigning to an undeclared name does not implicitly declare it.
-   Redeclaring `var` in the same scope replaces the existing variable's
    value.
-   Variables are dynamically typed unless an optional type annotation
    is present.

``` lua
var score = 50
score = 100

var score = 20 -- Same-scope redeclaration updates the existing value.
```

### 4.2 Globals

`global` declares a file-wide variable that is accessible inside
functions in that file.

``` lua
global score = 100

fn getScore()
    return score
end
```

Globals belong to their own file. Importing a file does not merge its
global namespace with the importing file's globals. Separate files may
have globals with the same name without colliding. Values are shared
explicitly through the module's `exports` object (section 13).

### 4.3 Constants

`const` declares a value that cannot be reassigned.

``` lua
const pi = 3.14159
```

The exact behaviour of mutating a collection held by a constant still
needs to be specified.

### 4.4 Lexical scope and shadowing

Scopes are lexical. A local declaration inside a nested scope can shadow
a name from an outer scope. Assignments update the binding selected by
normal scope resolution.

### 4.5 Undeclared variables

Reading an undeclared variable triggers a warning and then an error when
its value is actually used. A spelling mistake should not silently
become a new variable.

## 5. `nil` and deletion

Assigning `nil` to a variable removes that variable's binding from its
current scope.

``` lua
var score = 100
score = nil
```

After this assignment, reading `score` follows the undeclared-variable
behaviour: warn, then error when the value is used.

**Important consequence:** under this rule, `nil` is a deletion
mechanism rather than a value that can remain stored in a variable. This
is a deliberate departure from Lua and needs careful implementation,
especially for optional typed variables, collection entries, function
results, and missing-property handling. The interaction of these cases
is still unresolved.

## 6. Types and values

### 6.1 Dynamic typing

Variables are dynamically typed by default and can hold values of
different types over time.

``` lua
var value = 100
value = "hello"
value = true
```

### 6.2 Number

Solx has one language-level `Number` type for integers and decimals. The
runtime may use different internal representations or optimisations,
provided the observable behaviour is consistent.

### 6.3 Optional type annotations

Type annotations are optional.

``` lua
var score: Number = 100
var username: String = "Jinx"
var active: Boolean = true
```

-   Unannotated variables may change type freely.
-   Annotated variables must hold compatible values.
-   Initially, assigning an incompatible value produces a runtime error.
-   Type annotations may provide information for optimisation, but no
    performance benefit is assumed without benchmarks.

The complete type grammar, supported types, function parameter/return
annotations, and the interaction between annotations and `nil` remain
unresolved.

### 6.4 Truthiness

Solx uses Lua-style truthiness: - `false` is false. - `nil` is false. -
Everything else is true, including `0`, `""`, and empty lists.

## 7. Operators

Solx uses Lua's operator set and precedence rules.

  Purpose         Operators
  --------------- ------------------------------------
  Arithmetic      `+`, `-`, `*`, `/`, `//`, `%`, `^`
  Comparison      `==`, `~=`, `<`, `>`, `<=`, `>=`
  Logical         `and`, `or`, `not`
  Concatenation   `..`
  Length          `#`
  Assignment      `=`

-   Use `~=` for not-equal, not `!=`.
-   `+` is numeric addition only. Incompatible combinations such as
    `"10" + 5` produce an error.
-   `..` concatenates text and converts supported values to strings.
-   `num()` converts supported values to numbers.
-   `str()` converts supported values to strings.
-   `print()` remains available.

Detailed conversion rules, equality semantics across different types,
numeric edge cases, and the precise meaning of `#` for every supported
collection type remain to be specified.

## 8. Strings and interpolation

Concatenation is implemented. String interpolation is planned; the current
interpreter treats braces inside strings as literal text. The interpolation
example below illustrates the intended future syntax.

``` lua
var name = "Jinx"
var score = 100

print("Player: " .. name)
print("Player: {name}, Score: {score}")
```

Expressions inside interpolation braces are evaluated and their values
inserted into the string. Exact escaping rules for literal braces,
quotes, nested expressions, and multiline strings remain unresolved.

## 9. Lists and objects

Solx has separate list and object types.

### 9.1 Lists

Lists are ordered and use 1-based indexing.

``` lua
var items = ["Sword", "Shield", "Potion"]
print(items[1]) -- Sword
```

Accessing an index outside the valid list range produces a warning and
returns `nil`.

### 9.2 Objects

Objects store named properties.

``` lua
var player = {
    name = "Jinx",
    score = 100
}
```

Both dot and bracket property access are supported:

``` lua
print(player.name)
print(player["score"])

var property = "name"
print(player[property])
```

Accessing a missing object property produces a warning and returns
`nil`.

### 9.3 Collection questions still open

-   Whether negative, zero, non-integer, or non-number list indexes are
    errors or missing-index warnings.
-   Whether list/object assignment can create new entries or replace
    existing ones in every case.
-   How `nil` behaves when assigned to an object property or list slot,
    given that variable assignment of `nil` deletes the binding.
-   Collection mutation methods and standard operations.
-   Equality and length semantics for lists and objects.

### 9.4 Cyclic collections (implemented)

Lists and objects may contain themselves, directly or through other
collections. `str`, `print`, concatenation, Display, and Rust debugging
render a back-reference on the current traversal path as `<cycle>`.
For example, a list containing itself prints `[<cycle>]`; an object with
`self` referring to itself prints `{self = <cycle>}`. Repeated sharing
without a cycle still prints each occurrence in full. Ordinary Solx
formatting, including sorting rendered object entries, is unchanged.
The marker is diagnostic text, not a new literal or serialization format.

Collection equality compares structural contents: matching list lengths
and ordered elements, or matching object keys and values. A previously
visited pair of collection identities is considered already checked;
all other reachable pairs must match. Thus a one-node self-cycle and a
two-node cycle with the same infinite structural unfolding compare equal.
Sharing topology does not affect equality. Scalar rules remain unchanged,
including `NaN ~= NaN` even inside the same collection; functions and
modules retain identity equality. `list.find` uses these same rules.

Formatting, Debug, and equality use explicit traversal stacks. Debug
quotes strings and keeps functions/modules opaque. A collection currently
mutably borrowed by a Rust host renders as `<borrowed>`; ordinary Solx
execution does not expose such borrows. These operations do not themselves
collect cycles or guarantee bounded output size. Collection checkpoints are
described below. Ordinary acyclic graph destruction is still recursive and
has separate limits; see `RUNTIME_AUDIT.md`.

### 9.5 Memory management (implemented)

Values retain the existing reference-counted representation. An automatic
cycle collector traces registered lists, objects, closure scopes, functions,
and module namespaces. It counts internal references and treats remaining
strong references as roots. Active interpreters, successful module caches,
temporary expression/call results, and values held by Rust callers therefore
remain live. Weak references do not keep a value alive. An exports object
and an alias exposing that same object are one graph node.

Collection checks run at statement and block/loop checkpoints after allocation
pressure reaches a threshold. Executed-program completion (including runtime
errors) and interpreter destruction also collect. The initial threshold is
512 registered allocations; after a pass it is at least the retained node
count. This is an implementation policy, not a language timing guarantee.
Collection does not change identity, mutate reachable values, run Solx code,
or clear live captured scopes. Completed side effects and cache behavior stay
unchanged. There are no finalizers or user-visible collection callbacks.

The collector is synchronous and thread-local, matching the existing `Rc`
API. Borrowed containers are retained conservatively and retried later.
Rust hosts may call `runtime::collect_garbage()` for a checkpoint and node
statistics, and `runtime::track_value(&value)` to adopt manually constructed
or directly mutated raw `Rc` graphs. Use these before disconnecting raw nodes;
unregistered, unreachable host graphs cannot be discovered afterward.
Holding a host Value after its Interpreter is destroyed is supported; after
dropping the last host root, collect explicitly or use the next interpreter
checkpoint. Normal Solx allocations are registered automatically.

This is cycle collection, not a moving heap, a sandbox, a memory budget, or a
guarantee of bounded pauses. Execution and syntax limits are specified in
section 12.4. Ordinary deep acyclic value destruction remains recursive. See
[MEMORY_MANAGEMENT.md](MEMORY_MANAGEMENT.md) for the ownership contract,
algorithm, measurements, and remaining work.

## 10. Functions

The tree-walk interpreter currently implements named and anonymous
functions, exact argument counts, lexical closures, recursion, and a
single return value. Run `cargo run -- examples/functions.solx` for an
example that prints `30`, `10`, `1`, and `2` on separate lines.

### 10.1 Named functions

``` lua
fn add(a, b)
    return a + b
end
```

### 10.2 Anonymous functions

``` lua
var double = fn(x)
    return x * 2
end
```

`fn` is the only function syntax in the initial design. Arrow functions
are not included.

Named declarations bind in the current scope, using the same redeclaration
and constant checks as `var`. Functions are values: they can be passed as
arguments, returned, and stored in lists or objects. The existing `print`,
`str`, and `num` built-ins are callable values and follow normal name
shadowing. `Function` is accepted as a variable type annotation.

Parameters are distinct, unannotated names. Zero parameters and zero
arguments are supported. Calls evaluate the callee first, then each
argument once from left to right in the caller's scope. User functions
require exactly their declared number of arguments; `print` remains
variadic. Function parameter and return type annotations are deferred.

### 10.3 Closures

Closures are supported. A function retains access to variables from its
enclosing lexical scope even after that scope has finished.

Each call gets a fresh scope for parameters and locals, enclosing the
function's captured scopes. Caller locals are not visible to the callee.
Sibling closures share captured bindings; separate calls create separate
locals. Existing `if` branches still introduce scopes. Captured bindings
retain their constant, type annotation, and nil-assignment checks.
Function equality compares identity, including when functions appear in
collections.

The current implementation shares scopes through `Rc<RefCell<...>>` and
collects unreachable function/environment, collection, and exported-closure
cycles at the checkpoints in section 9.5. Live closures continue to retain
their captured bindings. The earlier leak probes in `RUNTIME_AUDIT.md` now
have collection regression coverage.
Recursion uses the Rust call stack; tail-call optimisation is not implemented.
Call and combined evaluation depth are now limited (section 12.4). Parser
nesting and AST depth checks cover source-produced syntax and function-body
cloning/destruction. These guards are not a guarantee for arbitrarily small
host stacks or manually constructed host graphs.

### 10.4 Multiple return values

This section describes planned behaviour. The current interpreter rejects
multiple-value returns with a diagnostic; multiple-variable binding is
also not implemented yet.

Functions can return multiple values, and callers can bind them to
multiple variables.

``` lua
fn getStats()
    return 100, 25
end

var score, health = getStats()
```

### 10.5 Implicit return

A function that finishes without an explicit `return` produces `nil`.

A bare `return` also produces `nil`. A newline immediately after `return`
ends the statement; start a return expression on the same line as the
keyword. Returns exit through nested `if` branches, and top-level returns
are rejected with a source-positioned parse error.

The existing binding behaviour is preserved: declarations (including
parameter binding) can initially hold `nil`, while assigning a nil result
to an existing untyped variable deletes that binding. Constants and type
annotations are checked before assignment or deletion.

The exact interaction of multiple returns with expressions, argument
evaluation, and the special deletion semantics of `nil` remains to be
defined.

## 11. Loops

Lua-style numeric `for` loops and `while` loops are supported. `break`
and `continue` are supported.

``` lua
for i = 1, 5 do
    print(i)
end

var count = 0
while count < 3 do
    count = count + 1
end
```

Numeric loops accept an optional step: `for i = start, limit, step do`
... `end`. The default step is `1`. Start, limit, and step expressions
are evaluated once, from left to right in the enclosing scope, before
binding the loop variable. All three must be finite Numbers, and the
step must be nonzero, even for an empty range.

The bounds are inclusive: a positive step continues while the counter
is at most the limit; a negative step continues while it is at least
the limit. A range facing away from its limit runs zero times. Equal
bounds run once with either step direction. For example, `for i = 5, 1,
-2 do print(i) end` prints `5`, `3`, and `1`.

Each iteration has a fresh local scope and loop-variable binding.
Closures retain the binding of the iteration that created them; they
still share variables captured from outside the loop. The loop variable
shadows an enclosing name and is not visible after the loop. Assigning,
redeclaring, or deleting that visible binding uses the existing variable
rules and does not change the internal counter or saved bounds.

The current Number representation uses floating-point arithmetic, so
fractional steps may round. If an increment rounds back to the same
counter before reaching the limit, execution reports `for loop step
does not advance the counter`. An increment that overflows past finite
bounds ends the loop; nonfinite header values are rejected.

`while` evaluates its condition before each iteration, outside the fresh
body scope, using normal Solx truthiness. Both loop kinds consume the shared
execution budget, even when their bodies are empty (section 12.4).

`break` exits the nearest enclosing loop. `continue` skips the rest of
that iteration, advancing a numeric counter or reevaluating a `while`
condition. Both work through nested `if` scopes and are rejected outside
a loop in the same function. A function declared inside a loop cannot
break or continue its caller's loop. `return` exits the current function
through any nested loops. Normal completion, all three control-flow
statements, and runtime errors restore enclosing scopes; completed
mutations are retained.

## 12. Error handling

### 12.1 `try` / `catch`

`try`/`catch` is implemented. It handles recoverable Solx runtime errors
without wrapping the protected body in another function.

``` lua
try
    riskyOperation()
catch err
    print(err.message)
end

print("Program continues")
```

-   The `try` body runs normally.
-   If a runtime error occurs, execution transfers to `catch`.
-   `err` receives an Object containing the error information below.
-   Execution continues after the closing `end`.
-   If no error occurs, `catch` is skipped.
-   A source buffer is fully lexed and parsed before any of it executes.
    Its syntax errors cannot be caught by a handler in that buffer.
    Imports load separate buffers during execution, so a surrounding
    handler in the importer can catch a module's read, syntax, or
    initialization failure. Failed modules remain eligible for retry.

Syntax is `try ... catch name ... end`: exactly one handler and a required
identifier. Empty bodies are allowed. Try and catch each have a separate
lexical scope. The handler can see outer bindings, but not try-local
variables. Its error binding is an ordinary mutable, unannotated local;
it can shadow an outer binding without changing that binding. Escaped
closures retain their captured try/catch locals under the usual rules.

Completed assignments, collection mutations, and module side effects are
preserved; catching an error is not a transaction. `return`, `break`, and
`continue` retain their usual targets and do not enter a handler. An error
in the handler propagates to the next enclosing try, or to the caller;
it never reenters the same handler. Scopes and call/import contexts are
restored on all these paths. Warnings alone do not enter a handler and
already-emitted warnings are not suppressed.

The error Object is a detached, mutable snapshot with these fields:

| Field | Value |
| --- | --- |
| `message` | The original String message without location/trace decoration. |
| `location` | An Object with `file`, `line`, and `column`, or nil when unavailable. |
| `trace` | A List of `{description, location}` Objects, innermost call/import first, ending at the handler boundary. May be empty. |

A location's `file` is the canonical path for file execution or `<source>`
for interactive/source buffers. `line` and `column` are 1-based Numbers,
or nil when that location has no span (for example, a file read failure).
Each field exists even when its value is nil. Runtime positions identify
the failing statement; lexical and syntax errors identify their token.
Trace locations identify call/import sites. No live environments are
exposed, and modifying one error snapshot does not change another error.

`core.error(message)` still accepts only a String. Passing `err.message`
raises a new error at that statement; structured rethrow and cause chains
are not implemented. There is no `finally`/`defer` syntax yet. Catching
Rust panics, stack exhaustion, allocation failure, or process termination
is outside this mechanism. Execution-limit failures and cooperative cancellation
also bypass handlers (section 12.4). Cycle collection is separate (section 9.5).

Run `cargo run -- examples/error_handling.solx` for a working example.

### 12.2 `pcall()`

`pcall()` is planned for catching runtime errors through a function call.
Its exact return values and behaviour remain unresolved.

### 12.3 Uncaught runtime errors

Uncaught runtime errors stop normal execution and produce a full
diagnostic report, intended to include: - Clear error message. - File,
line, and column. - Relevant source excerpt and highlighted location. -
Stack trace. - Helpful suggestions when a likely fix can be identified.

Formatting shown in examples is illustrative, not yet a stable CLI
contract.

Implemented diagnostics retain the original error message, a source
location, and innermost-first function/import context. Syntax and lexical
errors use the offending token position. Runtime errors use the start of
the innermost failing statement; call contexts use the calling statement's
position. These are 1-based line/column positions, not expression-level
highlights. File execution uses canonical file paths, and closures retain
their definition's file and statement positions after the source buffer
is gone. Imports include the requested specifier and defining import base.

Runtime diagnostics for `-e`, REPL buffers, and `run_source` use `<source>`
with positions relative to that input buffer, including after an entry
file ran. The REPL's direct parser diagnostics retain `at line:column`.
Their import
base still follows section 13; diagnostics do not mislabel interactive
lines as lines of that file. AST statements constructed by Rust without
source metadata have no precise position. `RuntimeError.message` contains
only the original message; `location` and `contexts` hold structured
metadata, and Display renders all three. Source excerpts, individual
expression spans, unique REPL-buffer names, and suggestions are deferred.

### 12.4 Execution limits and cancellation (implemented)

Each public interpreter run starts a fresh evaluation budget of 10,000,000
steps by default. Each executed statement, evaluated expression, and entered
loop iteration costs one step; metadata wrappers and short-circuited operands
do not. Functions, imports, and catch handlers share the caller's budget.
A zero budget allows only empty programs. CLI `--max-steps N` and the Rust
`ExecutionLimits` API configure it; `--unlimited-steps` or host `None` disables
only the step budget.

Defaults and current ceilings are 64 combined parser entries, 128 AST nodes
along any path (including location wrappers), 64 combined active statement/
expression evaluations, 32 user-function calls, and 16 uncached imports.
Entry files and cache hits do not add import depth. All depth settings may be
lowered but not raised above these ceilings or disabled. Combined evaluation
depth can be exhausted before an individual call/import depth limit.

Parser nesting is checked before descent, and AST depth while constructing
expressions/statements, including iterative postfix and left-associated chains.
Host-supplied Programs are validated before execution or function cloning.
The host remains responsible for safely dropping rejected manual ASTs.

Limit failures and cancellation stop the current public run, bypassing all
Solx handlers, with source/call/import context where available. Ordinary
language errors retain their catch behavior. Host errors distinguish these
cases through `RuntimeError.kind` and `ParseError.limit`. On interruption,
scopes and active counters/loading contexts are restored; completed effects
and successful module caches remain. Incomplete modules can be retried.

The CLI maps Ctrl+C to a cooperative cancellation token. The REPL clears the
request after the interrupted buffer and accepts another command; embedders
must explicitly reset their token after the run/controller have ended. The
token is thread-safe, while the interpreter remains single-threaded.

Checks occur at evaluation/loop boundaries, before imports, and around parsing.
They do not preempt native calls, parsing, I/O, output, or GC already underway.
These are implementation limits, not a sandbox, byte/memory/time limit, or a
proof for every host stack. Deep runtime value destruction remains recursive.
See [EXECUTION_LIMITS.md](EXECUTION_LIMITS.md) for APIs, CLI options, counting,
tested behavior, and remaining work toward an explicit execution stack.

### 12.5 Non-executing syntax checking (implemented)

`solx check FILE` and `solx check --json --stdin` use the real lexer and parser
without constructing an interpreter. They do not execute code or resolve/load
imports. They report the first lexer/parser error or existing syntax-depth
limit, with preserved half-open UTF-8 byte ranges. Syntax checking does not
check names, types, module availability, or runtime behavior.

The CLI limits input to 1 MiB of UTF-8 source. JSON schema version 1 distinguishes
valid input (exit 0), source diagnostics (exit 1), and input failures (exit 2).
The schema, offset units, and editor integration are specified in
[CHECKING.md](CHECKING.md). Lexer errors now retain byte ranges when converted
to ParseError; this adds metadata without changing accepted syntax.

## 13. Imports and modules

### 13.1 Explicit imports

Solx has an `import` keyword. Built-in libraries are shipped with Solx
but must be explicitly imported before use. Only imported libraries
should be initialised, helping avoid unnecessary runtime overhead.

``` lua
import "math"

var result = math.sqrt(25)
print(result)
```

### 13.2 File and module imports

Both file-path imports and module-name imports are supported in the
design:

``` lua
import "player.solx"
import "math"
```

-   File-path imports resolve `.solx` files.
-   Module-name imports resolve built-in libraries or modules in defined
    search locations.
-   Imported modules should be cached so they are not needlessly
    initialised repeatedly.
-   Each file has its own global namespace.
-   Values are shared across files through an explicit module interface,
    not by merging globals.

### 13.3 Module decisions

The minimal syntax is `import "specifier"`, optionally followed by
`as name`. `as` is contextual here, not a new reserved word. Imports are
statements allowed in any scope, and the specifier must be a string
literal. The default binding is the built-in module name or the file's
stem; use an alias when that stem is not a valid Solx identifier. Imports
use the same local binding/redeclaration checks as `var`.

```lua
-- counter.solx
global count = 0
fn increment()
    count = count + 1
    return count
end
exports.increment = increment

-- main.solx
import "./counter.solx" as counter
print(counter.increment())
```

An imported file executes in a fresh environment with its own globals,
file locals, and the existing `print`, `num`, and `str` built-ins. Its
predefined local `const exports: Object = {}` holds public values; all
other names remain private. No new export keyword or top-level return
semantics are introduced. Closures retain their defining environment,
including the defining file's path for imports made during later calls.

Importing binds a `Module` value. Dot access and string-key indexing read
its exports. A missing export is an error. Namespace properties cannot
be assigned by importers; `Object` mutation helpers do not accept Module
values. The defining file can update `exports` after initialization and
importers see those updates. Exporting an ordinary scalar copies its
current value, not its variable binding; exporting a list, object, or
function shares that value. Mutable exported collections remain mutable.
Modules compare by identity, are truthy, format as `<module name>`, and
support the `Module` variable annotation. They are not Objects.

Bare names resolve only registered Rust standard modules. Other
specifiers must name an explicit `.solx` file; no extension guessing,
directory entry search, environment search paths, or package downloads
occur. Relative paths resolve against the defining file's directory.
CLI entry files resolve from the process working directory; `-e`, plain
REPL sessions, and `Interpreter::run_source` without a file context use
the interpreter's initial working directory. A REPL entered after a file
uses that file's directory. Absolute paths are accepted. `/` is the
portable path separator; host-native paths also work. This loader is
not a filesystem sandbox.

File identities use canonical absolute paths (including resolved
symlinks). Within one Interpreter, successful imports share one namespace
and execute once, even through aliases or normalized paths. Entry files
run explicitly through `run_file` are executed each time; they participate
in cycle detection but are not inserted into the import cache. Loading
is synchronous and follows statement execution order. Import cycles
report the loading chain, including cycles through an entry file.

The cache is a snapshot for the interpreter's lifetime, not hot reload:
changing file contents does not reinitialize an already cached namespace.
Each import still canonicalizes the requested path, so a deleted or
unresolvable path errors even if that file was cached. Existing namespace
values remain usable. Restoring a file at the same canonical path reuses
the cached namespace; use a new Interpreter to load new contents.

On read, parse, or initialization failure, the caller's environment and
file context are restored and no incomplete module is cached. A later
import can retry. Successful dependencies remain cached, and completed
side effects (such as printed output or shared collection mutation) are
not rolled back. A closure published into a successful dependency before
failure remains usable and retains its defining environment; failed
module scopes therefore cannot simply be cleared. Diagnostics follow
section 12.3, retaining the original error location through nested imports.

An explicit `run_file` selects the persistent entry import base after
path resolution, reading, and cycle checks succeed, even if parsing or
execution subsequently fails. Completed entry-file mutations remain;
entry execution is not a transaction. This policy is separate from
restoring the caller after an imported file fails.

The loader owns resolution, module identities, the cache, and the loading
stack separately from expression evaluation. Both Rust providers and
source files produce the same namespace value. Future package resolution
can add a source provider without changing call evaluation or exports.
Unresolved: package lookup integration, hot reload, asynchronous loading,
and a capability model for untrusted code.

## 14. Standard library

### 14.1 Existing built-ins and supported scope

`print(...)` prints tab-separated values plus a newline
and returns nil; zero arguments print a blank line. `str(value)` accepts
one value and uses the existing Solx formatting. `num(value)` accepts one
Number or a trimmed numeric String, including the existing nonfinite
number conversions; other types and invalid strings error. All three
remain global, first-class, and shadowable, with unchanged diagnostics.
There are no duplicate namespaced conversion aliases.

The interpreter implements the five native modules below and the file
imports in section 13. JSON, randomness, filesystem/process utilities,
try/catch, and pcall remain deferred. No dependencies beyond the existing
CLI dependency are added. Run `cargo run -- examples/modules/main.solx`
for a two-file example that combines native APIs and a pure Solx utility.

### 14.2 Standard module API

These are registered Rust modules, initialized only when imported, not
pretend `.solx` files. Functions are ordinary first-class Function values.
All validate arity before types and report the qualified function name
and argument position for invalid types. Arguments still evaluate once,
left to right, before validation, following ordinary calls.

| Module | API | Behaviour |
| --- | --- | --- |
| `core` | `type(value)` | Returns the annotation-style type name, including Nil, Function, and Module. |
| `core` | `assert(condition[, message])` | Uses Solx truthiness; returns the condition on success, otherwise raises an error. Optional message must be String, even on success. |
| `core` | `error(message)` | Raises a runtime error with the supplied String. |
| `math` | `abs(x)`, `sqrt(x)`, `floor(x)`, `ceil(x)`, `round(x)` | Finite Number inputs; sqrt rejects negative numbers. Round uses nearest integer, halves away from zero. |
| `math` | `min(...)`, `max(...)` | One or more finite Numbers. |
| `math` | `pi` | Number constant. |
| `string` | `len(text)` | Unicode scalar count, matching `#text`; not UTF-8 byte or grapheme count. |
| `string` | `upper(text)`, `lower(text)`, `trim(text)` | Unicode case conversion and Unicode whitespace trimming; case conversion may change length. |
| `string` | `slice(text, start, count)` | 1-based Unicode scalar start and nonnegative count. Both finite integers; range must fit. Start `len + 1` is valid only with zero count. |
| `string` | `find(text, needle)` | First 1-based Unicode scalar position, or nil if absent. Empty needle returns 1. Literal, case-sensitive search. |
| `string` | `split(text, separator)` | Literal nonempty separator; preserves empty fields, including trailing ones. Splitting empty text returns `[""]`. |
| `string` | `join(items, separator)` | List of Strings and String separator; no coercion. Empty list returns empty String. |
| `string` | `starts_with(text, prefix)`, `ends_with(text, suffix)` | Case-sensitive; empty prefix/suffix matches. |
| `list` | `len(items)` | Number of entries, matching `#items`. |
| `list` | `append(items, value)` | Mutates the shared list, including nil entries; returns nil. |
| `list` | `remove(items, index)` | Removes and returns the entry, shifts later entries left. Index is a finite integer in `1..len`; invalid or empty-list removal errors. |
| `list` | `find(items, value)` | First 1-based matching index using existing value equality; nil if absent. |
| `list` | `sort(items)` | Stable, ascending, in-place sort of uniformly finite Numbers or uniformly Strings. Empty lists accepted. Mixed types, nonfinite numbers, and other types error before mutation. Returns nil; custom comparators deferred. |
| `object` | `keys(object)` | A new List of keys sorted lexicographically for deterministic output. |
| `object` | `has(object, key)` | Boolean; key must be String. A property holding nil still exists. |
| `object` | `remove(object, key)` | Removes and returns the property, or nil when absent, without warning. |

Lengths and indexes are Numbers. Collection helpers mutate shared
collections even when held by a constant, matching existing property and
index assignment: const protects a binding, not its contents. Assigning
nil to variables still deletes bindings; nil in collections remains an
entry until explicitly removed. Existing arithmetic, conversions, and
acyclic equality are unchanged; cyclic formatting and structural equality
follow section 9.4.

Random generation is deferred until seeding, reproducibility, generator
state isolation, and cryptographic/noncryptographic API boundaries are
specified. No implicit clock-seeded global generator is introduced.

## 15. Programming paradigms

The initial language supports procedural and functional programming: -
Variables, loops, conditions, and functions. - Closures and anonymous
functions. - Higher-order functions. - Plain mutable objects.

Classes and inheritance are not part of the initial design. Adding
classes later is not ruled out, but should require a clear benefit.

## 16. Package manager

A package manager is planned for a later stage. First implement and
stabilise the language, runtime, and module system. Then design
dependency manifests, package installation, version resolution, and
third-party library distribution.

The following is a proposed future design, not implemented tooling:

- A project `solx.toml` declares package name, semantic version, entry
  file, and dependencies. Sources are explicit local paths or registry
  packages with version constraints. Names use lowercase ASCII segments,
  optionally owner-qualified (`owner/name`), with no path traversal.
- Resolve the complete graph before execution. Initially choose one
  compatible version per package and report dependency chains on
  conflicts. Prerelease rules and multiple-version coexistence remain open.
- Store immutable packages in a local content-addressed cache, mapped
  through the project lockfile. Importing never downloads code or searches
  arbitrary working-directory packages. Local dependencies have explicit
  canonical roots.
- `solx.lock` records exact versions, sources, dependency edges, and
  SHA-256 digests. Frozen installs reproduce that graph without resolving
  versions. Offline execution uses verified installed content. Changed
  local-path contents must be recorded or rejected in frozen mode.
- Verify archives against locked digests before extraction. Reject
  absolute paths, traversal, escaping symlinks, duplicate paths, and
  excessive sizes. Extract temporarily and publish atomically. Digests
  verify consistency; publisher authenticity needs a separate trust and
  signature policy.
- Installation only copies and verifies data: no lifecycle hooks, native
  builds, or arbitrary installation scripts. Code runs when the user
  runs/imports it. Verification is not a runtime sandbox; capability
  restrictions need a separate design.

Manifest parsing, resolution, lockfile tooling, registry protocol,
authentication, signing, and install commands are deferred. This
milestone does not download or install external packages.

## 17. Performance and benchmarking

Performance is a core goal, with emphasis on both speed and memory use.

Benchmarking principles: - Compare equivalent programs and workloads. -
Include standard Lua and, where practical, LuaJIT. - Measure execution
time, startup time, peak or representative memory use, and
garbage-collection behaviour. - Include different workloads: arithmetic
loops, function calls, string operations, collection operations, and
realistic programs. - Record hardware, runtime versions, command lines,
and test conditions. - Report measured results rather than claiming a
fixed speedup in advance. - Use profiling to identify bottlenecks before
adding complexity.

Compact value representation is a design requirement from the beginning,
but its exact bit layout and storage strategy are implementation
decisions subject to measurement.

## 18. Confirmed decisions checklist

These are design decisions, not a completed-feature checklist. In particular,
multiple returns, `pcall`, and the bytecode VM remain
unimplemented. See [the language review](LANGUAGE_REVIEW.md) for current gaps.

-   [x] Language name: Solx.
-   [x] File extension: `.solx`.
-   [x] Implementation language: Rust.
-   [x] Lua-inspired syntax with `then`, `do`, and `end`; no mandatory
    semicolons.
-   [x] `var`, `global`, and `const`.
-   [x] Same-scope `var` redeclaration replaces the existing value.
-   [x] File-scoped globals; imported files have separate global
    namespaces.
-   [x] Dynamic typing by default; optional type annotations.
-   [x] One language-level `Number` type.
-   [x] Lua-style truthiness.
-   [x] Lua operator set and precedence.
-   [x] `num()`, `str()`, and `print()`.
-   [x] Separate lists and objects; 1-based list indexing.
-   [x] Missing list indexes warn and return `nil`.
-   [x] Missing object properties warn and return `nil`.
-   [x] Dot and bracket object access.
-   [x] Assigning `nil` removes a variable binding.
-   [x] Named and anonymous `fn` functions; no arrow functions
    initially.
-   [x] Closures and multiple return values.
-   [x] Implicit function return produces `nil`.
-   [x] Numeric `for`, `while`, `break`, and `continue`.
-   [x] `try`/`catch` and `pcall()`.
-   [x] Full diagnostics for uncaught runtime errors.
-   [x] Explicit imports for bundled standard libraries.
-   [x] Both file-path and module-name imports.
-   [x] Procedural and functional programming; no classes initially.
-   [x] Automatic garbage collection.
-   [x] Compact value representation from the start.
-   [x] Package manager deferred until the language and module system
    stabilise.
-   [x] Performance measured through benchmarks; no unsupported speedup
    claims.

## 19. Unresolved decisions

These are intentionally not treated as settled: 1. Exact representation
and behaviour of `nil` in expressions, returns, collections, and typed
variables, given that assigning `nil` to a variable removes its binding.
2. Complete type system: supported type names, annotation grammar,
function signatures, compatibility rules, and error timing. 3. String
interpolation escaping and multiline-string syntax. 4. Equality rules
across different types and for collections. 5. Numeric edge cases,
conversions, division-by-zero behaviour, and length operator semantics.
6. Broader collection operations beyond section 14. 7. Deep immutability
(current constants protect bindings only). 8. Package resolution, hot
reload, and untrusted-module capabilities beyond section 13.
9. Exact `pcall()` result values. 10. Detailed runtime
diagnostic format. 11. Collector tuning and future heap representation. 12.
Standard-library inventory and APIs. 13. CLI command design, project
layout, and build/release process. 14. Benchmark suite, baseline
versions, and acceptance thresholds.

## 20. Suggested implementation order

1.  Create the Rust workspace and automated test setup.
2.  Implement a lexer with tests for keywords, identifiers, numbers,
    strings, operators, comments, and source positions.
3.  Implement a parser and AST for literals, variables, arithmetic,
    concatenation, conditions, and `print`.
4.  Implement runtime values and scope resolution.
5.  Add functions, closures, return values, and loops.
6.  Add lists and objects, including the chosen warning behaviour.
7.  Add diagnostics with source locations and stack traces.
8.  Resolve `nil` and type-system edge cases before depending on them
    throughout the runtime.
9.  Add modules and explicit standard-library imports.
10. Add `try`/`catch` and specify `pcall()` behaviour.
11. Add the bytecode VM and benchmark against the interpreter.
12. Profile, optimise, and publish reproducible performance and memory
    results.

------------------------------------------------------------------------

**Change policy:** When a decision changes, update this document and the
relevant tests together. Mark a rule as confirmed only after the
intended behaviour is explicit enough to test.
