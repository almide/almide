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

The bodies are pure Almide, routed through canonical `Float` (f64) as a
dimensional pivot: `float.from_float32(x)` widens, then `float.to_<dst>(...)`
narrows to the integer width or re-renders (`stdlib/float32.almd`). Both hops
collapse — Rust folds the double cast, and the wasm renderer resolves them
inline — so this costs nothing at runtime and both targets agree by
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

## Signature index (30 functions)

```
// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_int8(x: Float32) -> Int8

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_int16(x: Float32) -> Int16

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_int32(x: Float32) -> Int32

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_int64(x: Float32) -> Int64

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_uint8(x: Float32) -> UInt8

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_uint16(x: Float32) -> UInt16

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_uint32(x: Float32) -> UInt32

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float32.to_uint64(x: Float32) -> UInt64

// Exact widening; never rounds.
// @since 0.15.0 or earlier
float32.to_float64(x: Float32) -> Float64

// Shortest f32 round-trip digits, no exponent; 0.1 stays 0.1, 2.0 keeps .0.
// @since 0.15.0 or earlier
float32.to_string(x: Float32) -> String

// -1, 0 or 1 in IEEE 754 total order: -0.0 < 0.0, NaN above inf, NaN equal to NaN.
// @since unreleased
float32.compare(a: Float32, b: Float32) -> Int

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since unreleased
float32.min(a: Float32, b: Float32) -> Float32

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since unreleased
float32.max(a: Float32, b: Float32) -> Float32

// x limited to lo..hi; lo > hi or a NaN bound aborts.
// @since unreleased
float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32

// True only for NaN.
// @since unreleased
float32.is_nan(x: Float32) -> Bool

// True for inf and -inf; false for NaN.
// @since unreleased
float32.is_infinite(x: Float32) -> Bool

// True unless x is inf, -inf or NaN.
// @since unreleased
float32.is_finite(x: Float32) -> Bool

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since unreleased
float32.abs(x: Float32) -> Float32

// Round toward -inf; -0.5 gives -1.0.
// @since unreleased
float32.floor(x: Float32) -> Float32

// Round toward +inf; -0.5 gives -0.0.
// @since unreleased
float32.ceil(x: Float32) -> Float32

// Nearest whole value; halves round away from zero.
// @since unreleased
float32.round(x: Float32) -> Float32

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float32.trunc(x: Float32) -> Float32

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float32.signum(x: Float32) -> Float32

// The 32 IEEE-754 binary32 bits as a non-negative Int; any NaN gives 0x7fc00000.
// @since unreleased
float32.to_bits(x: Float32) -> Int

// Float32 whose binary32 bits are the low 32 bits of bits.
// @since unreleased
float32.from_bits(bits: Int) -> Float32

// Positive infinity.
// @since unreleased
float32.infinity() -> Float32

// The one quiet NaN (bits 0x7fc00000).
// @since unreleased
float32.nan() -> Float32

// Gap between 1.0 and the next Float32: 2^-23.
// @since unreleased
float32.epsilon() -> Float32

// Largest finite Float32, about 3.4028235e38.
// @since unreleased
float32.max_value() -> Float32

// Most negative finite Float32 (= -max_value()), not the smallest positive.
// @since unreleased
float32.min_value() -> Float32
```

<!-- END GENERATED SIGNATURE INDEX -->
