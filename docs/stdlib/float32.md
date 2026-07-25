# float32

Width conversions for `Float32` (32-bit float, IEEE-754 binary32). Auto-imported — no `import`
needed.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let x: Float32 = 1.5
let wide = x.to_int64()      // resolves to float32.to_int64(x)
let text = x.to_string()
```

## Conversion semantics

The bodies are pure Almide, routed through canonical `Int` (i64) as a dimensional
pivot: `int.from_float32(x)` widens, then `int.to_<dst>(...)` narrows or re-widens.
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

`Float32` holds an f64 internally and narrows at the conversion boundary, so a
value outside binary32's range becomes an infinity rather than an error.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (10 functions)

```
float32.to_int8(x: Float32) -> Int8
float32.to_int16(x: Float32) -> Int16
float32.to_int32(x: Float32) -> Int32
float32.to_int64(x: Float32) -> Int64
float32.to_uint8(x: Float32) -> UInt8
float32.to_uint16(x: Float32) -> UInt16
float32.to_uint32(x: Float32) -> UInt32
float32.to_uint64(x: Float32) -> UInt64
float32.to_float64(x: Float32) -> Float64
float32.to_string(x: Float32) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
