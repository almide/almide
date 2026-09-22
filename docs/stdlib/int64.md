# int64

Width conversions for `Int64` (64-bit signed integer). Auto-imported — no `import` needed.

`Int64` and the canonical `Int` share a representation but are distinct types, so
`Int64` gets its own module for the same reason `int32` does: without it the only
route was the `int.to_string(int.from_int64(x))` pivot, which a reader who has
used `int32.to_string` has no reason to expect.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let n: Int64 = 7
let narrow = n.to_int16()    // resolves to int64.to_int16(n)
let text = n.to_string()
```

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_int64(x)` re-tags, then `int.to_<dst>(...)` narrows or widens.
Both hops collapse — Rust folds the double cast, and the wasm renderer resolves
them inline — so this costs nothing at runtime and both targets agree by
construction.

The rules match Rust's `as`:

| Direction | Behaviour |
|---|---|
| Narrowing an integer | WRAPS (two's complement truncation), never traps |
| Widening a signed integer | Sign-extends |
| Widening an unsigned integer | Zero-extends |
| Integer → float | Nearest representable, ties to even |

Because narrowing wraps rather than trapping, convert deliberately: check the
range first when a value must round-trip.

## Functions

`to_int8`, `to_int16`, `to_int32`, `to_uint8`, `to_uint16`, `to_uint32`,
`to_uint64`, `to_float32`, `to_float64` and `to_string`. There is no `to_int64` —
that is the identity on this type. The exact set is the machine-owned signature
index below.

## Checked / saturating narrowings and bounds

Every **lossy** pair (source range does not fit the destination) also has
`to_<dst>_checked` (`None` on overflow) and `to_<dst>_saturating` (clamp to the
destination range); lossless pairs deliberately have only the plain form — an
always-`Some` checked variant would be noise. `min_value()` / `max_value()`
give this type's bounds. The whole surface is derived from the range table and
machine-enforced by the numeric-matrix gate in `almide docs-gen --check` (#956).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (26 functions)

```
// Wraps to 8 bits; 128 becomes -128.
int64.to_int8(x: Int64) -> Int8

// Wraps to 16 bits; 32768 becomes -32768.
int64.to_int16(x: Int64) -> Int16

// Wraps to 32 bits; 2^31 becomes -2^31.
int64.to_int32(x: Int64) -> Int32

// Wraps to 8 bits; -1 becomes 255.
int64.to_uint8(x: Int64) -> UInt8

// Wraps to 16 bits; -1 becomes 65535.
int64.to_uint16(x: Int64) -> UInt16

// Wraps to 32 bits; -1 becomes 2^32-1.
int64.to_uint32(x: Int64) -> UInt32

// Wraps to 64 bits; -1 becomes 2^64-1.
int64.to_uint64(x: Int64) -> UInt64

// Float32 value; may round when abs(x) > 2^24.
int64.to_float32(x: Int64) -> Float32

// Float64 value; may round when abs(x) > 2^53.
int64.to_float64(x: Int64) -> Float64

// Decimal digits, with - when negative.
int64.to_string(x: Int64) -> String

// some(x), or none if outside -128..127.
int64.to_int8_checked(x: Int64) -> Option[Int8]

// Clamps x to -128..127.
int64.to_int8_saturating(x: Int64) -> Int8

// some(x), or none if outside -32768..32767.
int64.to_int16_checked(x: Int64) -> Option[Int16]

// Clamps x to -32768..32767.
int64.to_int16_saturating(x: Int64) -> Int16

// some(x), or none if outside -2^31..2^31-1.
int64.to_int32_checked(x: Int64) -> Option[Int32]

// Clamps x to -2^31..2^31-1.
int64.to_int32_saturating(x: Int64) -> Int32

// some(x), or none if outside 0..255.
int64.to_uint8_checked(x: Int64) -> Option[UInt8]

// Clamps x to 0..255.
int64.to_uint8_saturating(x: Int64) -> UInt8

// some(x), or none if outside 0..65535.
int64.to_uint16_checked(x: Int64) -> Option[UInt16]

// Clamps x to 0..65535.
int64.to_uint16_saturating(x: Int64) -> UInt16

// some(x), or none if outside 0..2^32-1.
int64.to_uint32_checked(x: Int64) -> Option[UInt32]

// Clamps x to 0..2^32-1.
int64.to_uint32_saturating(x: Int64) -> UInt32

// some(x), or none if x is negative.
int64.to_uint64_checked(x: Int64) -> Option[UInt64]

// Negative x clamps to 0.
int64.to_uint64_saturating(x: Int64) -> UInt64

// Smallest Int64: -2^63.
int64.min_value() -> Int64

// Largest Int64: 2^63-1.
int64.max_value() -> Int64
```

<!-- END GENERATED SIGNATURE INDEX -->
