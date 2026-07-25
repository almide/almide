# float

Floating-point operations. auto-imported.

### `float.to_string(n: Float) -> String`

Convert a float to its string representation.

```almd
float.to_string(3.14) // => \"3.14\
```

### `float.to_int(n: Float) -> Int`

Truncate a float to an integer (rounds toward zero).

```almd
float.to_int(3.9) // => 3
```

### `float.round(n: Float) -> Float`

Round a float to the nearest integer value (as Float).

```almd
float.round(3.6) // => 4.0
```

### `float.floor(n: Float) -> Float`

Round a float down to the nearest integer value (as Float).

```almd
float.floor(3.9) // => 3.0
```

### `float.ceil(n: Float) -> Float`

Round a float up to the nearest integer value (as Float).

```almd
float.ceil(3.1) // => 4.0
```

### `float.abs(n: Float) -> Float`

Return the absolute value of a float.

```almd
float.abs(-2.5) // => 2.5
```

### `float.sqrt(n: Float) -> Float`

Return the square root of a float.

```almd
float.sqrt(9.0) // => 3.0
```

### `float.parse(s: String) -> Result[Float, String]`

Parse a string into a float. Returns err if the string is not a valid number.

```almd
float.parse(\"3.14\") // => ok(3.14)
```

### `float.from_int(n: Int) -> Float`

Convert an integer to a float.

```almd
float.from_int(42) // => 42.0
```

### `float.min(a: Float, b: Float) -> Float`

Return the smaller of two floats.

```almd
float.min(1.5, 2.5) // => 1.5
```

### `float.max(a: Float, b: Float) -> Float`

Return the larger of two floats.

```almd
float.max(1.5, 2.5) // => 2.5
```

### `float.to_fixed(n: Float, decimals: Int) -> String`

Format a float with a fixed number of decimal places.

```almd
float.to_fixed(3.14159, 2) // => \"3.14\
```

### `float.clamp(n: Float, lo: Float, hi: Float) -> Float`

Clamp a float to the range [lo, hi].

```almd
float.clamp(15.0, 0.0, 10.0) // => 10.0
```

### `float.sign(n: Float) -> Float`

Return the sign of a float: -1.0, 0.0, or 1.0.

```almd
float.sign(-3.5) // => -1.0
```

### `float.is_nan(n: Float) -> Bool`

Check if a float is NaN (not a number).

```almd
float.is_nan(0.0 / 0.0) // => true
```

### `float.is_infinite(n: Float) -> Bool`

Check if a float is positive or negative infinity.

```almd
float.is_infinite(1.0 / 0.0) // => true
```

### `float.to_bits(f: Float) -> Int`

Reinterpret a float as its IEEE 754 bit representation (i64).

```almd
float.to_bits(1.0) // => 4607182418800017408
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
