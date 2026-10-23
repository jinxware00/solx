# Raw multiline strings

Three double quotes open and close a raw String literal:

```lua
var message = """First line
Second line: {braces stay literal}
Path: C:\Users\name\file.txt"""
print(message)
```

The content is copied exactly. Newlines, indentation, leading and trailing
spaces, Unicode, and backslashes are preserved. LF and CRLF line endings remain
distinct in the value. There is no automatic trimming or indentation removal.
An initial newline after the opening delimiter and a final newline before the
closing delimiter both become part of the string.

Raw literals do not process escapes or interpolation. `\n` is a backslash
followed by `n`, and `{name}` is literal text. Comments, keywords, single quotes,
and isolated runs of one or two double quotes are ordinary content.

The first following sequence of three double quotes closes the literal. A
backslash immediately before that delimiter does not escape it. Six double
quotes together (`""""""`) produce an empty raw string. To include three double
quotes within a larger value, concatenate an ordinary escaped string:

```lua
var quoted = """before""" .. "\"\"\"" .. """after"""
```

The resulting value is an ordinary `String`. Existing string operations,
concatenation, equality, type annotations, function arguments, and collection
entries work without any new runtime type. Raw strings can also be written on
one line.

Ordinary single- and double-quoted literals keep their existing escape rules
and still reject unescaped newlines. Triple single quotes are not raw-string
delimiters. Code that previously placed ordinary double-quoted literals next
to each other without whitespace may now form a raw delimiter; add whitespace
between those literals or use explicit concatenation.

An unterminated raw string reports `unterminated raw string literal`, with its
source range extending from the first opening quote through end of input.
The REPL accepts additional lines until the closing delimiter arrives. Source
size and other configured execution limits continue to apply.

See [SPEC.md](SPEC.md) for the authoritative language rules and
`tests/raw_strings.rs` for lexical, runtime, checker, and REPL coverage.
