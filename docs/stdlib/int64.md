# int64

`Int64` (64-bit signed integer): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

`Int64` and the canonical `Int` share a representation but are distinct types,
so `Int64` gets its own module for the same reason `int32` does: the bounds,
arithmetic, text and conversions of the type live where its name says.

## Conversions

A conversion is named after the type it builds (R-L0-40): `int64.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: Int64 = int64.from(12)
let wide = int.from(x)                 // leaving Int64: the destination's `from`
let same = int64.from_wrapping(-1)     // -1: Int is already 64 bits wide
let fits = int64.from_checked(0.5)     // none: 0.5 has a fraction
let top = int64.from_saturating(float.infinity()) // 9223372036854775807: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `int64.from(x)` | x toward zero; NaN, an infinity or a value outside Int64 aborts |
| `int64.from_checked(x)` | `some` when Int64 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `int64.from_saturating(x)` | x toward zero, clamped to Int64's range; NaN is 0 |
| `int64.from_wrapping(x)` | integer sources only: the low 64 bits, two's complement |

To leave `Int64`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened, and the checker currently accepts the mismatch
> where the generated code does not build ([#867]). `int.from(x)` is the idiom
> that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (37 functions)

```
// Decimal digits, with - when negative.
// @since 0.37.0 or earlier
int64.to_string(x: Int64) -> String

// Smallest Int64: -2^63.
// @since 0.38.0
int64.min_value() -> Int64

// Largest Int64: 2^63-1.
// @since 0.38.0
int64.max_value() -> Int64

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int64.compare(a: Int64, b: Int64) -> Int

// Lesser of a and b.
// @since unreleased
int64.min(a: Int64, b: Int64) -> Int64

// Greater of a and b.
// @since unreleased
int64.max(a: Int64, b: Int64) -> Int64

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int64.floor_div(a: Int64, b: Int64) -> Int64

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int64.modulo(a: Int64, b: Int64) -> Int64

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
int64.pow(base: Int64, exp: Int) -> Int64

// True when x is a multiple of 2.
// @since unreleased
int64.is_even(x: Int64) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int64.is_odd(x: Int64) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int64.abs(x: Int64) -> Int64

// -1, 0 or 1 by the sign of x.
// @since unreleased
int64.sign(x: Int64) -> Int

// Bitwise AND.
// @since unreleased
int64.band(a: Int64, b: Int64) -> Int64

// Bitwise OR.
// @since unreleased
int64.bor(a: Int64, b: Int64) -> Int64

// Bitwise XOR.
// @since unreleased
int64.bxor(a: Int64, b: Int64) -> Int64

// Bitwise complement of all 64 bits.
// @since unreleased
int64.bnot(x: Int64) -> Int64

// a shifted left by n bits; n is taken mod 64.
// @since unreleased
int64.bshl(a: Int64, n: Int) -> Int64

// Sign-filling right shift; n is taken mod 64.
// @since unreleased
int64.bshr(a: Int64, n: Int) -> Int64

// Zero bits above the highest set bit, counted in 64 bits; 64 for 0.
// @since unreleased
int64.count_leading_zeros(x: Int64) -> Int

// Zero bits below the lowest set bit; 64 for 0.
// @since unreleased
int64.count_trailing_zeros(x: Int64) -> Int

// Number of set bits among the 64.
// @since unreleased
int64.count_ones(x: Int64) -> Int

// The 8 bytes in reverse order (endian flip).
// @since unreleased
int64.byte_swap(x: Int64) -> Int64

// a + b wrapped to 64 bits.
// @since unreleased
int64.wrap_add(a: Int64, b: Int64) -> Int64

// a - b wrapped to 64 bits.
// @since unreleased
int64.wrap_sub(a: Int64, b: Int64) -> Int64

// a * b wrapped to 64 bits.
// @since unreleased
int64.wrap_mul(a: Int64, b: Int64) -> Int64

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since unreleased
int64.rotate_left(a: Int64, n: Int) -> Int64

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since unreleased
int64.rotate_right(a: Int64, n: Int) -> Int64

// Bits needed to hold x; 0 for 0, 64 if negative.
// @since unreleased
int64.bit_length(x: Int64) -> Int

// Decimal Int64 from ASCII-trimmed s; err on junk or overflow.
// @since unreleased
int64.parse(s: String) -> Result[Int64, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int64.to_string_radix(x: Int64, radix: Int) -> String

// Int64 from trimmed s in radix, the grammar of int.parse_radix; err on junk or overflow.
// @since unreleased
int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]

// x as Int64, toward zero; NaN, an infinity or a value outside Int64 aborts.
// @since unreleased
int64.from(x: S) -> Int64

// some(x as Int64) when Int64 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int64.from_checked(x: S) -> Option[Int64]

// x as Int64, toward zero, clamped to Int64's range; NaN is 0.
// @since unreleased
int64.from_saturating(x: S) -> Int64

// The low 64 bits of integer x as Int64 (two's complement).
// @since unreleased
int64.from_wrapping(x: S) -> Int64
```

<!-- END GENERATED SIGNATURE INDEX -->
