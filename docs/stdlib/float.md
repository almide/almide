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
