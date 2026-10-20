# Solx Language Specification

**Status:** Draft  
**File extension:** `.solx`  
**Implementation language:** Rust  
**Language inspiration:** Lua

## 1. Overview

Solx is a modern, Lua-inspired programming language designed to preserve Lua's simplicity while improving its syntax, consistency, error handling, standard library and performance potential.

Solx will initially use a tree-walk interpreter. A bytecode compiler and virtual machine will follow.

## 2. Variables and Scope

### 2.1 Local Variables

The `var` keyword declares a variable in the current scope.

```lua
var name = "Jinx"
var age = 14
var active = true
```

Local variables are accessible only within their scope and any nested scopes that can see them.

### 2.2 Global Variables

The `global` keyword declares a global variable.

```lua
global score = 0
global version = 1
```

Global variables can be accessed and reassigned throughout the program.

Global declarations are made at the top-level program scope.

### 2.3 Assignment

Assignment updates an existing variable.

```lua
var score = 10
score = 20
```

Assigning to a name that has not been declared in an accessible scope or globally produces an error. Assignment must not silently create a variable.

### 2.4 Redeclaration

Declaring a variable with `var` again in the same scope replaces its existing value.

```lua
var score = 10
var score = 20

print(score)
```

Output:

```text
20
```

This rule applies to the existing variable in that scope. Redeclaration behaviour across nested scopes will follow the language's lexical-scoping rules.

### 2.5 Nested Scopes

Solx uses lexical scoping. A nested scope can access variables from enclosing scopes when those variables are visible.

```lua
fn test()
    var outer = 10

    if outer > 5 then
        var inner = 20
        print(inner)
    end

    print(outer)
end
```

The `inner` variable belongs to the nested block. It cannot be accessed outside that block.

Exact closure behaviour and function scope rules remain to be specified.

## 3. Data Types

The initial type system is expected to support:

- `nil`
- `boolean`
- `number`
- `string`
- `function`
- `table`

Additional types may be introduced during language design.

## 4. Type Conversion

Solx uses short conversion functions.

```lua
var age = num("14")
var message = str(14)
```

- `num()` converts a compatible value to a number.
- `str()` converts a value to a string.

Exact conversion behaviour and handling of invalid conversions remain to be specified.

## 5. Functions

Functions use the `fn` keyword and retain Lua-style `end` block closures.

```lua
fn add(a, b)
    return a + b
end

print(add(10, 20))
```

Function scope, closures, recursion and argument handling remain to be specified.

## 6. Booleans and Nil

Solx retains the keywords `true`, `false` and `nil`.

Their precise behaviour in conditions and comparisons will be specified before implementation.

## 7. Output

Solx provides a built-in `print()` function.

```lua
print("Hello, world!")
```

## 8. Design Principles

- Familiar to Lua programmers.
- Straightforward syntax for beginners.
- Explicit variable declarations and predictable scope.
- Errors instead of silently creating undeclared variables.
- Consistent language behaviour.
- Useful error messages.
- A capable standard library.
- Room for bytecode compilation and future optimisation.
- Performance measured through benchmarks rather than assumed.
- Compatibility decisions made deliberately.

## 9. Unfinalised Decisions

The following require further design:

- Function scope and closures.
- Nested-scope shadowing.
- Conditional and loop syntax.
- Tables, lists and data structures.
- Operators and precedence.
- Module and library declarations.
- Error handling.
- Type checking and optional types.
- Standard library design.
- Coroutines and asynchronous tasks.
- Bytecode format and execution model.
- Package management and developer tooling.
