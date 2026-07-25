# int

Integer arithmetic and bitwise. auto-imported.

### `int.to_string(n: Int) -> String`

Convert an integer to its decimal string representation.

```almd
int.to_string(42) // => \"42\
```

### `int.to_hex(n: Int) -> String`

Convert an integer to its hexadecimal string representation (lowercase).

```almd
int.to_hex(255) // => \"ff\
```

### `int.parse(s: String) -> Result[Int, String]`

Parse a decimal string into an integer. Returns err if the string is not a valid integer.

```almd
int.parse(\"42\") // => ok(42)
```

### `int.from_hex(s: String) -> Result[Int, String]`

Parse a hexadecimal string into an integer. Returns err if the string is not valid hex.

```almd
int.parse_hex(\"ff\") // => ok(255)
```

### `int.abs(n: Int) -> Int`

Return the absolute value of an integer.

```almd
int.abs(-5) // => 5
```

### `int.min(a: Int, b: Int) -> Int`

Return the smaller of two integers.

```almd
int.min(3, 7) // => 3
```

### `int.max(a: Int, b: Int) -> Int`

Return the larger of two integers.

```almd
int.max(3, 7) // => 7
```

### `int.band(a: Int, b: Int) -> Int`

Bitwise AND of two integers.

```almd
int.band(0b1100, 0b1010) // => 0b1000
```

### `int.bor(a: Int, b: Int) -> Int`

Bitwise OR of two integers.

```almd
int.bor(0b1100, 0b1010) // => 0b1110
```

### `int.bxor(a: Int, b: Int) -> Int`

Bitwise XOR of two integers.

```almd
int.bxor(0b1100, 0b1010) // => 0b0110
```

### `int.bshl(a: Int, n: Int) -> Int`

Bitwise shift left.

```almd
int.bshl(1, 3) // => 8
```

### `int.bshr(a: Int, n: Int) -> Int`

Bitwise shift right (arithmetic).

```almd
int.bshr(8, 2) // => 2
```

### `int.bnot(a: Int) -> Int`

Bitwise NOT (complement) of an integer.

```almd
int.bnot(0) // => -1
```

### `int.wrap_add(a: Int, b: Int, bits: Int) -> Int`

Wrapping addition within a given bit width. Overflow wraps around.

```almd
int.wrap_add(255, 1, 8) // => 0
```

### `int.wrap_mul(a: Int, b: Int, bits: Int) -> Int`

Wrapping multiplication within a given bit width. Overflow wraps around.

```almd
int.wrap_mul(16, 16, 8) // => 0
```

### `int.rotate_right(a: Int, n: Int, bits: Int) -> Int`

Rotate bits right within a given bit width.

```almd
int.rotate_right(1, 1, 8) // => 128
```

### `int.rotate_left(a: Int, n: Int, bits: Int) -> Int`

Rotate bits left within a given bit width.

```almd
int.rotate_left(128, 1, 8) // => 1
```

### `int.to_u32(a: Int) -> Int`

Truncate an integer to an unsigned 32-bit value (mask to 0..4294967295).

```almd
int.to_u32(300) // => 300
```

### `int.to_u8(a: Int) -> Int`

Truncate an integer to an unsigned 8-bit value (mask to 0..255).

```almd
int.to_u8(300) // => 44
```

### `int.clamp(n: Int, lo: Int, hi: Int) -> Int`

Clamp an integer to the range [lo, hi].

```almd
int.clamp(15, 0, 10) // => 10
```

### `int.to_float(n: Int) -> Float`

Convert an integer to a floating-point number.

```almd
int.to_float(42) // => 42.0
```

### `int.bits_to_float(bits: Int) -> Float`

Reinterpret an integer's bits as an IEEE 754 float (f64).

```almd
int.bits_to_float(4607182418800017408) // => 1.0
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (66 functions)

```
int.to_string(n: Int) -> String
int.to_hex(n: Int) -> String
int.parse(s: String) -> Result[Int, String]
int.from_hex(s: String) -> Result[Int, String]
int.to_float(n: Int) -> Float
int.bits_to_float(bits: Int) -> Float
int.bits_to_f32(bits: Int) -> Float
int.abs(n: Int) -> Int
int.min(a: Int, b: Int) -> Int
int.max(a: Int, b: Int) -> Int
int.clamp(n: Int, lo: Int, hi: Int) -> Int
int.band(a: Int, b: Int) -> Int
int.bor(a: Int, b: Int) -> Int
int.bxor(a: Int, b: Int) -> Int
int.bnot(a: Int) -> Int
int.bshl(a: Int, n: Int) -> Int
int.bshr(a: Int, n: Int) -> Int
int.count_leading_zeros(n: Int) -> Int
int.count_trailing_zeros(n: Int) -> Int
int.pop_count(n: Int) -> Int
int.bit_reverse(n: Int) -> Int
int.byte_swap(n: Int) -> Int
int.bit_width(n: Int) -> Int
int.log2_floor(n: Int) -> Int
int.log2_ceil(n: Int) -> Int
int.next_power_of_two(n: Int) -> Int
int.prev_power_of_two(n: Int) -> Int
int.wrap_add(a: Int, b: Int, bits: Int) -> Int
int.wrap_mul(a: Int, b: Int, bits: Int) -> Int
int.rotate_right(a: Int, n: Int, bits: Int) -> Int
int.rotate_left(a: Int, n: Int, bits: Int) -> Int
int.to_u32(a: Int) -> Int
int.to_u8(a: Int) -> Int
int.to_int8(n: Int) -> ?
int.to_int16(n: Int) -> ?
int.to_int32(n: Int) -> ?
int.to_uint8(n: Int) -> ?
int.to_uint16(n: Int) -> ?
int.to_uint32(n: Int) -> ?
int.to_uint64(n: Int) -> ?
int.to_float32(n: Int) -> ?
int.to_float64(n: Int) -> ?
int.to_int64(n: Int) -> ?
int.from_int8(n: ?) -> Int
int.from_int16(n: ?) -> Int
int.from_int32(n: ?) -> Int
int.from_int64(n: ?) -> Int
int.from_uint8(n: ?) -> Int
int.from_uint16(n: ?) -> Int
int.from_uint32(n: ?) -> Int
int.from_uint64(n: ?) -> Int
int.to_int8_checked(n: Int) -> Option[?]
int.to_int16_checked(n: Int) -> Option[?]
int.to_int32_checked(n: Int) -> Option[?]
int.to_uint8_checked(n: Int) -> Option[?]
int.to_uint16_checked(n: Int) -> Option[?]
int.to_uint32_checked(n: Int) -> Option[?]
int.to_uint64_checked(n: Int) -> Option[?]
int.to_float32_checked(n: Int) -> Option[?]
int.to_int8_saturating(n: Int) -> ?
int.to_int16_saturating(n: Int) -> ?
int.to_int32_saturating(n: Int) -> ?
int.to_uint8_saturating(n: Int) -> ?
int.to_uint16_saturating(n: Int) -> ?
int.to_uint32_saturating(n: Int) -> ?
int.to_uint64_saturating(n: Int) -> ?
```

<!-- END GENERATED SIGNATURE INDEX -->
