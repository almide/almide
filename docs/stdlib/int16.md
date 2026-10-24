# int16

Width conversions for `Int16` (16-bit signed, -32,768 … 32,767). Auto-imported — no `import`
needed.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let x: Int16 = 1200
let wide = x.to_int64()      // resolves to int16.to_int64(x)
let text = x.to_string()
```

> **Convert to `Int` before passing to an `Int`-taking function.** A narrower
> result (`x.to_int32()`) is not implicitly widened, and the checker currently
> accepts the mismatch where the generated code does not build ([#867]).
> `x.to_int64()` is the idiom that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_int16(x)` widens, then `int.to_<dst>(...)` narrows or re-widens.
Both hops collapse — Rust folds the double cast, and the wasm renderer resolves
them inline — so this costs nothing at runtime and both targets agree by
construction.

The rules match Rust's `as`:

| Direction | Behaviour |
|---|---|
| Narrowing an integer | WRAPS (two's complement truncation), never traps |
| Widening a signed integer | Sign-extends |
| Widening an unsigned integer | Zero-extends |
| Float → integer | Saturating truncation (NaN → 0, out-of-range → the nearest bound) |
| Integer → float | Nearest representable, ties to even |

Because narrowing wraps rather than trapping, convert deliberately: check the
range first when a value must round-trip.

## Functions

`to_int8`, `to_int16`, `to_int32`, `to_int64`, `to_uint8`, `to_uint16`,
`to_uint32`, `to_uint64`, `to_float32`, `to_float64` and `to_string`, minus the
identity conversion for this module's own type. The exact set is the machine-owned
signature index below.

Literals of a sized type are range-checked at compile time — `let a: Int16 = 40000`
is an E024 error, not a silent fold.

## Checked / saturating narrowings and bounds

Every **lossy** pair (source range does not fit the destination) also has
`to_<dst>_checked` (`None` on overflow) and `to_<dst>_saturating` (clamp to the
destination range); lossless pairs deliberately have only the plain form — an
always-`Some` checked variant would be noise. `min_value()` / `max_value()`
give this type's bounds. The whole surface is derived from the range table and
machine-enforced by the numeric-matrix gate in `almide docs-gen --check` (#956).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (50 functions)

```
// Wraps to 8 bits; 128 becomes -128.
// @since 0.15.0 or earlier
int16.to_int8(x: Int16) -> Int8

// Sign-extending widen; always exact.
// @since 0.15.0 or earlier
int16.to_int32(x: Int16) -> Int32

// Sign-extending widen; always exact.
// @since 0.15.0 or earlier
int16.to_int64(x: Int16) -> Int64

// Wraps to 8 bits; -1 becomes 255.
// @since 0.15.0 or earlier
int16.to_uint8(x: Int16) -> UInt8

// Wraps to 16 bits; -1 becomes 65535.
// @since 0.15.0 or earlier
int16.to_uint16(x: Int16) -> UInt16

// Wraps to 32 bits; -1 becomes 2^32-1.
// @since 0.15.0 or earlier
int16.to_uint32(x: Int16) -> UInt32

// Wraps to 64 bits; -1 becomes 2^64-1.
// @since 0.15.0 or earlier
int16.to_uint64(x: Int16) -> UInt64

// Same value as Float32; always exact.
// @since 0.15.0 or earlier
int16.to_float32(x: Int16) -> Float32

// Same value as Float64; always exact.
// @since 0.15.0 or earlier
int16.to_float64(x: Int16) -> Float64

// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int16.to_string(x: Int16) -> String

// some(x), or none if outside -128..127.
// @since 0.38.0
int16.to_int8_checked(x: Int16) -> Option[Int8]

// Clamps x to -128..127.
// @since 0.38.0
int16.to_int8_saturating(x: Int16) -> Int8

// some(x), or none if outside 0..255.
// @since 0.38.0
int16.to_uint8_checked(x: Int16) -> Option[UInt8]

// Clamps x to 0..255.
// @since 0.38.0
int16.to_uint8_saturating(x: Int16) -> UInt8

// some(x), or none if x is negative.
// @since 0.38.0
int16.to_uint16_checked(x: Int16) -> Option[UInt16]

// Negative x clamps to 0.
// @since 0.38.0
int16.to_uint16_saturating(x: Int16) -> UInt16

// some(x), or none if x is negative.
// @since 0.38.0
int16.to_uint32_checked(x: Int16) -> Option[UInt32]

// Negative x clamps to 0.
// @since 0.38.0
int16.to_uint32_saturating(x: Int16) -> UInt32

// some(x), or none if x is negative.
// @since 0.38.0
int16.to_uint64_checked(x: Int16) -> Option[UInt64]

// Negative x clamps to 0.
// @since 0.38.0
int16.to_uint64_saturating(x: Int16) -> UInt64

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
```

<!-- END GENERATED SIGNATURE INDEX -->
