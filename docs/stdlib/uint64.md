# uint64

`UInt64` (64-bit unsigned, 0 … 18,446,744,073,709,551,615): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

## Conversions

A conversion is named after the type it builds (R-L0-40): `uint64.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: UInt64 = uint64.from(12)
let wide = int.from(x)                 // leaving UInt64: the destination's `from`
let low = uint64.from_wrapping(-1)     // 18446744073709551615: the low 64 bits, two's complement
let fits = uint64.from_checked(-1)     // none: -1 is not a UInt64
let top = uint64.from_saturating(float.infinity()) // 18446744073709551615: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `uint64.from(x)` | x toward zero; NaN, an infinity or a value outside UInt64 aborts |
| `uint64.from_checked(x)` | `some` when UInt64 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `uint64.from_saturating(x)` | x toward zero, clamped to UInt64's range; NaN is 0 |
| `uint64.from_wrapping(x)` | integer sources only: the low 64 bits, two's complement |

To leave `UInt64`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened: the checker rejects the mismatched types.
> Use `int.from(x)` to request the conversion explicitly on both targets;
> an out-of-range value aborts (use the checked, saturating, or wrapping
> variant when that is the intended policy).

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (35 functions)

```
// Decimal digits, no sign; exact up to 2^64-1.
// @since 0.15.0 or earlier
uint64.to_string(x: UInt64) -> String

// Smallest UInt64: 0.
// @since 0.38.0
uint64.min_value() -> UInt64

// Largest UInt64: 2^64-1.
// @since 0.38.0
uint64.max_value() -> UInt64

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint64.compare(a: UInt64, b: UInt64) -> Int

// Lesser of a and b.
// @since unreleased
uint64.min(a: UInt64, b: UInt64) -> UInt64

// Greater of a and b.
// @since unreleased
uint64.max(a: UInt64, b: UInt64) -> UInt64

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint64.floor_div(a: UInt64, b: UInt64) -> UInt64

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint64.modulo(a: UInt64, b: UInt64) -> UInt64

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
uint64.pow(base: UInt64, exp: Int) -> UInt64

// True when x is a multiple of 2.
// @since unreleased
uint64.is_even(x: UInt64) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint64.is_odd(x: UInt64) -> Bool

// Bitwise AND.
// @since unreleased
uint64.band(a: UInt64, b: UInt64) -> UInt64

// Bitwise OR.
// @since unreleased
uint64.bor(a: UInt64, b: UInt64) -> UInt64

// Bitwise XOR.
// @since unreleased
uint64.bxor(a: UInt64, b: UInt64) -> UInt64

// Bitwise complement of all 64 bits.
// @since unreleased
uint64.bnot(x: UInt64) -> UInt64

// a shifted left by n bits; n is taken mod 64.
// @since unreleased
uint64.bshl(a: UInt64, n: Int) -> UInt64

// Zero-filling right shift; n is taken mod 64.
// @since unreleased
uint64.bshr(a: UInt64, n: Int) -> UInt64

// Zero bits above the highest set bit, counted in 64 bits; 64 for 0.
// @since unreleased
uint64.count_leading_zeros(x: UInt64) -> Int

// Zero bits below the lowest set bit; 64 for 0.
// @since unreleased
uint64.count_trailing_zeros(x: UInt64) -> Int

// Number of set bits among the 64.
// @since unreleased
uint64.count_ones(x: UInt64) -> Int

// The 8 bytes in reverse order (endian flip).
// @since unreleased
uint64.byte_swap(x: UInt64) -> UInt64

// a + b wrapped to 64 bits.
// @since unreleased
uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64

// a - b wrapped to 64 bits.
// @since unreleased
uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64

// a * b wrapped to 64 bits.
// @since unreleased
uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since unreleased
uint64.rotate_left(a: UInt64, n: Int) -> UInt64

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since unreleased
uint64.rotate_right(a: UInt64, n: Int) -> UInt64

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint64.bit_length(x: UInt64) -> Int

// Decimal UInt64 from ASCII-trimmed s; err on junk, a sign of -, or above 2^64-1.
// @since unreleased
uint64.parse(s: String) -> Result[UInt64, String]

// Digits of x in radix, lowercase, no sign; exact up to 2^64-1; radix outside 2..36 aborts.
// @since unreleased
uint64.to_string_radix(x: UInt64, radix: Int) -> String

// UInt64 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 2^64-1.
// @since unreleased
uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]

// x as UInt64, toward zero; NaN, an infinity or a value outside UInt64 aborts.
// @since unreleased
uint64.from(x: S) -> UInt64

// some(x as UInt64) when UInt64 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint64.from_checked(x: S) -> Option[UInt64]

// x as UInt64, toward zero, clamped to UInt64's range; NaN is 0.
// @since unreleased
uint64.from_saturating(x: S) -> UInt64

// The low 64 bits of integer x as UInt64 (two's complement).
// @since unreleased
uint64.from_wrapping(x: S) -> UInt64
```

<!-- END GENERATED SIGNATURE INDEX -->
