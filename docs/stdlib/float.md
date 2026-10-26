# float

Floating-point operations. auto-imported.

### `float.to_string(x: Float) -> String`

Convert a float to its string representation.

```almd run
fn main() -> Unit = {
  println(float.to_string(3.14))
}
```
```output
3.14
```

### `float.to_int(n: Float) -> Int`

Truncate a float to an integer (rounds toward zero).

```almd run
fn main() -> Unit = {
  println("${float.to_int(3.9)}")
}
```
```output
3
```

`float.to_int` saturates: NaN gives `0` and a value past the `Int` range gives
`Int` max or min, so a bad input is indistinguishable from a real one. Its
twins say which one you mean — the conversion trio every lossy Float → integer
conversion carries:

- `float.to_int_checked(n: Float) -> Option[Int]` — `some` only when `n` is an
  integer in the `Int` range; `none` for NaN, ±inf, a fractional part or out of
  range.
- `float.to_int_saturating(n: Float) -> Int` — the clamp, spelled explicitly
  (the same value as `float.to_int`).

```almd run
fn main() -> Unit = {
  println("${float.to_int_checked(12.0)} ${float.to_int_checked(12.9)} ${float.to_int_checked(1.0 / 0.0)}")
  println("${float.to_int_saturating(-1.0 / 0.0)}")
}
```
```output
some(12) none none
-9223372036854775808
```

### `float.round(n: Float) -> Float`

Round a float to the nearest integer value (as Float), halves away from zero
(`round(2.5) = 3.0`, `round(-2.5) = -3.0`), exactly on the true value: `round(0.49999999999999994)`
is `0.0` and an odd integer above 2^52 is unchanged. `-0.0` and values in `(-0.5, 0)` round to `-0.0`.

```almd run
fn main() -> Unit = {
  println(float.to_string(float.round(3.6)))
}
```
```output
4.0
```

### `float.floor(x: Float) -> Float`

Round a float down to the nearest integer value (as Float).

```almd run
fn main() -> Unit = {
  println(float.to_string(float.floor(3.9)))
}
```
```output
3.0
```

### `float.ceil(x: Float) -> Float`

Round a float up to the nearest integer value (as Float).

```almd run
fn main() -> Unit = {
  println(float.to_string(float.ceil(3.1)))
}
```
```output
4.0
```

### `float.abs(x: Float) -> Float`

Return the absolute value of a float.

```almd run
fn main() -> Unit = {
  println(float.to_string(float.abs(-2.5)))
}
```
```output
2.5
```

### `float.sqrt(n: Float) -> Float`

Return the square root of a float.

```almd run
fn main() -> Unit = {
  println(float.to_string(float.sqrt(9.0)))
}
```
```output
3.0
```

### `float.parse(s: String) -> Result[Float, String]`

Parse a string into a finite float. Returns err if the string is not a valid number.

Only ASCII whitespace (space, tab, LF, CR, form feed) around the number is ignored; any other whitespace, such as a no-break space, is an error. The result is always finite: `"inf"`, `"Infinity"` and `"NaN"` in any case are `err("invalid float literal")`, and a number too large for a Float, such as `"1e400"`, is `err("float literal out of range")`. A number too small rounds to `0.0`. Many languages accept these spellings; Almide refuses them because a NaN makes every comparison false, so a check like `if amount > limit then reject` would let it through. Write `1.0 / 0.0` if you need an infinity.

```almd run
fn show(r: Result[Float, String]) -> String = match r {
  ok(x) => "ok(${float.to_string(x)})",
  err(e) => "err(${e})",
}

fn main() -> Unit = {
  println(show(float.parse("3.14")))
  println(show(float.parse("abc")))
}
```
```output
ok(3.14)
err(invalid float literal)
```

### `float.from_int(n: Int) -> Float`

Convert an integer to a float.

```almd run
fn main() -> Unit = {
  println(float.to_string(float.from_int(42)))
}
```
```output
42.0
```

### `float.min(a: Float, b: Float) -> Float`

Return the smaller of two floats.

```almd run
fn main() -> Unit = {
  println(float.to_string(float.min(1.5, 2.5)))
}
```
```output
1.5
```

### `float.max(a: Float, b: Float) -> Float`

Return the larger of two floats.

```almd run
fn main() -> Unit = {
  println(float.to_string(float.max(1.5, 2.5)))
}
```
```output
2.5
```

### `float.to_fixed(x: Float, decimals: Int) -> String`

Format a float with a fixed number of decimal places.

```almd run
fn main() -> Unit = {
  println(float.to_fixed(3.14159, 2))
}
```
```output
3.14
```

### `float.clamp(x: Float, lo: Float, hi: Float) -> Float`

Clamp a float to the range [lo, hi].

```almd run
fn main() -> Unit = {
  println(float.to_string(float.clamp(15.0, 0.0, 10.0)))
}
```
```output
10.0
```

### `float.sign(n: Float) -> Float`

Return the sign of a float as 1.0 or -1.0, decided by the sign BIT (f64
`signum`): `sign(0.0)` is 1.0 and `sign(-0.0)` is -1.0. A NaN input returns
NaN. It never returns 0.0.

```almd run
fn main() -> Unit = {
  println(float.to_string(float.sign(-3.5)))
}
```
```output
-1.0
```

### `float.is_nan(x: Float) -> Bool`

Check if a float is NaN (not a number).

```almd run
fn main() -> Unit = {
  println("${float.is_nan(0.0 / 0.0)}")
}
```
```output
true
```

### `float.is_infinite(x: Float) -> Bool`

Check if a float is positive or negative infinity.

```almd run
fn main() -> Unit = {
  println("${float.is_infinite(1.0 / 0.0)}")
}
```
```output
true
```

### `float.to_bits(f: Float) -> Int`

Reinterpret a float as its IEEE 754 bit representation (i64).

```almd run
fn main() -> Unit = {
  println("${float.to_bits(1.0)}")
}
```
```output
4607182418800017408
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (61 functions)

```
// Shortest round-trip digits, no exponent; 2.0 keeps .0.
// @since 0.5.0 or earlier
float.to_string(n: Float) -> String

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.5.0 or earlier
float.to_int(n: Float) -> Int

// Nearest Float; may round when |n| > 2^53.
// @since 0.5.0 or earlier
float.from_int(n: Int) -> Float

// Finite Float from s trimmed of ASCII space; err on junk, inf, NaN, overflow.
// @since 0.5.0 or earlier
float.parse(s: String) -> Result[Float, String]

// Fixed-point, ties to even; decimals outside 0..4096 abort.
// @since 0.5.0 or earlier
float.to_fixed(n: Float, decimals: Int) -> String

// IEEE-754 bits as Int; any NaN gives the canonical NaN.
// @since 0.12.1 or earlier
float.to_bits(f: Float) -> Int

// Square root; NaN below 0, -0.0 stays -0.0.
// @since 0.5.0 or earlier
float.sqrt(n: Float) -> Float

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since 0.5.0 or earlier
float.abs(n: Float) -> Float

// Round toward -inf; -0.5 gives -1.0.
// @since 0.5.0 or earlier
float.floor(n: Float) -> Float

// Round toward +inf; -0.5 gives -0.0.
// @since 0.5.0 or earlier
float.ceil(n: Float) -> Float

// Nearest whole value; halves round away from zero.
// @since 0.5.0 or earlier
float.round(n: Float) -> Float

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since 0.5.0 or earlier
float.min(a: Float, b: Float) -> Float

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since 0.5.0 or earlier
float.max(a: Float, b: Float) -> Float

// n limited to lo..hi; lo > hi or a NaN bound aborts.
// @since 0.5.0 or earlier
float.clamp(n: Float, lo: Float, hi: Float) -> Float

// 1.0 or -1.0 by sign bit (0.0 gives 1.0); NaN for NaN.
// @since 0.5.13 or earlier
float.sign(n: Float) -> Float   (deprecated — use float.signum)

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float.signum(x: Float) -> Float

// True only for NaN (unequal to itself).
// @since 0.6.0 or earlier
float.is_nan(n: Float) -> Bool

// True for inf and -inf; false for NaN.
// @since 0.6.0 or earlier
float.is_infinite(n: Float) -> Bool

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int8(n: Float) -> Int8

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int16(n: Float) -> Int16

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int32(n: Float) -> Int32

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint8(n: Float) -> UInt8

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint16(n: Float) -> UInt16

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint32(n: Float) -> UInt32

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint64(n: Float) -> UInt64

// Nearest Float32; overflow gives inf.
// @since 0.15.0 or earlier
float.to_float32(n: Float) -> Float32

// Truncates toward 0, saturating; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int64(n: Float) -> Int64

// Same f64 value; always exact.
// @since 0.15.0 or earlier
float.to_float64(n: Float) -> Float64

// Exact widening; never rounds.
// @since 0.15.0 or earlier
float.from_float32(n: Float32) -> Float

// Same f64 value; always exact.
// @since 0.15.0 or earlier
float.from_float64(n: Float64) -> Float

// some iff n is an integer in -2^63..2^63-1; NaN and ±inf give none.
// @since unreleased
float.to_int_checked(n: Float) -> Option[Int]

// some iff n is an integer in -128..127.
// @since 0.15.0 or earlier
float.to_int8_checked(n: Float) -> Option[Int8]

// some iff n is an integer in -32768..32767.
// @since 0.15.0 or earlier
float.to_int16_checked(n: Float) -> Option[Int16]

// some iff n is an integer in -2^31..2^31-1.
// @since 0.15.0 or earlier
float.to_int32_checked(n: Float) -> Option[Int32]

// some iff n is an integer in -2^63..2^63-1.
// @since 0.15.0 or earlier
float.to_int64_checked(n: Float) -> Option[Int64]

// some iff n is an integer in 0..255.
// @since 0.15.0 or earlier
float.to_uint8_checked(n: Float) -> Option[UInt8]

// some iff n is an integer in 0..65535.
// @since 0.15.0 or earlier
float.to_uint16_checked(n: Float) -> Option[UInt16]

// some iff n is an integer in 0..2^32-1.
// @since 0.15.0 or earlier
float.to_uint32_checked(n: Float) -> Option[UInt32]

// some iff n is an integer in 0..2^64-1.
// @since 0.15.0 or earlier
float.to_uint64_checked(n: Float) -> Option[UInt64]

// some if n is exact in Float32; NaN and ±inf give none.
// @since 0.15.0 or earlier
float.to_float32_checked(n: Float) -> Option[Float32]

// Truncates, clamps to Int range; NaN gives 0.
// @since unreleased
float.to_int_saturating(n: Float) -> Int

// Truncates, clamps to -128..127; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int8_saturating(n: Float) -> Int8

// Truncates, clamps to -32768..32767; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int16_saturating(n: Float) -> Int16

// Truncates, clamps to Int32 range; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int32_saturating(n: Float) -> Int32

// Truncates, clamps to Int64 range; NaN gives 0.
// @since 0.15.0 or earlier
float.to_int64_saturating(n: Float) -> Int64

// Truncates, clamps to 0..255; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint8_saturating(n: Float) -> UInt8

// Truncates, clamps to 0..65535; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint16_saturating(n: Float) -> UInt16

// Truncates, clamps to 0..2^32-1; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint32_saturating(n: Float) -> UInt32

// Truncates, clamps to 0..2^64-1; NaN gives 0.
// @since 0.15.0 or earlier
float.to_uint64_saturating(n: Float) -> UInt64

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float.trunc(x: Float) -> Float

// True unless x is inf, -inf or NaN.
// @since unreleased
float.is_finite(x: Float) -> Bool

// base to the exp; NaN for a negative base with a fractional exp.
// @since unreleased
float.pow(base: Float, exp: Float) -> Float

// -1, 0 or 1 in IEEE-754 total order: -0.0 < 0.0, NaN above inf and equal to itself.
// @since unreleased
float.compare(a: Float, b: Float) -> Int

// Float whose IEEE-754 binary64 bits are bits; any NaN gives the one NaN.
// @since unreleased
float.from_bits(bits: Int) -> Float

// Positive infinity.
// @since unreleased
float.infinity() -> Float

// The one quiet NaN (bits 0x7ff8000000000000).
// @since unreleased
float.nan() -> Float

// Gap between 1.0 and the next Float: 2^-52.
// @since unreleased
float.epsilon() -> Float

// Largest finite Float, about 1.7976931348623157e308.
// @since unreleased
float.max_value() -> Float

// Most negative finite Float (= -max_value()), not the smallest positive.
// @since unreleased
float.min_value() -> Float

// x as the nearest Float (ties to even); NaN stays NaN.
// @since unreleased
float.from(x: S) -> Float

// some(x as Float) when Float holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float.from_checked(x: S) -> Option[Float]
```

<!-- END GENERATED SIGNATURE INDEX -->
