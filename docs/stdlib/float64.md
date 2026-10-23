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

## Signature index (31 functions)

```
// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_int8(x: Float64) -> Int8

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_int16(x: Float64) -> Int16

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_int32(x: Float64) -> Int32

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_int64(x: Float64) -> Int64

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_uint8(x: Float64) -> UInt8

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_uint16(x: Float64) -> UInt16

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_uint32(x: Float64) -> UInt32

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.37.0 or earlier
float64.to_uint64(x: Float64) -> UInt64

// Nearest Float32; overflow becomes infinity.
// @since 0.37.0 or earlier
float64.to_float32(x: Float64) -> Float32

// Plain decimal, never an exponent; 2.0 keeps .0.
// @since 0.37.0 or earlier
float64.to_string(x: Float64) -> String

// -1, 0 or 1 in IEEE 754 total order: -0.0 < 0.0, NaN above inf, NaN equal to NaN.
// @since unreleased
float64.compare(a: Float64, b: Float64) -> Int

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since unreleased
float64.min(a: Float64, b: Float64) -> Float64

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since unreleased
float64.max(a: Float64, b: Float64) -> Float64

// x limited to lo..hi; lo > hi or a NaN bound aborts.
// @since unreleased
float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64

// True only for NaN.
// @since unreleased
float64.is_nan(x: Float64) -> Bool

// True for inf and -inf; false for NaN.
// @since unreleased
float64.is_infinite(x: Float64) -> Bool

// True unless x is inf, -inf or NaN.
// @since unreleased
float64.is_finite(x: Float64) -> Bool

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since unreleased
float64.abs(x: Float64) -> Float64

// Round toward -inf; -0.5 gives -1.0.
// @since unreleased
float64.floor(x: Float64) -> Float64

// Round toward +inf; -0.5 gives -0.0.
// @since unreleased
float64.ceil(x: Float64) -> Float64

// Nearest whole value; halves round away from zero.
// @since unreleased
float64.round(x: Float64) -> Float64

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float64.trunc(x: Float64) -> Float64

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float64.signum(x: Float64) -> Float64

// The 64 IEEE-754 binary64 bits as Int; any NaN gives the canonical NaN.
// @since unreleased
float64.to_bits(x: Float64) -> Int

// Float64 whose IEEE-754 binary64 bits are bits.
// @since unreleased
float64.from_bits(bits: Int) -> Float64

// Positive infinity.
// @since unreleased
float64.infinity() -> Float64

// The one quiet NaN (bits 0x7ff8000000000000).
// @since unreleased
float64.nan() -> Float64

// Gap between 1.0 and the next Float64: 2^-52.
// @since unreleased
float64.epsilon() -> Float64

// Largest finite Float64, about 1.7976931348623157e308.
// @since unreleased
float64.max_value() -> Float64

// Most negative finite Float64 (= -max_value()), not the smallest positive.
// @since unreleased
float64.min_value() -> Float64

// Float64 from trimmed s, the grammar of float.parse; err on junk.
// @since unreleased
float64.parse(s: String) -> Result[Float64, String]
```

<!-- END GENERATED SIGNATURE INDEX -->
