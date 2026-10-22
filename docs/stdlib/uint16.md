# uint16

Width conversions for `UInt16` (16-bit unsigned, 0 … 65,535). Auto-imported — no `import`
needed.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let x: UInt16 = 60000
let wide = x.to_int64()      // resolves to uint16.to_int64(x)
let text = x.to_string()
```

> **Convert to `Int` before passing to an `Int`-taking function.** A narrower
> result (`x.to_int32()`) is not implicitly widened, and the checker currently
> accepts the mismatch where the generated code does not build ([#867]).
> `x.to_int64()` is the idiom that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_uint16(x)` widens, then `int.to_<dst>(...)` narrows or re-widens.
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

Literals of a sized type are range-checked at compile time — `let a: UInt16 = 70000`
is an E024 error, not a silent fold.

## Checked / saturating narrowings and bounds

Every **lossy** pair (source range does not fit the destination) also has
`to_<dst>_checked` (`None` on overflow) and `to_<dst>_saturating` (clamp to the
destination range); lossless pairs deliberately have only the plain form — an
always-`Some` checked variant would be noise. `min_value()` / `max_value()`
give this type's bounds. The whole surface is derived from the range table and
machine-enforced by the numeric-matrix gate in `almide docs-gen --check` (#956).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (36 functions)

```
// Wraps to 8 bits; 255 becomes -1.
// @since 0.15.0 or earlier
uint16.to_int8(x: UInt16) -> Int8

// Wraps to 16 bits; 65535 becomes -1.
// @since 0.15.0 or earlier
uint16.to_int16(x: UInt16) -> Int16

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint16.to_int32(x: UInt16) -> Int32

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint16.to_int64(x: UInt16) -> Int64

// Wraps to 8 bits; 256 becomes 0.
// @since 0.15.0 or earlier
uint16.to_uint8(x: UInt16) -> UInt8

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint16.to_uint32(x: UInt16) -> UInt32

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint16.to_uint64(x: UInt16) -> UInt64

// Same value as Float32; always exact.
// @since 0.15.0 or earlier
uint16.to_float32(x: UInt16) -> Float32

// Same value as Float64; always exact.
// @since 0.15.0 or earlier
uint16.to_float64(x: UInt16) -> Float64

// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint16.to_string(x: UInt16) -> String

// some(x), or none if x exceeds 127.
// @since 0.38.0
uint16.to_int8_checked(x: UInt16) -> Option[Int8]

// Clamps x to at most 127.
// @since 0.38.0
uint16.to_int8_saturating(x: UInt16) -> Int8

// some(x), or none if x exceeds 32767.
// @since 0.38.0
uint16.to_int16_checked(x: UInt16) -> Option[Int16]

// Clamps x to at most 32767.
// @since 0.38.0
uint16.to_int16_saturating(x: UInt16) -> Int16

// some(x), or none if x exceeds 255.
// @since 0.38.0
uint16.to_uint8_checked(x: UInt16) -> Option[UInt8]

// Clamps x to at most 255.
// @since 0.38.0
uint16.to_uint8_saturating(x: UInt16) -> UInt8

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
```

<!-- END GENERATED SIGNATURE INDEX -->
