# int16

`Int16` (16-bit signed, -32,768 … 32,767): bounds, arithmetic, bit operations, text, and the conversions
into the type. Auto-imported — no `import` needed.

## Conversions

A conversion is named after the type it builds (R-L0-40): `int16.from(x)` and its
siblings take any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: Int16 = int16.from(12)
let wide = int.from(x)                 // leaving Int16: the destination's `from`
let low = int16.from_wrapping(32812)   // -32724: the low 16 bits, two's complement
let fits = int16.from_checked(32768)   // none: 32768 is not an Int16
let top = int16.from_saturating(float.infinity()) // 32767: clamped to the range
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `int16.from(x)` | x toward zero; NaN, an infinity or a value outside Int16 aborts |
| `int16.from_checked(x)` | `some` when Int16 holds x exactly, else `none` (a fraction, NaN, out of range) |
| `int16.from_saturating(x)` | x toward zero, clamped to Int16's range; NaN is 0 |
| `int16.from_wrapping(x)` | integer sources only: the low 16 bits, two's complement |

To leave `Int16`, call the destination's family — `int.from(x)`, `float.from(x)`,
`int16.from_wrapping(x)` and so on. `from` is the right call wherever the
destination holds every value the source can carry; a narrowing destination
chooses abort, `none`, clamp or wrap by name, never silently.

> **Convert to `Int` before passing to an `Int`-taking function.** A sized value
> is not implicitly widened, and the checker currently accepts the mismatch
> where the generated code does not build ([#867]). `int.from(x)` is the idiom
> that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

Literals of a sized type are range-checked at compile time — `let a: Int16 = 40000`
is an E024 error, not a silent fold.

`min_value()` / `max_value()` give this type's bounds. The exact surface is the
machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (37 functions)

```
// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int16.to_string(x: Int16) -> String

// Smallest Int16: -32768.
// @since 0.38.0
int16.min_value() -> Int16

// Largest Int16: 32767.
// @since 0.38.0
int16.max_value() -> Int16

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int16.compare(a: Int16, b: Int16) -> Int

// Lesser of a and b.
// @since unreleased
int16.min(a: Int16, b: Int16) -> Int16

// Greater of a and b.
// @since unreleased
int16.max(a: Int16, b: Int16) -> Int16

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int16.floor_div(a: Int16, b: Int16) -> Int16

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int16.modulo(a: Int16, b: Int16) -> Int16

// base raised to exp, wrapping to 16 bits like *; exp < 0 aborts.
// @since unreleased
int16.pow(base: Int16, exp: Int) -> Int16

// True when x is a multiple of 2.
// @since unreleased
int16.is_even(x: Int16) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int16.is_odd(x: Int16) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int16.abs(x: Int16) -> Int16

// -1, 0 or 1 by the sign of x.
// @since unreleased
int16.sign(x: Int16) -> Int

// Bitwise AND.
// @since unreleased
int16.band(a: Int16, b: Int16) -> Int16

// Bitwise OR.
// @since unreleased
int16.bor(a: Int16, b: Int16) -> Int16

// Bitwise XOR.
// @since unreleased
int16.bxor(a: Int16, b: Int16) -> Int16

// Bitwise complement of all 16 bits.
// @since unreleased
int16.bnot(x: Int16) -> Int16

// a shifted left by n bits; n is taken mod 16.
// @since unreleased
int16.bshl(a: Int16, n: Int) -> Int16

// Sign-filling right shift; n is taken mod 16.
// @since unreleased
int16.bshr(a: Int16, n: Int) -> Int16

// Zero bits above the highest set bit, counted in 16 bits; 16 for 0.
// @since unreleased
int16.count_leading_zeros(x: Int16) -> Int

// Zero bits below the lowest set bit; 16 for 0.
// @since unreleased
int16.count_trailing_zeros(x: Int16) -> Int

// Number of set bits among the 16.
// @since unreleased
int16.count_ones(x: Int16) -> Int

// The 2 bytes in reverse order (endian flip).
// @since unreleased
int16.byte_swap(x: Int16) -> Int16

// a + b wrapped to 16 bits.
// @since unreleased
int16.wrap_add(a: Int16, b: Int16) -> Int16

// a - b wrapped to 16 bits.
// @since unreleased
int16.wrap_sub(a: Int16, b: Int16) -> Int16

// a * b wrapped to 16 bits.
// @since unreleased
int16.wrap_mul(a: Int16, b: Int16) -> Int16

// The 16 bits of a rotated left by n; n is taken mod 16.
// @since unreleased
int16.rotate_left(a: Int16, n: Int) -> Int16

// The 16 bits of a rotated right by n; n is taken mod 16.
// @since unreleased
int16.rotate_right(a: Int16, n: Int) -> Int16

// Bits needed to hold x; 0 for 0, 16 if negative.
// @since unreleased
int16.bit_length(x: Int16) -> Int

// Decimal Int16 from ASCII-trimmed s; err on junk or outside -32768..32767.
// @since unreleased
int16.parse(s: String) -> Result[Int16, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int16.to_string_radix(x: Int16, radix: Int) -> String

// Int16 from trimmed s in radix, the grammar of int.parse_radix; err on junk or outside -32768..32767.
// @since unreleased
int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]

// x as Int16, toward zero; NaN, an infinity or a value outside Int16 aborts.
// @since unreleased
int16.from(x: S) -> Int16

// some(x as Int16) when Int16 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int16.from_checked(x: S) -> Option[Int16]

// x as Int16, toward zero, clamped to Int16's range; NaN is 0.
// @since unreleased
int16.from_saturating(x: S) -> Int16

// The low 16 bits of integer x as Int16 (two's complement).
// @since unreleased
int16.from_wrapping(x: S) -> Int16
```

<!-- END GENERATED SIGNATURE INDEX -->
