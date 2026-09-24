# float

Floating-point operations. auto-imported.

### `float.to_string(n: Float) -> String`

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

### `float.floor(n: Float) -> Float`

Round a float down to the nearest integer value (as Float).

```almd run
fn main() -> Unit = {
  println(float.to_string(float.floor(3.9)))
}
```
```output
3.0
```

### `float.ceil(n: Float) -> Float`

Round a float up to the nearest integer value (as Float).

```almd run
fn main() -> Unit = {
  println(float.to_string(float.ceil(3.1)))
}
```
```output
4.0
```

### `float.abs(n: Float) -> Float`

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

Parse a string into a float. Returns err if the string is not a valid number.

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

### `float.to_fixed(n: Float, decimals: Int) -> String`

Format a float with a fixed number of decimal places.

```almd run
fn main() -> Unit = {
  println(float.to_fixed(3.14159, 2))
}
```
```output
3.14
```

### `float.clamp(n: Float, lo: Float, hi: Float) -> Float`

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

### `float.is_nan(n: Float) -> Bool`

Check if a float is NaN (not a number).

```almd run
fn main() -> Unit = {
  println("${float.is_nan(0.0 / 0.0)}")
}
```
```output
true
```

### `float.is_infinite(n: Float) -> Bool`

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

## Signature index (46 functions)

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

// Float from trimmed s; takes 1e3, inf, NaN; err on junk.
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
float.sign(n: Float) -> Float

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
```

<!-- END GENERATED SIGNATURE INDEX -->
