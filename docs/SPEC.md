# Solx Language Documentation

**Status:** Design draft  
**File extension:** `.solx`  
**Implementation language:** Rust

Solx is a Lua-inspired programming language aiming to keep code simple to write while making common mistakes easier to spot. This document records the decisions made so far. Anything marked **TBD** has not been decided yet and should not be treated as final language behaviour.

## 1. Design goals

- Keep syntax lightweight and familiar to Lua programmers.
- Use clear rules for variables and scopes.
- Be dynamically typed.
- Make common mistakes easier to identify without making the language needlessly difficult for beginners.
- Build a working interpreter first, then consider a bytecode compiler/virtual machine.
- Do not promise performance advantages over Lua until they are measured.

## 2. Files and syntax

Solx source files use the `.solx` extension.

Solx keeps Lua-style block endings and does not require semicolons:

```lua
if score >= 100 then
    print("High score!")
elseif score > 0 then
    print("Keep going!")
else
    print("Start playing!")
end
```

The language uses `then` after conditions and `end` to close blocks. Loops use `do` and `end`.

## 3. Variables and constants

### `var`

`var` declares a variable. Assigning to its name later updates the existing variable.

```lua
var score = 50
score = 100
score = score + 25

print(score) -- 125
```

Assigning to an undeclared name does not silently create a variable.

Redeclaring `var` in the same scope replaces the existing variable's value:

```lua
var score = 10
var score = 20
print(score) -- 20
```

### `global`

`global` declares a file-wide variable that can be accessed from functions in that file.

```lua
global score = 10

fn increase()
    score = score + 1
    return nil
end

increase()
print(score) -- 11
```

A local variable can shadow a global variable. Updating the local variable does not change the global one:

```lua
global score = 100

fn play()
    var score = 10
    score = 20
    print(score) -- 20
    return nil
end

play()
print(score) -- 100
```

The exact interaction between globals and imported files is **TBD**.

### `const`

`const` declares a value that cannot be reassigned.

```lua
const pi = 3.14159
var score = 10
score = 20 -- Valid
pi = 4     -- Error
```

The exact scope and redeclaration rules for constants are **TBD**.

### Scope

Solx uses lexical scoping. A nested scope can declare a local variable with the same name as an outer variable; the inner variable takes priority inside that scope.

## 4. Types and values

Solx is dynamically typed. A variable can hold values of different types over time:

```lua
var value = 100
value = "hello"
value = true
```

The planned basic values include:

- `Number` for both integers and decimals.
- `String` for text.
- `Boolean`: `true` or `false`.
- `nil` for an absent or empty value.
- Lists for ordered values.
- Objects for named properties.
- Functions.

### Truthiness

Solx follows Lua-style truthiness: only `false` and `nil` are false. Values such as `0`, an empty string (`""`), and an empty list are true.

### `nil`

A declared variable may hold `nil`. Reading that variable is valid and returns `nil`.

Whether assigning `nil` removes a variable or simply clears its value is **TBD**. The current preferred direction is to keep the declaration, but this has not been confirmed.

### Undeclared names

The intended behaviour is to warn about an undeclared name and report an error if the program attempts to read it. A declared variable holding `nil` must not trigger this undeclared-name warning. The exact timing and wording of warnings/errors is **TBD**.

## 5. Operators and conversions

### Arithmetic and text concatenation

`+` is for numerical addition. It does not silently convert strings into numbers or turn addition into concatenation.

`..` joins values as text and automatically converts supported values to strings:

```lua
var age = 14

print("Age: " .. age)       -- Age: 14
print("Hello" .. " Jinx")   -- Hello Jinx
print(10 + 5)                -- 15
print("10" + 5)              -- Type error
```

### Conversion functions

- `num(value)` converts a suitable value to a number.
- `str(value)` converts a value to a string.

Exact conversion rules, including what happens when conversion fails, are **TBD**.

### Other operators

Comparison operators and logical operators will be needed, but their full behaviour and precedence rules are **TBD**.

## 6. Lists and objects

Solx has separate list and object structures rather than using one table type for both.

### Lists

Lists are ordered collections and use **1-based indexing**, like Lua:

```lua
var inventory = ["Sword", "Shield", "Potion"]

print(inventory[1]) -- Sword
print(inventory[2]) -- Shield
print(inventory[3]) -- Potion
```

The syntax and rules for list mutation, out-of-range indexes, and list length are **TBD**.

### Objects

Objects store named properties:

```lua
var player = {
    name = "Jinx",
    score = 100
}

print(player.name) -- Jinx
```

Whether both dot notation (`player.name`) and bracket notation (`player["name"]`) are supported is **TBD**. The design discussion has not yet settled this.

## 7. Functions

Named functions use `fn`, and blocks end with `end`:

```lua
fn add(a, b)
    return a + b
end

print(add(5, 3)) -- 8
```

### Return values

- Functions can return multiple values.
- A function may finish without an explicit `return`; its result is `nil`.
- `return` exits the current function.

```lua
fn getStats()
    return 100, 25
end

var score, health = getStats()
```

Exact rules for assigning multiple return values are **TBD**.

### Anonymous functions and closures

Solx supports anonymous functions:

```lua
var double = fn(x)
    return x * 2
end

print(double(5)) -- 10
```

Functions support closures: an inner function remembers variables from its enclosing lexical scope, even after that scope has finished.

```lua
fn makeCounter()
    var count = 0

    return fn()
        count = count + 1
        return count
    end
end

var counter = makeCounter()
print(counter()) -- 1
print(counter()) -- 2
```

## 8. Conditions and loops

### Conditions

Solx supports `if`, `elseif`, and `else`, with `then` and `end`.

### Numeric `for` loops

```lua
for i = 1, 5 do
    print(i)
end
```

The loop prints the values from 1 through 5. The exact rules for step values and loop-variable scope are **TBD**.

### `while` loops

```lua
var count = 1

while count <= 5 do
    print(count)
    count = count + 1
end
```

### Loop control

- `break` exits the current loop.
- `continue` skips to the next iteration.

Iteration syntax for walking through lists and objects is **TBD**.

## 9. Errors

Solx is intended to support both `try`/`catch` and `pcall()` for handling runtime errors.

Illustrative `try`/`catch` syntax:

```lua
try
    riskyOperation()
catch error
    print(error)
end
```

Illustrative `pcall()` syntax:

```lua
var success, result = pcall(riskyOperation)
```

The exact syntax, error value format, and which errors each mechanism can catch are **TBD**. Syntax errors should be reported before program execution; runtime error handling is a separate concern.

## 10. Imports and modules

Solx uses the `import` keyword to bring in modules. `require()` is not the chosen import syntax.

Illustrative syntax:

```lua
import "math"
import "player"
```

The exact import grammar, module search paths, exports, circular-import behaviour, and how built-in libraries are declared are **TBD**.

## 11. Objects and classes

The initial design does **not** include classes. Programs can use objects for data and functions for behaviour. Class-based object-oriented programming can be reconsidered later if there is a clear need.

## 12. Implementation plan

1. Finalise the core language specification.
2. Implement a Rust lexer and parser.
3. Build a tree-walk interpreter and runtime values/scopes.
4. Add tests for agreed language behaviour.
5. Add standard-library functionality and module imports.
6. Consider a bytecode compiler and virtual machine after the interpreter works reliably.

The bytecode VM is a later implementation stage, not a feature that needs to be completed before Solx can run programs.

## 13. Decisions still open

The following should be decided before the relevant features are implemented:

- Whether assigning `nil` clears a variable's value or removes its declaration.
- Exact warning/error behaviour for undeclared names.
- Constant scope and redeclaration rules.
- Operator set, logical operators, and precedence.
- Number conversion and string conversion failure behaviour.
- List mutation, length, and out-of-range indexing.
- Whether object access supports both dot and bracket notation.
- Multiple-return assignment details.
- Function argument rules and recursion details.
- `for` loop step syntax and loop-variable scope.
- Iteration over lists and objects.
- Exact `try`/`catch` and `pcall()` behaviour.
- Module exports, file resolution, and built-in library declarations.
- Standard-library contents.

This is a living draft. Update it when a decision is explicitly agreed, and keep undecided behaviour marked as **TBD** rather than silently choosing a rule.
