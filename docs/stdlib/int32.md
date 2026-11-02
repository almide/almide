# int32

`Int32` (32-bit signed): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

## Conversions

A conversion is named after the type it builds (R-L0-40): `int32.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: Int32 = int32.from(12)
let wide = int.from(x)                 // leaving Int32: the destination's `from`
let low = int32.from_wrapping(2147483692)// -2147483604: the low 32 bits, two's complement
let fits = int32.from_checked(2147483648)// none: 2147483648 is not an Int32
let top = int32.from_saturating(float.infinity()) // 2147483647: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `int32.from(x)` | x toward zero; NaN, an infinity or a value outside Int32 aborts |
| `int32.from_checked(x)` | `some` when Int32 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `int32.from_saturating(x)` | x toward zero, clamped to Int32's range; NaN is 0 |
| `int32.from_wrapping(x)` | integer sources only: the low 32 bits, two's complement |

To leave `Int32`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened: the checker rejects the mismatched types.
> Use `int.from(x)` to request the conversion explicitly on both targets;
> an out-of-range value aborts (use the checked, saturating, or wrapping
> variant when that is the intended policy).

Literals of a sized type are range-checked at compile time — `let a: Int32 = 3000000000`
is an E024 error, not a silent fold.

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (37 functions)

```
// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int32.to_string(x: Int32) -> String

// Smallest Int32: -2147483648.
// @since 0.38.0
int32.min_value() -> Int32

// Largest Int32: 2147483647.
// @since 0.38.0
int32.max_value() -> Int32

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int32.compare(a: Int32, b: Int32) -> Int

// Lesser of a and b.
// @since unreleased
int32.min(a: Int32, b: Int32) -> Int32

// Greater of a and b.
// @since unreleased
int32.max(a: Int32, b: Int32) -> Int32

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int32.floor_div(a: Int32, b: Int32) -> Int32

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int32.modulo(a: Int32, b: Int32) -> Int32

// base raised to exp, wrapping to 32 bits like *; exp < 0 aborts.
// @since unreleased
int32.pow(base: Int32, exp: Int) -> Int32

// True when x is a multiple of 2.
// @since unreleased
int32.is_even(x: Int32) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int32.is_odd(x: Int32) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int32.abs(x: Int32) -> Int32

// -1, 0 or 1 by the sign of x.
// @since unreleased
int32.sign(x: Int32) -> Int

// Bitwise AND.
// @since unreleased
int32.band(a: Int32, b: Int32) -> Int32

// Bitwise OR.
// @since unreleased
int32.bor(a: Int32, b: Int32) -> Int32

// Bitwise XOR.
// @since unreleased
int32.bxor(a: Int32, b: Int32) -> Int32

// Bitwise complement of all 32 bits.
// @since unreleased
int32.bnot(x: Int32) -> Int32

// a shifted left by n bits; n is taken mod 32.
// @since unreleased
int32.bshl(a: Int32, n: Int) -> Int32

// Sign-filling right shift; n is taken mod 32.
// @since unreleased
int32.bshr(a: Int32, n: Int) -> Int32

// Zero bits above the highest set bit, counted in 32 bits; 32 for 0.
// @since unreleased
int32.count_leading_zeros(x: Int32) -> Int

// Zero bits below the lowest set bit; 32 for 0.
// @since unreleased
int32.count_trailing_zeros(x: Int32) -> Int

// Number of set bits among the 32.
// @since unreleased
int32.count_ones(x: Int32) -> Int

// The 4 bytes in reverse order (endian flip).
// @since unreleased
int32.byte_swap(x: Int32) -> Int32

// a + b wrapped to 32 bits.
// @since unreleased
int32.wrap_add(a: Int32, b: Int32) -> Int32

// a - b wrapped to 32 bits.
// @since unreleased
int32.wrap_sub(a: Int32, b: Int32) -> Int32

// a * b wrapped to 32 bits.
// @since unreleased
int32.wrap_mul(a: Int32, b: Int32) -> Int32

// The 32 bits of a rotated left by n; n is taken mod 32.
// @since unreleased
int32.rotate_left(a: Int32, n: Int) -> Int32

// The 32 bits of a rotated right by n; n is taken mod 32.
// @since unreleased
int32.rotate_right(a: Int32, n: Int) -> Int32

// Bits needed to hold x; 0 for 0, 32 if negative.
// @since unreleased
int32.bit_length(x: Int32) -> Int

// Decimal Int32 from ASCII-trimmed s; err on junk or outside -2147483648..2147483647.
// @since unreleased
int32.parse(s: String) -> Result[Int32, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int32.to_string_radix(x: Int32, radix: Int) -> String

// Int32 from trimmed s in radix, the grammar of int.parse_radix; err on junk or outside -2147483648..2147483647.
// @since unreleased
int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]

// x as Int32, toward zero; NaN, an infinity or a value outside Int32 aborts.
// @since unreleased
int32.from(x: S) -> Int32

// some(x as Int32) when Int32 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int32.from_checked(x: S) -> Option[Int32]

// x as Int32, toward zero, clamped to Int32's range; NaN is 0.
// @since unreleased
int32.from_saturating(x: S) -> Int32

// The low 32 bits of integer x as Int32 (two's complement).
// @since unreleased
int32.from_wrapping(x: S) -> Int32
```

<!-- END GENERATED SIGNATURE INDEX -->
