# uint8

Width conversions for `UInt8` (8-bit unsigned, 0 … 255). Auto-imported — no `import`
needed.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let x: UInt8 = 200
let wide = x.to_int64()      // resolves to uint8.to_int64(x)
let text = x.to_string()
```

> **Convert to `Int` before passing to an `Int`-taking function.** A narrower
> result (`x.to_int32()`) is not implicitly widened, and the checker currently
> accepts the mismatch where the generated code does not build ([#867]).
> `x.to_int64()` is the idiom that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_uint8(x)` widens, then `int.to_<dst>(...)` narrows or re-widens.
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

Literals of a sized type are range-checked at compile time — `let a: UInt8 = 300`
is an E024 error, not a silent fold.

## Checked / saturating narrowings and bounds

Every **lossy** pair (source range does not fit the destination) also has
`to_<dst>_checked` (`None` on overflow) and `to_<dst>_saturating` (clamp to the
destination range); lossless pairs deliberately have only the plain form — an
always-`Some` checked variant would be noise. `min_value()` / `max_value()`
give this type's bounds. The whole surface is derived from the range table and
machine-enforced by the numeric-matrix gate in `almide docs-gen --check` (#956).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (32 functions)

```
// Wraps to 8 bits; 255 becomes -1.
// @since 0.15.0 or earlier
uint8.to_int8(x: UInt8) -> Int8

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint8.to_int16(x: UInt8) -> Int16

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint8.to_int32(x: UInt8) -> Int32

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint8.to_int64(x: UInt8) -> Int64

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint8.to_uint16(x: UInt8) -> UInt16

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint8.to_uint32(x: UInt8) -> UInt32

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint8.to_uint64(x: UInt8) -> UInt64

// Same value as Float32; always exact.
// @since 0.15.0 or earlier
uint8.to_float32(x: UInt8) -> Float32

// Same value as Float64; always exact.
// @since 0.15.0 or earlier
uint8.to_float64(x: UInt8) -> Float64

// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint8.to_string(x: UInt8) -> String

// some(x), or none if x exceeds 127.
// @since 0.38.0
uint8.to_int8_checked(x: UInt8) -> Option[Int8]

// Clamps x to at most 127.
// @since 0.38.0
uint8.to_int8_saturating(x: UInt8) -> Int8

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
```

<!-- END GENERATED SIGNATURE INDEX -->
