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

Round a float to the nearest integer value (as Float).

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

Return the sign of a float: -1.0, 0.0, or 1.0.

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
float.to_string(n: Float) -> String
float.to_int(n: Float) -> Int
float.from_int(n: Int) -> Float
float.parse(s: String) -> Result[Float, String]
float.to_fixed(n: Float, decimals: Int) -> String
float.to_bits(f: Float) -> Int
float.sqrt(n: Float) -> Float
float.abs(n: Float) -> Float
float.floor(n: Float) -> Float
float.ceil(n: Float) -> Float
float.round(n: Float) -> Float
float.min(a: Float, b: Float) -> Float
float.max(a: Float, b: Float) -> Float
float.clamp(n: Float, lo: Float, hi: Float) -> Float
float.sign(n: Float) -> Float
float.is_nan(n: Float) -> Bool
float.is_infinite(n: Float) -> Bool
float.to_int8(n: Float) -> Int8
float.to_int16(n: Float) -> Int16
float.to_int32(n: Float) -> Int32
float.to_uint8(n: Float) -> UInt8
float.to_uint16(n: Float) -> UInt16
float.to_uint32(n: Float) -> UInt32
float.to_uint64(n: Float) -> UInt64
float.to_float32(n: Float) -> Float32
float.to_int64(n: Float) -> Int64
float.to_float64(n: Float) -> Float64
float.from_float32(n: Float32) -> Float
float.from_float64(n: Float64) -> Float
float.to_int8_checked(n: Float) -> Option[Int8]
float.to_int16_checked(n: Float) -> Option[Int16]
float.to_int32_checked(n: Float) -> Option[Int32]
float.to_int64_checked(n: Float) -> Option[Int64]
float.to_uint8_checked(n: Float) -> Option[UInt8]
float.to_uint16_checked(n: Float) -> Option[UInt16]
float.to_uint32_checked(n: Float) -> Option[UInt32]
float.to_uint64_checked(n: Float) -> Option[UInt64]
float.to_float32_checked(n: Float) -> Option[Float32]
float.to_int8_saturating(n: Float) -> Int8
float.to_int16_saturating(n: Float) -> Int16
float.to_int32_saturating(n: Float) -> Int32
float.to_int64_saturating(n: Float) -> Int64
float.to_uint8_saturating(n: Float) -> UInt8
float.to_uint16_saturating(n: Float) -> UInt16
float.to_uint32_saturating(n: Float) -> UInt32
float.to_uint64_saturating(n: Float) -> UInt64
```

<!-- END GENERATED SIGNATURE INDEX -->
