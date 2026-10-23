# String utilities

Import `string` explicitly. Its functions validate argument counts before
inspecting values, evaluate arguments once under normal call rules, and report
qualified function names in runtime errors.

```lua
import "string"

print(string.replace("hello world", "world", "Solx")) -- hello Solx
print(string.len("aé🙂"))      -- 3 Unicode scalars
print(string.byte_len("aé🙂")) -- 7 UTF-8 bytes

var points = string.codepoints("aé🙂") -- [97, 233, 128578]
print(string.from_codepoints(points)) -- aé🙂
```

| Function | Behavior |
| --- | --- |
| `replace(text, needle, replacement)` | Exactly three Strings. Replace all literal, nonoverlapping matches, scanning left to right. Empty needle is an error; empty replacement deletes matches. Replacement text is not searched again. |
| `byte_len(text)` | Exactly one String. Return its UTF-8 byte length as an exact integer Number. |
| `codepoints(text)` | Exactly one String. Return a new List of exact integer Numbers, one Unicode scalar value per entry, in order. |
| `from_codepoints(items)` | Exactly one List of Numbers. Return a String containing those Unicode scalars in order. |

Replacement has no regular-expression, capture-group, or special replacement
syntax. For example, replacing `"aa"` with `"x"` in `"aaaaa"` returns `"xxa"`.
`"$1"` and backslashes in replacement values remain literal characters.

Code-point conversion preserves NUL, newlines, combining marks, and noncharacters
without normalization. Valid scalars are integers from 0 through 1,114,111,
excluding the surrogate range 55,296 through 57,343. `from_codepoints` accepts
both exact integer storage and finite floating Numbers with an integral value,
such as `65.0`. It rejects fractional values, nonfinite Numbers, invalid scalar
values, and non-Number entries. Errors identify the offending 1-based list
index. An empty List becomes an empty String; an empty String becomes an empty
List.

`replace` and `from_codepoints` permit at most 1,048,576 UTF-8 output bytes.
`codepoints` permits at most 1,048,576 output list entries. These are different
units: one scalar can occupy one to four UTF-8 bytes, and a List entry has its
normal runtime storage overhead. Output lengths are checked before allocation;
oversized results produce catchable runtime errors with no partial output.
Replacement can accept a larger input if its result shrinks to fit, and an
unused replacement string does not count toward the result size.

All four functions leave their inputs unchanged. If a Rust host holds a mutable
borrow of the List passed to `from_codepoints`, the function reports a runtime
error; releasing the borrow permits a retry. Shared read borrows are supported.

The existing `len`, `slice`, and `find` APIs still count Unicode scalars and use
1-based positions. They do not count UTF-8 bytes or user-perceived grapheme
clusters: for example, `"e"` followed by a combining accent has two scalars.
These new limits do not impose a general memory budget or change the other
string operations.
