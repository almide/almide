# uint8

`UInt8` (8-bit unsigned, 0 … 255): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

## Conversions

A conversion is named after the type it builds (R-L0-40): `uint8.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: UInt8 = uint8.from(12)
let wide = int.from(x)                 // leaving UInt8: the destination's `from`
let low = uint8.from_wrapping(300)     // 44: the low 8 bits, two's complement
let fits = uint8.from_checked(256)     // none: 256 is not a UInt8
let top = uint8.from_saturating(float.infinity()) // 255: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `uint8.from(x)` | x toward zero; NaN, an infinity or a value outside UInt8 aborts |
| `uint8.from_checked(x)` | `some` when UInt8 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `uint8.from_saturating(x)` | x toward zero, clamped to UInt8's range; NaN is 0 |
| `uint8.from_wrapping(x)` | integer sources only: the low 8 bits, two's complement |

To leave `UInt8`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened: the checker rejects the mismatched types.
> Use `int.from(x)` to request the conversion explicitly on both targets;
> an out-of-range value aborts (use the checked, saturating, or wrapping
> variant when that is the intended policy).

Literals of a sized type are range-checked at compile time — `let a: UInt8 = 256`
is an E024 error, not a silent fold.

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (35 functions)

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint8.to_string(x: UInt8) -> String

// Smallest UInt8: 0.
// @since 0.38.0
uint8.min_value() -> UInt8

// Largest UInt8: 255.
// @since 0.38.0
uint8.max_value() -> UInt8

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint8.compare(a: UInt8, b: UInt8) -> Int

// Lesser of a and b.
// @since unreleased
uint8.min(a: UInt8, b: UInt8) -> UInt8

// Greater of a and b.
// @since unreleased
uint8.max(a: UInt8, b: UInt8) -> UInt8

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint8.floor_div(a: UInt8, b: UInt8) -> UInt8

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint8.modulo(a: UInt8, b: UInt8) -> UInt8

// base raised to exp, wrapping to 8 bits like *; exp < 0 aborts.
// @since unreleased
uint8.pow(base: UInt8, exp: Int) -> UInt8

// True when x is a multiple of 2.
// @since unreleased
uint8.is_even(x: UInt8) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint8.is_odd(x: UInt8) -> Bool

// Bitwise AND.
// @since unreleased
uint8.band(a: UInt8, b: UInt8) -> UInt8

// Bitwise OR.
// @since unreleased
uint8.bor(a: UInt8, b: UInt8) -> UInt8

// Bitwise XOR.
// @since unreleased
uint8.bxor(a: UInt8, b: UInt8) -> UInt8

// Bitwise complement of all 8 bits.
// @since unreleased
uint8.bnot(x: UInt8) -> UInt8

// a shifted left by n bits; n is taken mod 8.
// @since unreleased
uint8.bshl(a: UInt8, n: Int) -> UInt8

// Zero-filling right shift; n is taken mod 8.
// @since unreleased
uint8.bshr(a: UInt8, n: Int) -> UInt8

// Zero bits above the highest set bit, counted in 8 bits; 8 for 0.
// @since unreleased
uint8.count_leading_zeros(x: UInt8) -> Int

// Zero bits below the lowest set bit; 8 for 0.
// @since unreleased
uint8.count_trailing_zeros(x: UInt8) -> Int

// Number of set bits among the 8.
// @since unreleased
uint8.count_ones(x: UInt8) -> Int

// The bytes in reverse order; one byte, so x itself.
// @since unreleased
uint8.byte_swap(x: UInt8) -> UInt8

// a + b wrapped to 8 bits.
// @since unreleased
uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8

// a - b wrapped to 8 bits.
// @since unreleased
uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8

// a * b wrapped to 8 bits.
// @since unreleased
uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8

// The 8 bits of a rotated left by n; n is taken mod 8.
// @since unreleased
uint8.rotate_left(a: UInt8, n: Int) -> UInt8

// The 8 bits of a rotated right by n; n is taken mod 8.
// @since unreleased
uint8.rotate_right(a: UInt8, n: Int) -> UInt8

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint8.bit_length(x: UInt8) -> Int

// Decimal UInt8 from ASCII-trimmed s; err on junk, a sign of -, or above 255.
// @since unreleased
uint8.parse(s: String) -> Result[UInt8, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint8.to_string_radix(x: UInt8, radix: Int) -> String

// UInt8 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 255.
// @since unreleased
uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]

// x as UInt8, toward zero; NaN, an infinity or a value outside UInt8 aborts.
// @since unreleased
uint8.from(x: S) -> UInt8

// some(x as UInt8) when UInt8 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint8.from_checked(x: S) -> Option[UInt8]

// x as UInt8, toward zero, clamped to UInt8's range; NaN is 0.
// @since unreleased
uint8.from_saturating(x: S) -> UInt8

// The low 8 bits of integer x as UInt8 (two's complement).
// @since unreleased
uint8.from_wrapping(x: S) -> UInt8
```

<!-- END GENERATED SIGNATURE INDEX -->
