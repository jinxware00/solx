# Collection callbacks

Import `list` explicitly. These functions accept ordinary user functions,
closures, and native functions as callbacks:

```lua
import "list"

var values = [3, 1, 2]
var doubled = list.map(values, fn(value) return value * 2 end)
var selected = list.filter(values, fn(value) return value > 1 end)
var total = list.fold(values, 0, fn(acc, value) return acc + value end)
var ordered = list.sorted(values, fn(left, right) return left - right end)

print(doubled) -- [6, 2, 4]
print(selected) -- [3, 2]
print(total) -- 6
print(ordered) -- [1, 2, 3]
print(values) -- [3, 1, 2]
```

| Function | Callback arguments | Result |
| --- | --- | --- |
| `map(items, callback)` | One value | New List containing each callback result, including `nil`, in input order. |
| `filter(items, predicate)` | One value | New List containing input values whose predicate result is truthy, in input order. Only `false` and `nil` are false. |
| `fold(items, initial, callback)` | Current accumulator, then one value | Final accumulator after visiting input values from left to right. |
| `sorted(items, comparator)` | Left value, then right value | New, stably sorted List. Comparator returns a finite Number: negative for left before right, zero for a tie, positive for left after right. |

The functions require exactly the arguments shown, a List as the first
argument, and a Function as the callback. Arity is checked before argument
types. Call arguments evaluate once, from left to right, before validation.
Callbacks use the interpreter's normal arity, lexical-scope, return-value,
error-location, and call-trace rules; a callback is not a separate interpreter
or a string evaluated as source.

`map`, `filter`, and `sorted` return a distinct List even for empty inputs.
`fold` returns the initial value for an empty List, preserving references to
mutable values. Empty Lists trigger no callbacks; `sorted` also skips its
comparator for a one-element List. Callback Function type is still validated,
but callback arity is checked only if a call occurs.

## Snapshot and mutation behavior

Each function first takes a shallow snapshot of the List's membership. It
releases the source List's read borrow before invoking callbacks. Appending,
removing, replacing, or reordering source entries inside a callback therefore
does not change which snapshot entries are visited. Copying can fail on a
resource limit before any callback runs.

Nested Lists, Objects, functions, and other reference values remain shared.
A callback that changes an Object stored in an entry changes that Object
wherever it is referenced. `filter` retains the selected original values;
`map` retains the values returned by its callback. `fold` passes its current
accumulator directly to the next callback, including `nil` and mutable values.

```lua
import "list"

var values = [1, 2]
var result = list.map(values, fn(value)
    list.append(values, 99)
    return value
end)
print(result) -- [1, 2]
print(values) -- [1, 2, 99, 99]
```

The helpers do not mutate source membership themselves. Callback side effects
remain in place on success, callback failure, cancellation, or a resource limit.
They are not rolled back. A failed operation returns no partial result. If a
Rust host holds a mutable borrow of the source List, snapshot creation reports
a runtime error before calling the callback; shared read borrows permit the
snapshot, although a callback attempting a conflicting mutation still errors.

## Stable sorting

`sorted` uses a fallible, iterative merge sort. Equal comparisons choose the
earlier element, so a consistent comparator preserves the order of equal keys.
Both exact integer and finite floating Number results are accepted. Large
integer signs are compared exactly; they are not converted to floating point.
Boolean, `nil`, other types, and nonfinite Numbers are comparator-result errors.

Comparators should give consistent ordering results. An inconsistent or
nontransitive comparator still terminates within the bounded merge procedure
and cannot trigger a standard-library sort panic, but its result need not be
globally ordered. Callback errors and execution limits still abort normally.
The number and order of comparator calls are implementation details.

The existing `list.sort(items)` is unchanged: it sorts a uniform List of finite
Numbers or Strings in place and takes no callback. Use `sorted` for a separate
result and a custom comparator.

## Limits and errors

Snapshot copies, each visited element, and every merge movement consume the
same execution budget as surrounding code. Callback evaluation also consumes
that budget and shares cancellation, deadlines, and user-call/evaluation-depth
limits. This includes callbacks that are themselves native list functions.
The implementation does not reset limits while invoking a callback.

Ordinary callback/validation errors can be caught with `try`/`catch`; execution
limits and cancellation remain uncatchable. Scopes unwind under the normal
interpreter rules, preserving completed side effects, and the interpreter can
run again afterward. Cancellation remains sticky until its host token is reset.
These cooperative checks do not add a general heap budget or preempt a single
uninterrupted operation performed inside another native function.
