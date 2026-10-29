# float64

`Float64` (64-bit float, IEEE-754 binary64): bounds, arithmetic, bits, text, and the conversions into the
type. Auto-imported — no `import` needed.

`Float64` and the canonical `Float` share a representation but are distinct
types, so `Float64` gets its own module for the same reason `float32` does.

## Conversions

A conversion is named after the type it builds (R-L0-40): `float64.from(x)` takes
any numeric source — `Int`, a sized integer, `Float` or a sized float.

```almd
let x: Float64 = float64.from(1.5)
let r = float64.from(0.1)              // the same value, re-tagged
let exact = float64.from_checked(16777217) // some: every Int up to 2^53 is exact
let n = int.from_saturating(x)         // leaving Float64: the destination's family
let text = x.to_string()
```

| Function | Behaviour |
|---|---|
| `float64.from(x)` | the nearest Float64 (ties to even); NaN stays NaN |
| `float64.from_checked(x)` | `some` when Float64 holds x exactly, else `none` (a rounded value, NaN) |

To leave `Float64`, call the destination's family: `float.from(x)` widens exactly;
an integer destination chooses by name — `int.from(x)` aborts on NaN, an
infinity or an out-of-range value, `int.from_checked(x)` answers `none` for a
fraction, `int.from_saturating(x)` truncates toward zero and clamps (NaN is 0).

The exact surface is the machine-owned signature index below.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (24 functions)

```
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

// Float64 from ASCII-trimmed s, the grammar of float.parse; err on junk, inf, NaN.
// @since unreleased
float64.parse(s: String) -> Result[Float64, String]

// x as the nearest Float64 (ties to even); NaN stays NaN.
// @since unreleased
float64.from(x: S) -> Float64

// some(x as Float64) when Float64 holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float64.from_checked(x: S) -> Option[Float64]
```

<!-- END GENERATED SIGNATURE INDEX -->
