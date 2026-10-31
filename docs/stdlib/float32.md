# float32

`Float32` (32-bit float, IEEE-754 binary32): bounds, arithmetic, bits, text, and the conversions into the
type. Auto-imported — no `import` needed.

`Float32` has binary32 precision: conversions and arithmetic results are
rounded to that precision. A conversion outside binary32's finite range
becomes an infinity rather than an error.

## Conversions

A conversion is named after the type it builds (R-L0-40): `float32.from(x)` takes
any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: Float32 = float32.from(1.5)
let r = float32.from(0.1)              // the nearest Float32, rounded once
let exact = float32.from_checked(0.1)  // none: 0.1 is not a binary32 value
let n = int.from_saturating(x)         // leaving Float32: the destination's family
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `float32.from(x)` | the nearest Float32 (ties to even); NaN stays NaN |
| `float32.from_checked(x)` | `some` when Float32 holds x exactly, else `none` (a rounded value, NaN) |

To leave `Float32`, call the destination's family: `float.from(x)` widens exactly;
an integer destination chooses by name — `int.from(x)` aborts on NaN, an
infinity or an out-of-range value, `int.from_checked(x)` answers `none` for a
fraction, `int.from_saturating(x)` truncates toward zero and clamps (NaN is 0).

The exact surface is the machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (24 functions)

```
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

// Float32 from trimmed s, the grammar of float.parse; err on junk; overflow gives inf.
// @since unreleased
float32.parse(s: String) -> Result[Float32, String]

// x as the nearest Float32 (ties to even), rounded once; NaN stays NaN.
// @since unreleased
float32.from(x: S) -> Float32

// some(x as Float32) when Float32 holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float32.from_checked(x: S) -> Option[Float32]
```

<!-- END GENERATED SIGNATURE INDEX -->
