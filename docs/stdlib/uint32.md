# uint32

Width conversions for `UInt32` (32-bit unsigned, 0 … 4,294,967,295). Auto-imported — no `import`
needed.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let x: UInt32 = 4000000000
let wide = x.to_int64()      // resolves to uint32.to_int64(x)
let text = x.to_string()
```

> **Convert to `Int` before passing to an `Int`-taking function.** A narrower
> result (`x.to_int32()`) is not implicitly widened, and the checker currently
> accepts the mismatch where the generated code does not build ([#867]).
> `x.to_int64()` is the idiom that works on both targets.

[#867]: https://github.com/almide/almide/issues/867

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_uint32(x)` widens, then `int.to_<dst>(...)` narrows or re-widens.
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

Literals of a sized type are range-checked at compile time — `let a: UInt32 = 5000000000`
is an E024 error, not a silent fold.

## Checked / saturating narrowings and bounds

Every **lossy** pair (source range does not fit the destination) also has
`to_<dst>_checked` (`None` on overflow) and `to_<dst>_saturating` (clamp to the
destination range); lossless pairs deliberately have only the plain form — an
always-`Some` checked variant would be noise. `min_value()` / `max_value()`
give this type's bounds. The whole surface is derived from the range table and
machine-enforced by the numeric-matrix gate in `almide docs-gen --check` (#956).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (22 functions)

```
// Wraps to 8 bits; 255 becomes -1.
// @since 0.15.0 or earlier
uint32.to_int8(x: UInt32) -> Int8

// Wraps to 16 bits; 65535 becomes -1.
// @since 0.15.0 or earlier
uint32.to_int16(x: UInt32) -> Int16

// Wraps to 32 bits; 2^32-1 becomes -1.
// @since 0.15.0 or earlier
uint32.to_int32(x: UInt32) -> Int32

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint32.to_int64(x: UInt32) -> Int64

// Wraps to 8 bits; 256 becomes 0.
// @since 0.15.0 or earlier
uint32.to_uint8(x: UInt32) -> UInt8

// Wraps to 16 bits; 65536 becomes 0.
// @since 0.15.0 or earlier
uint32.to_uint16(x: UInt32) -> UInt16

// Zero-extending widen; always exact.
// @since 0.15.0 or earlier
uint32.to_uint64(x: UInt32) -> UInt64

// Float32 value; may round when x > 2^24.
// @since 0.15.0 or earlier
uint32.to_float32(x: UInt32) -> Float32

// Same value as Float64; always exact.
// @since 0.15.0 or earlier
uint32.to_float64(x: UInt32) -> Float64

// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint32.to_string(x: UInt32) -> String

// some(x), or none if x exceeds 127.
// @since 0.38.0
uint32.to_int8_checked(x: UInt32) -> Option[Int8]

// Clamps x to at most 127.
// @since 0.38.0
uint32.to_int8_saturating(x: UInt32) -> Int8

// some(x), or none if x exceeds 32767.
// @since 0.38.0
uint32.to_int16_checked(x: UInt32) -> Option[Int16]

// Clamps x to at most 32767.
// @since 0.38.0
uint32.to_int16_saturating(x: UInt32) -> Int16

// some(x), or none if x exceeds 2^31-1.
// @since 0.38.0
uint32.to_int32_checked(x: UInt32) -> Option[Int32]

// Clamps x to at most 2^31-1.
// @since 0.38.0
uint32.to_int32_saturating(x: UInt32) -> Int32

// some(x), or none if x exceeds 255.
// @since 0.38.0
uint32.to_uint8_checked(x: UInt32) -> Option[UInt8]

// Clamps x to at most 255.
// @since 0.38.0
uint32.to_uint8_saturating(x: UInt32) -> UInt8

// some(x), or none if x exceeds 65535.
// @since 0.38.0
uint32.to_uint16_checked(x: UInt32) -> Option[UInt16]

// Clamps x to at most 65535.
// @since 0.38.0
uint32.to_uint16_saturating(x: UInt32) -> UInt16

// Smallest UInt32: 0.
// @since 0.38.0
uint32.min_value() -> UInt32

// Largest UInt32: 4294967295.
// @since 0.38.0
uint32.max_value() -> UInt32
```

<!-- END GENERATED SIGNATURE INDEX -->
