# float64

Width conversions for `Float64` (64-bit float, IEEE-754 binary64). Auto-imported —
no `import` needed.

`Float64` and the canonical `Float` share a representation but are distinct types,
so `Float64` gets its own module for the same reason `float32` does: without it
the only route was the `float.to_string(float.from_float64(x))` pivot, which a
reader who has used `float32.to_string` has no reason to expect.

Every function is a conversion and is **UFCS-dispatched**, so the method form is
the idiomatic one:

```almd
let x: Float64 = 1.5
let narrow = x.to_float32()   // resolves to float64.to_float32(x)
let text = x.to_string()
```

## Conversion semantics

The bodies are pure Almide, routed through canonical `Float` (f64) as a
dimensional pivot: `float.from_float64(x)` re-tags, then `float.to_<dst>(...)`
narrows or converts. Both hops collapse — Rust folds the double cast, and the
wasm renderer resolves them inline — so this costs nothing at runtime and both
targets agree by construction.

The rules match Rust's `as`:

| Direction | Behaviour |
|---|---|
| Float → integer | Saturating truncation (NaN → 0, out-of-range → the nearest bound) |
| Narrowing a float | Nearest representable; out of binary32 range becomes an infinity |
| Integer → float | Nearest representable, ties to even |

## Functions

`to_int8`, `to_int16`, `to_int32`, `to_int64`, `to_uint8`, `to_uint16`,
`to_uint32`, `to_uint64`, `to_float32` and `to_string`. There is no `to_float64` —
that is the identity on this type. The exact set is the machine-owned signature
index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (10 functions)

```
// Truncates toward 0, saturating; NaN gives 0.
float64.to_int8(x: Float64) -> Int8

// Truncates toward 0, saturating; NaN gives 0.
float64.to_int16(x: Float64) -> Int16

// Truncates toward 0, saturating; NaN gives 0.
float64.to_int32(x: Float64) -> Int32

// Truncates toward 0, saturating; NaN gives 0.
float64.to_int64(x: Float64) -> Int64

// Truncates toward 0, saturating; NaN gives 0.
float64.to_uint8(x: Float64) -> UInt8

// Truncates toward 0, saturating; NaN gives 0.
float64.to_uint16(x: Float64) -> UInt16

// Truncates toward 0, saturating; NaN gives 0.
float64.to_uint32(x: Float64) -> UInt32

// Truncates toward 0, saturating; NaN gives 0.
float64.to_uint64(x: Float64) -> UInt64

// Nearest Float32; overflow becomes infinity.
float64.to_float32(x: Float64) -> Float32

// Plain decimal, never an exponent; 2.0 keeps .0.
float64.to_string(x: Float64) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
