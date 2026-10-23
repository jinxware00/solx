# JSON

Import the built-in `json` module explicitly:

```lua
import "json"

var data = json.parse('{"name":"Solx","scores":[10,20],"optional":null}')
print(data.name)
print(json.stringify(data))
-- {"name":"Solx","optional":null,"scores":[10,20]}
```

`json.parse(text)` takes exactly one String and returns its decoded value.
`json.stringify(value)` takes exactly one value and returns a compact JSON
String. Both validate the argument count before inspecting types. Arguments
evaluate once, from left to right, under the normal function-call rules.
The namespace is read-only, supports import aliases, and is cached like the
other standard modules. Its functions are ordinary first-class functions.

| JSON | Solx |
| --- | --- |
| `null` | `nil` |
| `true`, `false` | Boolean |
| Number | Number, using the rules below |
| String | String |
| Array | List, preserving order and `nil` entries |
| Object | Object with String keys, preserving `nil` properties |

Strings and property names preserve Unicode. Escapes, control characters,
and UTF-16 surrogate pairs follow JSON syntax; invalid escapes and unpaired
surrogates are errors. Serialization escapes quotes, backslashes, and control
characters and writes other Unicode characters directly as UTF-8. Object keys
are sorted by their Unicode scalar order at every level. List order is retained.
Empty Lists and Objects serialize as `[]` and `{}` respectively.

Parsing accepts one JSON value with optional surrounding JSON whitespace.
Trailing data, comments, trailing commas, single-quoted strings, `NaN`, and
`Infinity` are rejected. Repeated object keys keep the last value, after all
numeric literals have passed validation. A JSON `null` property remains present
and can be distinguished from an absent property with `object.has`.

Integer tokens without a decimal point or exponent must fit the signed 64-bit
range, from `-9223372036854775808` to `9223372036854775807`. They retain every
integer digit, including values beyond the exact integer range of `f64`.
Out-of-range integer tokens are errors rather than rounded floating values.
The JSON negative-zero token `-0` retains its floating negative-zero sign.
Tokens with a decimal point or exponent use finite `f64` values and ordinary
binary floating-point rounding: `42.0` and `42e0` are floating values, and
`1e100` is accepted. A nonzero token that underflows to zero is rejected.
Representable nonzero subnormal values are accepted. Serialization preserves
signed 64-bit integers, permits finite floating values, and rejects nonfinite
values. Floating representations may change spelling, such as `42e0` becoming
`42.0`; decoding preserves the numeric value rather than its source spelling.
Both internal representations have the language-level type `Number`.

The functions impose these fixed limits:

- Input to `parse`: at most 1,048,576 UTF-8 bytes, including whitespace.
- Output from `stringify`: at most 1,048,576 UTF-8 bytes, including JSON escaping.
- Nesting: at most 64 arrays/objects on any path. A root container counts as one;
  scalar values do not add a level.

Input size and nesting are checked before recursive JSON parsing. Conversion
never descends beyond the nesting limit, and serialization checks the output
bound while writing. Repeated sharing is serialized at each occurrence and
counts toward the output bound. For example, two references to the same List
can produce `[[1],[1]]`; a List containing itself produces an error.

Functions, native functions, and Module namespaces cannot be serialized, even
when nested. Direct and indirect collection cycles, unavailable collection
borrows held by a Rust host, invalid JSON, unsafe numbers, and exceeded JSON
limits all produce catchable runtime errors. A failed conversion returns no
partial result and does not mutate the supplied collections. These limits bound
JSON conversion; they do not add a general runtime memory budget or change the
destruction rules for arbitrary graphs constructed by Rust hosts.

```lua
try
    json.parse("true false")
catch err
    print(err.message)
end
```
