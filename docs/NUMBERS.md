# Numbers in Solx 0.2

Solx exposes one `Number` type, stored internally as either a signed 64-bit
integer or an IEEE 754 binary64 decimal. `core.type` and `Number` annotations
accept both. This replaces the 0.1 runtime's all-binary64 representation.

## Literals and conversion

Digits without a decimal point or exponent are exact integers in
`-9223372036854775808..9223372036854775807`. Out-of-range integer forms error;
they are never silently rounded to decimals. The negative minimum literal is
accepted. Forms such as `12.0`, `.5`, and `12e0` use binary64. Decimal arithmetic
is approximate: for example, `0.1 + 0.2` need not equal `0.3`.

`num(text)` trims whitespace and applies the same integer/decimal distinction,
including an optional leading sign. `num(number)` preserves its representation.
Nonfinite values, invalid strings, overflowing integers, and nonzero decimal
values that underflow to zero are errors. Finite subnormals remain supported.
Use a decimal point or exponent explicitly when a binary64 value is intended.

```solx
var exact: Number = 9007199254740993
print(exact - 9007199254740992) -- 1
print(exact > 9007199254740992.0) -- true
```

## Arithmetic

Integer `+`, `-`, `*`, unary minus, `//`, `%`, and nonnegative integral powers
retain exact integer results. Overflow produces a catchable runtime error;
there is no wrapping or implicit decimal fallback. `0 ^ 0` is `1`.
The minimum integer cannot be negated or made positive with `math.abs`.

`//` rounds toward negative infinity. `%` is consistent with that quotient:
`a = (a // b) * b + a % b`, with a nonzero remainder having the divisor's sign.
The minimum integer modulo `-1` is zero, while floor-dividing it by `-1`
overflows. All division/modulo by zero, including negative decimal zero, errors.

`/` produces a decimal. Mixed integer/decimal operations and negative powers
also use decimals. An integer is converted only when binary64 can represent it
exactly: `9007199254740993 + 0.0` errors instead of silently losing a bit.
This rule applies even if an eventual result could have been exact, such as
dividing two equal but unrepresentable integers. `9007199254740992 / 2` works.

Decimal operations reject nonfinite results. Multiplication, division, and
power reject nonzero underflow to zero; representable subnormal results are
accepted. Ordinary decimal rounding is still possible. Numeric errors preserve
the normal error location, trace, `try`/`catch`, and assignment semantics.

## Comparisons, collections, and loops

Integer/decimal comparisons compare the actual numeric values without first
rounding the integer. Therefore `1 == 1.0`, but the largest integer is less
than the decimal `9223372036854775808.0`. Structural collection equality and
`list.find` use the same rules. Rust hosts may still construct nonfinite decimal
values: NaN remains unequal to every value, and native numeric APIs reject it.

`math.min`, `math.max`, and `list.sort` compare both representations exactly.
Integer `abs`, `floor`, `ceil`, and `round` preserve integers; `sqrt` requires
an exact decimal conversion. Lengths and string search indexes are integers.
Collection indexes accept exact integer values and integral decimals that fit
the actual valid range; conversions cannot saturate into a valid index.

All-integer numeric-for headers use exact integer counters. Mixed headers use
decimals and require exact integer conversions. Each iteration retains a fresh
binding as before. Advancing beyond the numeric range ends the loop because it
has passed the finite bound; an increment that rounds to the same counter still
errors. Assigning to the visible loop binding never alters the internal counter.

## Rust embedding compatibility

`Value::Integer(i64)` is a new public enum variant. `Value::Number(f64)` remains
the decimal constructor, so hosts can still create decimal values directly.
Exhaustive matches must handle the new variant. `Value::as_number()` retains
its existing approximate `Option<f64>` interface; match `Integer` directly when
integer precision matters. Solx arithmetic does not use that lossy accessor.

JSON has explicit integer/decimal conversion and size/depth rules; see
[JSON.md](JSON.md). These changes do not add arbitrary-precision integers,
decimal fixed-point arithmetic, or static numeric types.
