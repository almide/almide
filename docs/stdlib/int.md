# int

Integer arithmetic and bitwise. auto-imported.

### `int.to_string(n: Int) -> String`

Convert an integer to its decimal string representation.

```almd run
fn main() -> Unit = {
  println(int.to_string(42))
}
```
```output
42
```

### `int.to_hex(n: Int) -> String`

Convert an integer to its hexadecimal string representation (lowercase).

```almd run
fn main() -> Unit = {
  println(int.to_hex(255))
}
```
```output
ff
```

### `int.parse(s: String) -> Result[Int, String]`

Parse a decimal string into an integer. Returns err if the string is not a valid integer.

```almd run
fn main() -> Unit = {
  match int.parse("42") {
    ok(n) => println("ok(${n})"),
    err(e) => println("err(${e})"),
  }
  match int.parse("4x2") {
    ok(n) => println("ok(${n})"),
    err(e) => println("err(${e})"),
  }
}
```
```output
ok(42)
err(invalid digit found in string)
```

### `int.from_hex(s: String) -> Result[Int, String]`

Parse a hexadecimal string into an integer. Returns err if the string is not valid hex.

```almd
int.parse_hex(\"ff\") // => ok(255)
```

### `int.abs(n: Int) -> Int`

Return the absolute value of an integer.

```almd run
fn main() -> Unit = {
  println("${int.abs(-5)}")
}
```
```output
5
```

### `int.min(a: Int, b: Int) -> Int`

Return the smaller of two integers.

```almd run
fn main() -> Unit = {
  println("${int.min(3, 7)}")
}
```
```output
3
```

### `int.max(a: Int, b: Int) -> Int`

Return the larger of two integers.

```almd run
fn main() -> Unit = {
  println("${int.max(3, 7)}")
}
```
```output
7
```

### `int.band(a: Int, b: Int) -> Int`

Bitwise AND of two integers.

```almd run
fn main() -> Unit = {
  println("${int.band(0b1100, 0b1010)}") // 0b1000
}
```
```output
8
```

### `int.bor(a: Int, b: Int) -> Int`

Bitwise OR of two integers.

```almd run
fn main() -> Unit = {
  println("${int.bor(0b1100, 0b1010)}") // 0b1110
}
```
```output
14
```

### `int.bxor(a: Int, b: Int) -> Int`

Bitwise XOR of two integers.

```almd run
fn main() -> Unit = {
  println("${int.bxor(0b1100, 0b1010)}") // 0b0110
}
```
```output
6
```

### `int.bshl(a: Int, n: Int) -> Int`

Bitwise shift left.

```almd run
fn main() -> Unit = {
  println("${int.bshl(1, 3)}")
}
```
```output
8
```

### `int.bshr(a: Int, n: Int) -> Int`

Bitwise shift right (arithmetic).

```almd run
fn main() -> Unit = {
  println("${int.bshr(8, 2)}")
  println("${int.bshr(-8, 2)}")
}
```
```output
2
-2
```

### `int.bnot(a: Int) -> Int`

Bitwise NOT (complement) of an integer.

```almd run
fn main() -> Unit = {
  println("${int.bnot(0)}")
}
```
```output
-1
```

### `int.wrap_add(a: Int, b: Int, bits: Int) -> Int`

Wrapping addition within a given bit width. Overflow wraps around.

```almd run
fn main() -> Unit = {
  println("${int.wrap_add(255, 1, 8)}")
}
```
```output
0
```

### `int.wrap_mul(a: Int, b: Int, bits: Int) -> Int`

Wrapping multiplication within a given bit width. Overflow wraps around.

```almd run
fn main() -> Unit = {
  println("${int.wrap_mul(16, 16, 8)}")
}
```
```output
0
```

### `int.rotate_right(a: Int, n: Int, bits: Int) -> Int`

Rotate bits right within a given bit width.

```almd run
fn main() -> Unit = {
  println("${int.rotate_right(1, 1, 8)}")
}
```
```output
128
```

### `int.rotate_left(a: Int, n: Int, bits: Int) -> Int`

Rotate bits left within a given bit width.

```almd run
fn main() -> Unit = {
  println("${int.rotate_left(128, 1, 8)}")
}
```
```output
1
```

### `int.to_u32(a: Int) -> Int`

Truncate an integer to an unsigned 32-bit value (mask to 0...4294967295).

```almd run
fn main() -> Unit = {
  println("${int.to_u32(300)}")
  println("${int.to_u32(-1)}")
}
```
```output
300
4294967295
```

### `int.to_u8(a: Int) -> Int`

Truncate an integer to an unsigned 8-bit value (mask to 0...255).

```almd run
fn main() -> Unit = {
  println("${int.to_u8(300)}")
}
```
```output
44
```

### `int.clamp(n: Int, lo: Int, hi: Int) -> Int`

Clamp an integer to the range [lo, hi].

```almd run
fn main() -> Unit = {
  println("${int.clamp(15, 0, 10)}")
  println("${int.clamp(-3, 0, 10)}")
}
```
```output
10
0
```

### `int.to_float(n: Int) -> Float`

Convert an integer to a floating-point number.

```almd run
fn main() -> Unit = {
  println(float.to_string(int.to_float(42)))
}
```
```output
42.0
```

### `int.bits_to_float(bits: Int) -> Float`

Reinterpret an integer's bits as an IEEE 754 float (f64).

```almd run
fn main() -> Unit = {
  println(float.to_string(int.bits_to_float(4607182418800017408)))
}
```
```output
1.0
```

## Matrix completeness (#956)

The `_checked`/`_saturating` families here are the `Int`-source row of the
integer conversion matrix; every sized module carries the same trio for its own
lossy pairs, `UInt64 → Int` (the one lossy widening) is `from_uint64_checked` /
`from_uint64_saturating`, and every carrier has `min_value()`/`max_value()`.
The surface is derived from the range table and machine-enforced by the
numeric-matrix gate in `almide docs-gen --check`.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (70 functions)

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
int.to_int8(n: Int) -> Int8
int.to_int16(n: Int) -> Int16
int.to_int32(n: Int) -> Int32
int.to_uint8(n: Int) -> UInt8
int.to_uint16(n: Int) -> UInt16
int.to_uint32(n: Int) -> UInt32
int.to_uint64(n: Int) -> UInt64
int.to_float32(n: Int) -> Float32
int.to_float64(n: Int) -> Float64
int.to_int64(n: Int) -> Int64
int.from_int8(n: Int8) -> Int
int.from_int16(n: Int16) -> Int
int.from_int32(n: Int32) -> Int
int.from_int64(n: Int64) -> Int
int.from_uint8(n: UInt8) -> Int
int.from_uint16(n: UInt16) -> Int
int.from_uint32(n: UInt32) -> Int
int.from_uint64(n: UInt64) -> Int
int.to_int8_checked(n: Int) -> Option[Int8]
int.to_int16_checked(n: Int) -> Option[Int16]
int.to_int32_checked(n: Int) -> Option[Int32]
int.to_uint8_checked(n: Int) -> Option[UInt8]
int.to_uint16_checked(n: Int) -> Option[UInt16]
int.to_uint32_checked(n: Int) -> Option[UInt32]
int.to_uint64_checked(n: Int) -> Option[UInt64]
int.to_float32_checked(n: Int) -> Option[Float32]
int.to_int8_saturating(n: Int) -> Int8
int.to_int16_saturating(n: Int) -> Int16
int.to_int32_saturating(n: Int) -> Int32
int.to_uint8_saturating(n: Int) -> UInt8
int.to_uint16_saturating(n: Int) -> UInt16
int.to_uint32_saturating(n: Int) -> UInt32
int.to_uint64_saturating(n: Int) -> UInt64
int.from_uint64_checked(n: UInt64) -> Option[Int]
int.from_uint64_saturating(n: UInt64) -> Int
int.min_value() -> Int
int.max_value() -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
