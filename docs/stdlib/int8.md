# int8

`Int8` (8-bit signed, -128 … 127): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

## Conversions

A conversion is named after the type it builds (R-L0-40): `int8.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: Int8 = int8.from(12)
let wide = int.from(x)                 // leaving Int8: the destination's `from`
let low = int8.from_wrapping(172)      // -84: the low 8 bits, two's complement
let fits = int8.from_checked(128)      // none: 128 is not an Int8
let top = int8.from_saturating(float.infinity()) // 127: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `int8.from(x)` | x toward zero; NaN, an infinity or a value outside Int8 aborts |
| `int8.from_checked(x)` | `some` when Int8 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `int8.from_saturating(x)` | x toward zero, clamped to Int8's range; NaN is 0 |
| `int8.from_wrapping(x)` | integer sources only: the low 8 bits, two's complement |

To leave `Int8`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened, and the checker currently accepts the mismatch
> where the generated code does not build ([#867]). `int.from(x)` is the idiom
> that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

Literals of a sized type are range-checked at compile time — `let a: Int8 = 200`
is an E024 error, not a silent fold.

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (37 functions)

```
// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int8.to_string(x: Int8) -> String

// Smallest Int8: -128.
// @since 0.38.0
int8.min_value() -> Int8

// Largest Int8: 127.
// @since 0.38.0
int8.max_value() -> Int8

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int8.compare(a: Int8, b: Int8) -> Int

// Lesser of a and b.
// @since unreleased
int8.min(a: Int8, b: Int8) -> Int8

// Greater of a and b.
// @since unreleased
int8.max(a: Int8, b: Int8) -> Int8

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int8.floor_div(a: Int8, b: Int8) -> Int8

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int8.modulo(a: Int8, b: Int8) -> Int8

// base raised to exp, wrapping to 8 bits like *; exp < 0 aborts.
// @since unreleased
int8.pow(base: Int8, exp: Int) -> Int8

// True when x is a multiple of 2.
// @since unreleased
int8.is_even(x: Int8) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int8.is_odd(x: Int8) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int8.abs(x: Int8) -> Int8

// -1, 0 or 1 by the sign of x.
// @since unreleased
int8.sign(x: Int8) -> Int

// Bitwise AND.
// @since unreleased
int8.band(a: Int8, b: Int8) -> Int8

// Bitwise OR.
// @since unreleased
int8.bor(a: Int8, b: Int8) -> Int8

// Bitwise XOR.
// @since unreleased
int8.bxor(a: Int8, b: Int8) -> Int8

// Bitwise complement of all 8 bits.
// @since unreleased
int8.bnot(x: Int8) -> Int8

// a shifted left by n bits; n is taken mod 8.
// @since unreleased
int8.bshl(a: Int8, n: Int) -> Int8

// Sign-filling right shift; n is taken mod 8.
// @since unreleased
int8.bshr(a: Int8, n: Int) -> Int8

// Zero bits above the highest set bit, counted in 8 bits; 8 for 0.
// @since unreleased
int8.count_leading_zeros(x: Int8) -> Int

// Zero bits below the lowest set bit; 8 for 0.
// @since unreleased
int8.count_trailing_zeros(x: Int8) -> Int

// Number of set bits among the 8.
// @since unreleased
int8.count_ones(x: Int8) -> Int

// The bytes in reverse order; one byte, so x itself.
// @since unreleased
int8.byte_swap(x: Int8) -> Int8

// a + b wrapped to 8 bits.
// @since unreleased
int8.wrap_add(a: Int8, b: Int8) -> Int8

// a - b wrapped to 8 bits.
// @since unreleased
int8.wrap_sub(a: Int8, b: Int8) -> Int8

// a * b wrapped to 8 bits.
// @since unreleased
int8.wrap_mul(a: Int8, b: Int8) -> Int8

// The 8 bits of a rotated left by n; n is taken mod 8.
// @since unreleased
int8.rotate_left(a: Int8, n: Int) -> Int8

// The 8 bits of a rotated right by n; n is taken mod 8.
// @since unreleased
int8.rotate_right(a: Int8, n: Int) -> Int8

// Bits needed to hold x; 0 for 0, 8 if negative.
// @since unreleased
int8.bit_length(x: Int8) -> Int

// Decimal Int8 from ASCII-trimmed s; err on junk or outside -128..127.
// @since unreleased
int8.parse(s: String) -> Result[Int8, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int8.to_string_radix(x: Int8, radix: Int) -> String

// Int8 from trimmed s in radix, the grammar of int.parse_radix; err on junk or outside -128..127.
// @since unreleased
int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]

// x as Int8, toward zero; NaN, an infinity or a value outside Int8 aborts.
// @since unreleased
int8.from(x: S) -> Int8

// some(x as Int8) when Int8 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int8.from_checked(x: S) -> Option[Int8]

// x as Int8, toward zero, clamped to Int8's range; NaN is 0.
// @since unreleased
int8.from_saturating(x: S) -> Int8

// The low 8 bits of integer x as Int8 (two's complement).
// @since unreleased
int8.from_wrapping(x: S) -> Int8
```

<!-- END GENERATED SIGNATURE INDEX -->
