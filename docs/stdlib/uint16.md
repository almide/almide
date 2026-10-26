# uint16

`UInt16` (16-bit unsigned, 0 … 65,535): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

## Conversions

A conversion is named after the type it builds (R-L0-40): `uint16.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: UInt16 = uint16.from(12)
let wide = int.from(x)                 // leaving UInt16: the destination's `from`
let low = uint16.from_wrapping(65580)  // 44: the low 16 bits, two's complement
let fits = uint16.from_checked(65536)  // none: 65536 is not a UInt16
let top = uint16.from_saturating(float.infinity()) // 65535: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `uint16.from(x)` | x toward zero; NaN, an infinity or a value outside UInt16 aborts |
| `uint16.from_checked(x)` | `some` when UInt16 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `uint16.from_saturating(x)` | x toward zero, clamped to UInt16's range; NaN is 0 |
| `uint16.from_wrapping(x)` | integer sources only: the low 16 bits, two's complement |

To leave `UInt16`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened, and the checker currently accepts the mismatch
> where the generated code does not build ([#867]). `int.from(x)` is the idiom
> that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

Literals of a sized type are range-checked at compile time — `let a: UInt16 = 70000`
is an E024 error, not a silent fold.

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (35 functions)

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint16.to_string(x: UInt16) -> String

// Smallest UInt16: 0.
// @since 0.38.0
uint16.min_value() -> UInt16

// Largest UInt16: 65535.
// @since 0.38.0
uint16.max_value() -> UInt16

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint16.compare(a: UInt16, b: UInt16) -> Int

// Lesser of a and b.
// @since unreleased
uint16.min(a: UInt16, b: UInt16) -> UInt16

// Greater of a and b.
// @since unreleased
uint16.max(a: UInt16, b: UInt16) -> UInt16

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint16.floor_div(a: UInt16, b: UInt16) -> UInt16

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint16.modulo(a: UInt16, b: UInt16) -> UInt16

// base raised to exp, wrapping to 16 bits like *; exp < 0 aborts.
// @since unreleased
uint16.pow(base: UInt16, exp: Int) -> UInt16

// True when x is a multiple of 2.
// @since unreleased
uint16.is_even(x: UInt16) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint16.is_odd(x: UInt16) -> Bool

// Bitwise AND.
// @since unreleased
uint16.band(a: UInt16, b: UInt16) -> UInt16

// Bitwise OR.
// @since unreleased
uint16.bor(a: UInt16, b: UInt16) -> UInt16

// Bitwise XOR.
// @since unreleased
uint16.bxor(a: UInt16, b: UInt16) -> UInt16

// Bitwise complement of all 16 bits.
// @since unreleased
uint16.bnot(x: UInt16) -> UInt16

// a shifted left by n bits; n is taken mod 16.
// @since unreleased
uint16.bshl(a: UInt16, n: Int) -> UInt16

// Zero-filling right shift; n is taken mod 16.
// @since unreleased
uint16.bshr(a: UInt16, n: Int) -> UInt16

// Zero bits above the highest set bit, counted in 16 bits; 16 for 0.
// @since unreleased
uint16.count_leading_zeros(x: UInt16) -> Int

// Zero bits below the lowest set bit; 16 for 0.
// @since unreleased
uint16.count_trailing_zeros(x: UInt16) -> Int

// Number of set bits among the 16.
// @since unreleased
uint16.count_ones(x: UInt16) -> Int

// The 2 bytes in reverse order (endian flip).
// @since unreleased
uint16.byte_swap(x: UInt16) -> UInt16

// a + b wrapped to 16 bits.
// @since unreleased
uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16

// a - b wrapped to 16 bits.
// @since unreleased
uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16

// a * b wrapped to 16 bits.
// @since unreleased
uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16

// The 16 bits of a rotated left by n; n is taken mod 16.
// @since unreleased
uint16.rotate_left(a: UInt16, n: Int) -> UInt16

// The 16 bits of a rotated right by n; n is taken mod 16.
// @since unreleased
uint16.rotate_right(a: UInt16, n: Int) -> UInt16

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint16.bit_length(x: UInt16) -> Int

// Decimal UInt16 from ASCII-trimmed s; err on junk, a sign of -, or above 65535.
// @since unreleased
uint16.parse(s: String) -> Result[UInt16, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
uint16.to_string_radix(x: UInt16, radix: Int) -> String

// UInt16 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 65535.
// @since unreleased
uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]

// x as UInt16, toward zero; NaN, an infinity or a value outside UInt16 aborts.
// @since unreleased
uint16.from(x: S) -> UInt16

// some(x as UInt16) when UInt16 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint16.from_checked(x: S) -> Option[UInt16]

// x as UInt16, toward zero, clamped to UInt16's range; NaN is 0.
// @since unreleased
uint16.from_saturating(x: S) -> UInt16

// The low 16 bits of integer x as UInt16 (two's complement).
// @since unreleased
uint16.from_wrapping(x: S) -> UInt16
```

<!-- END GENERATED SIGNATURE INDEX -->
