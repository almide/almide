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

```almd run
fn show(r: Result[Int, String]) -> String = match r { ok(n) => "ok " + int.to_string(n), err(e) => "err " + e }

fn main() -> Unit = {
  println(show(int.from_hex("ff")))
  println(show(int.from_hex("FF")))
  println(show(int.from_hex("zz")))
}
```
```output
ok 255
ok 255
err invalid digit found in string
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

### `int.is_even(n: Int) -> Bool`

Parity predicate: `true` when `n` is divisible by 2. Negative values and the
bounds are exact (`%` keeps the dividend's sign, so the test is `% 2 == 0`).

```almd run
fn main() -> Unit = {
  println("${int.is_even(4)} ${int.is_even(-3)} ${int.is_even(0)}")
}
```
```output
true false true
```

### `int.is_odd(n: Int) -> Bool`

The other half of the parity pair: `true` when `n` is not divisible by 2.
Scalar `int` only — a sized value reaches it through `int.from_int8(x)`.

```almd run
fn main() -> Unit = {
  println("${int.is_odd(7)} ${int.is_odd(-3)} ${int.is_odd(4)}")
}
```
```output
true true false
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

## Signature index (72 functions)

```
// Decimal digits, with - when negative.
int.to_string(n: Int) -> String

// Lowercase hex, no 0x; -1 gives 16 f digits.
int.to_hex(n: Int) -> String

// Decimal Int from trimmed s; err on junk or overflow.
int.parse(s: String) -> Result[Int, String]

// Hex, any case, 0x optional; err on junk or > 2^63-1.
int.from_hex(s: String) -> Result[Int, String]

// Nearest Float; may round when |n| > 2^53.
int.to_float(n: Int) -> Float

// Float whose IEEE-754 binary64 bits are bits.
int.bits_to_float(bits: Int) -> Float

// Low 32 bits read as binary32, widened to Float.
int.bits_to_f32(bits: Int) -> Float

// Magnitude of n; min_value wraps to itself.
int.abs(n: Int) -> Int

// Lesser of a and b.
int.min(a: Int, b: Int) -> Int

// Greater of a and b.
int.max(a: Int, b: Int) -> Int

// n limited to lo..hi; aborts when lo > hi.
int.clamp(n: Int, lo: Int, hi: Int) -> Int

// True when n % 2 == 0; exact for negatives.
int.is_even(n: Int) -> Bool

// True when n % 2 != 0; -3 counts as odd.
int.is_odd(n: Int) -> Bool

// Bitwise AND over all 64 bits.
int.band(a: Int, b: Int) -> Int

// Bitwise OR over all 64 bits.
int.bor(a: Int, b: Int) -> Int

// Bitwise XOR over all 64 bits.
int.bxor(a: Int, b: Int) -> Int

// Bitwise complement; equals -a - 1.
int.bnot(a: Int) -> Int

// a shifted left by n bits; n is taken mod 64.
int.bshl(a: Int, n: Int) -> Int

// Arithmetic (sign-filling) right shift; n mod 64.
int.bshr(a: Int, n: Int) -> Int

// Leading zero bits of the 64-bit word; 64 for 0.
int.count_leading_zeros(n: Int) -> Int

// Trailing zero bits; 64 for 0.
int.count_trailing_zeros(n: Int) -> Int

// Number of set bits; 64 for -1.
int.pop_count(n: Int) -> Int

// The 64 bits in reverse order; 1 gives min_value.
int.bit_reverse(n: Int) -> Int

// The 8 bytes in reverse order (endian flip).
int.byte_swap(n: Int) -> Int

// Bits needed to hold n; 0 for 0, 64 if negative.
int.bit_width(n: Int) -> Int

// Floor of log2(n); -1 when n <= 0.
int.log2_floor(n: Int) -> Int

// Ceiling of log2(n); 0 when n <= 1.
int.log2_ceil(n: Int) -> Int

// Least power of 2 >= n; 1 if n <= 1; min_value past 2^62.
int.next_power_of_two(n: Int) -> Int

// Greatest power of 2 <= n; 0 when n <= 0.
int.prev_power_of_two(n: Int) -> Int

// (a + b) mod 2^bits, non-negative when bits < 64.
int.wrap_add(a: Int, b: Int, bits: Int) -> Int

// (a * b) mod 2^bits, non-negative when bits < 64.
int.wrap_mul(a: Int, b: Int, bits: Int) -> Int

// Low bits of a rotated right by n; bits <= 0 aborts.
int.rotate_right(a: Int, n: Int, bits: Int) -> Int

// Low bits of a rotated left by n; bits <= 0 aborts.
int.rotate_left(a: Int, n: Int, bits: Int) -> Int

// Low 32 bits as unsigned; -1 becomes 4294967295.
int.to_u32(a: Int) -> Int

// Low 8 bits as unsigned; 300 becomes 44.
int.to_u8(a: Int) -> Int

// Wraps to 8 bits; 200 becomes -56.
int.to_int8(n: Int) -> Int8

// Wraps to 16 bits; 40000 becomes -25536.
int.to_int16(n: Int) -> Int16

// Wraps to 32 bits; 2^31 becomes -2^31.
int.to_int32(n: Int) -> Int32

// Wraps to 8 bits; -1 becomes 255.
int.to_uint8(n: Int) -> UInt8

// Wraps to 16 bits; -1 becomes 65535.
int.to_uint16(n: Int) -> UInt16

// Wraps to 32 bits; -1 becomes 2^32-1.
int.to_uint32(n: Int) -> UInt32

// Wraps to 64 bits; -1 becomes 2^64-1.
int.to_uint64(n: Int) -> UInt64

// Float32 value; may round when |n| > 2^24.
int.to_float32(n: Int) -> Float32

// Float64 value; may round when |n| > 2^53.
int.to_float64(n: Int) -> Float64

// Same value as Int64; always exact.
int.to_int64(n: Int) -> Int64

// Sign-extending widen; always exact.
int.from_int8(n: Int8) -> Int

// Sign-extending widen; always exact.
int.from_int16(n: Int16) -> Int

// Sign-extending widen; always exact.
int.from_int32(n: Int32) -> Int

// Same value as Int; always exact.
int.from_int64(n: Int64) -> Int

// Zero-extending widen; always exact.
int.from_uint8(n: UInt8) -> Int

// Zero-extending widen; always exact.
int.from_uint16(n: UInt16) -> Int

// Zero-extending widen; always exact.
int.from_uint32(n: UInt32) -> Int

// Reinterprets bits; values >= 2^63 go negative.
int.from_uint64(n: UInt64) -> Int

// some(n), or none outside -128..127.
int.to_int8_checked(n: Int) -> Option[Int8]

// some(n), or none outside -32768..32767.
int.to_int16_checked(n: Int) -> Option[Int16]

// some(n), or none outside -2^31..2^31-1.
int.to_int32_checked(n: Int) -> Option[Int32]

// some(n), or none outside 0..255.
int.to_uint8_checked(n: Int) -> Option[UInt8]

// some(n), or none outside 0..65535.
int.to_uint16_checked(n: Int) -> Option[UInt16]

// some(n), or none outside 0..2^32-1.
int.to_uint32_checked(n: Int) -> Option[UInt32]

// some(n), or none if n is negative.
int.to_uint64_checked(n: Int) -> Option[UInt64]

// some(f), or none if n fails the f32 round trip.
int.to_float32_checked(n: Int) -> Option[Float32]

// Clamps n to -128..127.
int.to_int8_saturating(n: Int) -> Int8

// Clamps n to -32768..32767.
int.to_int16_saturating(n: Int) -> Int16

// Clamps n to -2^31..2^31-1.
int.to_int32_saturating(n: Int) -> Int32

// Clamps n to 0..255.
int.to_uint8_saturating(n: Int) -> UInt8

// Clamps n to 0..65535.
int.to_uint16_saturating(n: Int) -> UInt16

// Clamps n to 0..2^32-1.
int.to_uint32_saturating(n: Int) -> UInt32

// Negative n clamps to 0.
int.to_uint64_saturating(n: Int) -> UInt64

// some(n), or none if n exceeds 2^63-1.
int.from_uint64_checked(n: UInt64) -> Option[Int]

// Clamps n to at most 2^63-1.
int.from_uint64_saturating(n: UInt64) -> Int

// Smallest Int: -2^63.
int.min_value() -> Int

// Largest Int: 2^63-1.
int.max_value() -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
