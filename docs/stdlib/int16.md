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

## Signature index (22 functions)

```
int16.to_int8(x: Int16) -> Int8
int16.to_int32(x: Int16) -> Int32
int16.to_int64(x: Int16) -> Int64
int16.to_uint8(x: Int16) -> UInt8
int16.to_uint16(x: Int16) -> UInt16
int16.to_uint32(x: Int16) -> UInt32
int16.to_uint64(x: Int16) -> UInt64
int16.to_float32(x: Int16) -> Float32
int16.to_float64(x: Int16) -> Float64
int16.to_string(x: Int16) -> String
int16.to_int8_checked(x: Int16) -> Option[Int8]
int16.to_int8_saturating(x: Int16) -> Int8
int16.to_uint8_checked(x: Int16) -> Option[UInt8]
int16.to_uint8_saturating(x: Int16) -> UInt8
int16.to_uint16_checked(x: Int16) -> Option[UInt16]
int16.to_uint16_saturating(x: Int16) -> UInt16
int16.to_uint32_checked(x: Int16) -> Option[UInt32]
int16.to_uint32_saturating(x: Int16) -> UInt32
int16.to_uint64_checked(x: Int16) -> Option[UInt64]
int16.to_uint64_saturating(x: Int16) -> UInt64
int16.min_value() -> Int16
int16.max_value() -> Int16
```

<!-- END GENERATED SIGNATURE INDEX -->
