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
-   Use automatic garbage collection. The specific collector algorithm
    remains undecided and must be chosen with measurements of memory
    overhead and pause times.

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
have globals with the same name without colliding. A module interface
will be used to share values between files; exact export syntax remains
unresolved.

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

Both concatenation and string interpolation are supported.

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

The current implementation shares scopes through `Rc<RefCell<...>>`.
Reference cycles between functions and their environments are not yet
collected and can retain memory. Recursion uses the Rust call stack;
tail-call optimisation and a garbage collector are not implemented.

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
body scope, using normal Solx truthiness. Neither loop kind adds a
general execution limit; a `while` condition must eventually become
false or the body must exit explicitly.

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

Solx supports `try`/`catch`.

``` lua
try
    riskyOperation()
catch err
    print(err)
end

print("Program continues")
```

-   The `try` body runs normally.
-   If a runtime error occurs, execution transfers to `catch`.
-   `err` receives error information.
-   Execution continues after the closing `end`.
-   If no error occurs, `catch` is skipped.
-   Syntax errors are detected before execution and cannot be caught by
    `try`/`catch`.

### 12.2 `pcall()`

`pcall()` is also supported for catching runtime errors through a
function call. Its exact return values and behaviour are unresolved and
must be documented before implementation.

### 12.3 Uncaught runtime errors

Uncaught runtime errors stop normal execution and produce a full
diagnostic report, intended to include: - Clear error message. - File,
line, and column. - Relevant source excerpt and highlighted location. -
Stack trace. - Helpful suggestions when a likely fix can be identified.

Formatting shown in examples is illustrative, not yet a stable CLI
contract.

## 13. Imports and modules

### 13.1 Explicit imports

Solx has an `import` keyword. Built-in libraries are shipped with Solx
but must be explicitly imported before use. Only imported libraries
should be initialised, helping avoid unnecessary runtime overhead.

``` lua
import "math"
import "json"

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

Unresolved module details: - Exact search paths and path
normalisation. - Export syntax and visibility rules. - Whether imports
are allowed only at file scope. - Import initialisation order. -
Circular imports. - Failure behaviour for missing modules. - Whether
repeated imports return a module object and the exact syntax for
accessing exports.

## 14. Standard library

Standard libraries are bundled with Solx and imported explicitly. `math`
and `json` are example library names; the complete standard-library
inventory is not final.

Potential libraries and capabilities need to be prioritised based on
actual use cases. Libraries should not be initialised unless imported.

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
6. List index validation and collection mutation rules. 7. Constant
behaviour for mutable objects/lists. 8. Module exports, module objects,
resolution paths, caching, circular imports, and initialisation
failures. 9. Exact `pcall()` result values. 10. Detailed runtime
diagnostic format. 11. Garbage-collection algorithm and tuning. 12.
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
