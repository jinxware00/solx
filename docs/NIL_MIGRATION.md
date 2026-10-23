# Migrating nil assignments

Assigning `nil` now stores the Nil value in the selected variable binding.
This is a breaking change from the earlier rule that removed the binding.
Declarations, reassignment, parameters, and return values now treat nil
consistently.

```lua
var result = 42
result = nil
print(result) -- nil; the variable remains declared.
result = 7    -- Reassignment still works.
```

The affected binding is selected by ordinary lexical scope lookup, including
captured locals and file globals. Assigning nil to an inner binding keeps the
outer binding shadowed. Sibling closures observe the stored nil and can later
assign another compatible value to the same binding.

```lua
var result = "outer"
if true then
    var result = "inner"
    result = nil
    print(result) -- nil; previously this printed "outer".
end
print(result) -- outer
```

## Updating existing code

If code used `value = nil` to make a subsequent access fail as undeclared,
check `value == nil` explicitly instead. If it used nil assignment to reveal
an outer variable, give that outer value a distinct name or end the inner
scope before accessing it. Locals leave their lexical scope normally;
closures can continue to retain captured bindings. There is no variable
deletion statement.

Assigning nil still releases the binding's reference to its previous value.
Reachable aliases and closures keep their values alive under the existing
memory-management rules; replacing a value does not promise immediate
collection of an unreachable cycle.

Optional results can now replace an existing unannotated variable directly.
An explicit `return nil`, a bare `return`, and reaching the end of a function
all produce a storable Nil value.

```lua
import "string"
var position = string.find("hello", "e")
position = string.find("hello", "missing")
if position == nil then
    print("No match")
end
```

## Rules that remain in effect

- Reading or assigning an undeclared name still warns and errors. Assigning
  nil does not declare a missing variable.
- Constants still reject reassignment, including assigning nil to a constant
  that already holds nil. Same-scope constant redeclaration remains an error.
- Type annotations retain their existing compatibility rules. `Nil` accepts
  only nil; `Number`, `String`, `Boolean`, `List`, `Object`, `Module`, and
  `Function` reject nil. A failed assignment keeps the previous value. There
  is no optional or union annotation syntax; use an unannotated variable for
  a value that can alternate between nil and another type.
- Nil remains false in conditions, compares equal to nil, has type `Nil`,
  and formats as `nil`. `false` is the only other false value.
- Lists and objects continue to store nil entries. Assigning nil to a list
  slot preserves its index and the list's length. Assigning nil to an object
  property preserves that property's presence: `object.has` returns true.
- Use `list.remove(items, index)` or `object.remove(record, key)` to delete a
  collection entry. List removal shifts later entries down by one index.
  These operations can return a stored nil; removing a missing object key
  also returns nil without a warning.
- Missing object properties and invalid or out-of-range list reads retain
  their warnings and return nil. The returned nil can now be assigned to an
  existing unannotated or `Nil`-annotated variable without deleting it.

The authoritative rules are in [SPEC.md](SPEC.md). Regression coverage is in
`tests/nil_values.rs`, `tests/functions.rs`, `tests/loops.rs`, and the runtime
environment unit tests.
