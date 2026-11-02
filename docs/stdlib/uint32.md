# uint32

`UInt32` (32-bit unsigned, 0 … 4,294,967,295): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

## Conversions

A conversion is named after the type it builds (R-L0-40): `uint32.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: UInt32 = uint32.from(12)
let wide = int.from(x)                 // leaving UInt32: the destination's `from`
let low = uint32.from_wrapping(4294967340)// 44: the low 32 bits, two's complement
let fits = uint32.from_checked(4294967296)// none: 4294967296 is not a UInt32
let top = uint32.from_saturating(float.infinity()) // 4294967295: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `uint32.from(x)` | x toward zero; NaN, an infinity or a value outside UInt32 aborts |
| `uint32.from_checked(x)` | `some` when UInt32 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `uint32.from_saturating(x)` | x toward zero, clamped to UInt32's range; NaN is 0 |
| `uint32.from_wrapping(x)` | integer sources only: the low 32 bits, two's complement |

To leave `UInt32`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened: the checker rejects the mismatched types.
> Use `int.from(x)` to request the conversion explicitly on both targets;
> an out-of-range value aborts (use the checked, saturating, or wrapping
> variant when that is the intended policy).

Literals of a sized type are range-checked at compile time — `let a: UInt32 = 5000000000`
is an E024 error, not a silent fold.

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (35 functions)

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint32.to_string(x: UInt32) -> String

// Smallest UInt32: 0.
// @since 0.38.0
uint32.min_value() -> UInt32

// Largest UInt32: 4294967295.
// @since 0.38.0
uint32.max_value() -> UInt32

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint32.compare(a: UInt32, b: UInt32) -> Int

// Lesser of a and b.
// @since unreleased
uint32.min(a: UInt32, b: UInt32) -> UInt32

// Greater of a and b.
// @since unreleased
uint32.max(a: UInt32, b: UInt32) -> UInt32

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint32.floor_div(a: UInt32, b: UInt32) -> UInt32

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint32.modulo(a: UInt32, b: UInt32) -> UInt32

// base raised to exp, wrapping to 32 bits like *; exp < 0 aborts.
// @since unreleased
uint32.pow(base: UInt32, exp: Int) -> UInt32

// True when x is a multiple of 2.
// @since unreleased
uint32.is_even(x: UInt32) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint32.is_odd(x: UInt32) -> Bool

// Bitwise AND.
// @since unreleased
uint32.band(a: UInt32, b: UInt32) -> UInt32

// Bitwise OR.
// @since unreleased
uint32.bor(a: UInt32, b: UInt32) -> UInt32

// Bitwise XOR.
// @since unreleased
uint32.bxor(a: UInt32, b: UInt32) -> UInt32

// Bitwise complement of all 32 bits.
// @since unreleased
uint32.bnot(x: UInt32) -> UInt32

// a shifted left by n bits; n is taken mod 32.
// @since unreleased
uint32.bshl(a: UInt32, n: Int) -> UInt32

// Zero-filling right shift; n is taken mod 32.
// @since unreleased
uint32.bshr(a: UInt32, n: Int) -> UInt32

// Zero bits above the highest set bit, counted in 32 bits; 32 for 0.
// @since unreleased
uint32.count_leading_zeros(x: UInt32) -> Int

// Zero bits below the lowest set bit; 32 for 0.
// @since unreleased
uint32.count_trailing_zeros(x: UInt32) -> Int

// Number of set bits among the 32.
// @since unreleased
uint32.count_ones(x: UInt32) -> Int

// The 4 bytes in reverse order (endian flip).
// @since unreleased
uint32.byte_swap(x: UInt32) -> UInt32

// a + b wrapped to 32 bits.
// @since unreleased
uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32

// a - b wrapped to 32 bits.
// @since unreleased
uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32

// a * b wrapped to 32 bits.
// @since unreleased
uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32

// The 32 bits of a rotated left by n; n is taken mod 32.
// @since unreleased
uint32.rotate_left(a: UInt32, n: Int) -> UInt32

// The 32 bits of a rotated right by n; n is taken mod 32.
// @since unreleased
uint32.rotate_right(a: UInt32, n: Int) -> UInt32

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint32.bit_length(x: UInt32) -> Int

// Decimal UInt32 from ASCII-trimmed s; err on junk, a sign of -, or above 4294967295.
// @since unreleased
uint32.parse(s: String) -> Result[UInt32, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint32.to_string_radix(x: UInt32, radix: Int) -> String

// UInt32 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 4294967295.
// @since unreleased
uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]

// x as UInt32, toward zero; NaN, an infinity or a value outside UInt32 aborts.
// @since unreleased
uint32.from(x: S) -> UInt32

// some(x as UInt32) when UInt32 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint32.from_checked(x: S) -> Option[UInt32]

// x as UInt32, toward zero, clamped to UInt32's range; NaN is 0.
// @since unreleased
uint32.from_saturating(x: S) -> UInt32

// The low 32 bits of integer x as UInt32 (two's complement).
// @since unreleased
uint32.from_wrapping(x: S) -> UInt32
```

<!-- END GENERATED SIGNATURE INDEX -->
