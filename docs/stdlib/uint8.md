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

## Signature index (14 functions)

```
uint8.to_int8(x: UInt8) -> Int8
uint8.to_int16(x: UInt8) -> Int16
uint8.to_int32(x: UInt8) -> Int32
uint8.to_int64(x: UInt8) -> Int64
uint8.to_uint16(x: UInt8) -> UInt16
uint8.to_uint32(x: UInt8) -> UInt32
uint8.to_uint64(x: UInt8) -> UInt64
uint8.to_float32(x: UInt8) -> Float32
uint8.to_float64(x: UInt8) -> Float64
uint8.to_string(x: UInt8) -> String
uint8.to_int8_checked(x: UInt8) -> Option[Int8]
uint8.to_int8_saturating(x: UInt8) -> Int8
uint8.min_value() -> UInt8
uint8.max_value() -> UInt8
```

<!-- END GENERATED SIGNATURE INDEX -->
