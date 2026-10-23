# Collection iteration

Collection `for` loops visit list entries and object properties directly.

```lua
for value in ["Sword", nil, "Potion"] do
    print(value)
end

for index, value in ["Sword", "Potion"] do
    print(index, value) -- 1 Sword, then 2 Potion
end

for key in { z = 3, a = 1 } do
    print(key) -- a, then z
end

for key, value in { z = 3, a = nil } do
    print(key, value) -- a nil, then z 3
end
```

| Collection | One name | Two names | Order |
| --- | --- | --- | --- |
| List | Value | Index, value | Original list order; indexes start at 1 |
| Object | Key | Key, value | Keys sorted lexicographically by Unicode scalar value |

Nil entries are visited. Empty collections run no body iterations. Object
ordering is case-sensitive and does not apply locale rules or Unicode
normalization. Lists produce exact integer indexes with language type `Number`.

The collection expression is evaluated exactly once, in the enclosing scope,
before the loop names are bound. It must produce a List or Object; other types,
including String and Module, cause a recoverable runtime error. One or two
distinct, unannotated names are required. Numeric `for i = start, limit, step`
loops keep their existing syntax and behavior.

## Snapshot and mutation rules

The loop copies all top-level entries before entering its body. List order and
indexes, and object keys and values, come from this initial snapshot.

- Adding, removing, sorting, or replacing source entries does not change which
  snapshot entries the loop visits or their order.
- Reassigning the variable that held the source collection does not change the
  active loop.
- Scalar values in the snapshot retain their original values. Lists, objects,
  functions, and modules retain shared references. Mutating a nested collection
  is visible through the snapshot; replacing its source entry does not replace
  the reference already in the snapshot.
- Assigning to either loop variable updates that local binding. It does not
  write back to the source collection.

```lua
import "list"
var items = [10, 20]
for index, value in items do
    if index == 1 then
        list.remove(items, 2)
        list.append(items, 30)
    end
    print(index, value) -- 1 10, then 2 20
end
print(items) -- [10, 30]
```

The snapshot is shallow. For example, replacing `items[2]` cannot change a
saved second entry, but changing `items[2].score` changes that shared object's
contents if the snapshot still refers to it. Snapshot values remain alive
through collection checkpoints, even if the source variable is assigned nil.

## Scope and control flow

Each iteration creates fresh writable local bindings. They shadow enclosing
names and leave scope after the iteration. Closures retain the bindings from
the iteration that created them; sibling closures from one iteration share
those bindings. Outer captured variables remain shared across iterations.

```lua
import "list"
var readers = []
for value in [10, 20] do
    list.append(readers, fn() return value end)
end
print(readers[1]()) -- 10
print(readers[2]()) -- 20
```

`continue` advances to the next snapshot entry. `break` exits the nearest
enclosing loop. `return` exits the current function. These operations work
through nested conditionals and try/catch blocks; they cannot cross a function
boundary to control its caller's loop. Completed mutations remain in effect
when a runtime error exits the loop, and enclosing scopes are restored.

## Limits and host integration

Snapshot construction costs one execution step per entry, and each visit costs
another step even with an empty body. Normal expression and statement costs
also apply. The complete snapshot is built even when the first body iteration
will break. A budget failure during copying runs no body iterations. Object
snapshots are ordered incrementally, with interruption checks between entries.
Cancellation and configured deadlines use these same checkpoints. Collection
checkpoints run after body iterations, as for numeric loops.

The snapshot uses memory proportional to the number of entries, plus copied
string data; it does not deep-copy nested collections. Execution steps and
deadlines are cooperative checks, not a memory cap or a guarantee on the time
required for an individual allocation, string comparison, or value copy.

Rust hosts may hold a shared collection borrow during iteration. A mutable
borrow prevents snapshot creation and produces a recoverable collection-borrow
error. The interpreter releases its own source borrow before running the body,
so ordinary body mutations use the existing collection APIs.

## Compatibility

`in` is now a reserved keyword. Rename any earlier variable or parameter named
`in`. Use a quoted key and bracket access for an object property with that name:
`{ "in" = 1 }` and `record["in"]`. This change adds no general iterator protocol,
destructuring assignment, multiple returns, or variable-deletion syntax.

See [SPEC.md](SPEC.md) for the authoritative language rules. Tests are in
`tests/iteration.rs`, `tests/iteration_syntax.rs`, and `tests/iteration_cli.rs`.
