# int

Integer arithmetic and bitwise. auto-imported.

### `int.to_string(x: Int) -> String`

Convert an integer to its decimal string representation.

```almd run
fn main() -> Unit = {
  println(int.to_string(42))
}
```
```output
42
```

### `int.to_hex(x: Int) -> String`

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

Only ASCII whitespace (space, tab, LF, CR, form feed) around the number is ignored. Any other whitespace, such as a no-break space (U+00A0), an ideographic space (U+3000) or a vertical tab, is an error: `int.parse("\u{A0}5")` is `err("invalid digit found in string")`. Call `string.trim` first if you want to accept Unicode whitespace.

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

### `int.abs(x: Int) -> Int`

Return the absolute value of an integer.

```almd run
fn main() -> Unit = {
  println("${int.abs(-5)}")
}
```
```output
5
```

### `int.is_even(x: Int) -> Bool`

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

### `int.is_odd(x: Int) -> Bool`

The other half of the parity pair: `true` when `n` is not divisible by 2.
Scalar `int` only — a sized value reaches it through `int.from(x)`.

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

### `int.bnot(x: Int) -> Int`

Bitwise NOT (complement) of an integer.

```almd run
fn main() -> Unit = {
  println("${int.bnot(0)}")
}
```
```output
-1
```

### `int.wrap_add(a: Int, b: Int) -> Int`

Wrapping addition over the 64-bit Int: overflow wraps around. For an N-bit
register, compose with `int.wrap`: `int.wrap(int.wrap_add(a, b), bits)`.

```almd run
fn main() -> Unit = {
  println("${int.wrap_add(int.max_value(), 1)}")
  println("${int.wrap(int.wrap_add(255, 1), 8)}")
}
```
```output
-9223372036854775808
0
```

### `int.wrap_mul(a: Int, b: Int) -> Int`

Wrapping multiplication over the 64-bit Int: overflow wraps around. For an
N-bit register, compose with `int.wrap`.

```almd run
fn main() -> Unit = {
  println("${int.wrap(int.wrap_mul(16, 16), 8)}")
}
```
```output
0
```

### `int.rotate_right(a: Int, n: Int) -> Int`

Rotate the 64 bits of `a` right. The count `n` is taken modulo 64
(Euclidean), so a negative count rotates left: `rotate_right(1, -1) = 2`.

```almd run
fn main() -> Unit = {
  println("${int.rotate_right(4, 1)}")
}
```
```output
2
```

### `int.rotate_left(a: Int, n: Int) -> Int`

Rotate the 64 bits of `a` left. The count `n` is taken modulo 64
(Euclidean), so a negative count rotates right: `rotate_left(2, -1) = 1`.

```almd run
fn main() -> Unit = {
  println("${int.rotate_left(int.min_value(), 1)}")
}
```
```output
1
```

### `int.clamp(x: Int, lo: Int, hi: Int) -> Int`

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

## Matrix completeness (#956)

A conversion is named after the type it builds (R-L0-40): `int.from(x)` (aborts
outside the range), `int.from_checked(x)` (`none`), `int.from_saturating(x)`
(clamp, NaN 0) and `int.from_wrapping(x)` (integer sources, two's complement)
take numeric sources, including `UInt64 → Int`, whose upper range does not
fit in `Int`. Every sized module carries the same family for its own type,
and every fixed-width numeric carrier
has `min_value()`/`max_value()`.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (47 functions)

```
// Decimal digits, with - when negative.
// @since 0.5.0 or earlier
int.to_string(x: Int) -> String

// Lowercase hex, no 0x; -1 gives 16 f digits.
// @since 0.5.0 or earlier
int.to_hex(x: Int) -> String

// Decimal Int from s trimmed of ASCII space/tab/LF/FF/CR; err on junk or overflow.
// @since 0.5.0 or earlier
int.parse(s: String) -> Result[Int, String]

// Magnitude of x; min_value wraps to itself.
// @since 0.5.0 or earlier
int.abs(x: Int) -> Int

// Lesser of a and b.
// @since 0.5.0 or earlier
int.min(a: Int, b: Int) -> Int

// Greater of a and b.
// @since 0.5.0 or earlier
int.max(a: Int, b: Int) -> Int

// x limited to lo..hi; aborts when lo > hi.
// @since 0.5.0 or earlier
int.clamp(x: Int, lo: Int, hi: Int) -> Int

// True when x % 2 == 0; exact for negatives.
// @since 0.62.0
int.is_even(x: Int) -> Bool

// True when x % 2 != 0; -3 counts as odd.
// @since 0.62.0
int.is_odd(x: Int) -> Bool

// Bitwise AND over all 64 bits.
// @since 0.5.0 or earlier
int.band(a: Int, b: Int) -> Int

// Bitwise OR over all 64 bits.
// @since 0.5.0 or earlier
int.bor(a: Int, b: Int) -> Int

// Bitwise XOR over all 64 bits.
// @since 0.5.0 or earlier
int.bxor(a: Int, b: Int) -> Int

// Bitwise complement; equals -x - 1.
// @since 0.5.0 or earlier
int.bnot(x: Int) -> Int

// a shifted left by n bits; n is taken mod 64.
// @since 0.5.0 or earlier
int.bshl(a: Int, n: Int) -> Int

// Arithmetic (sign-filling) right shift; n mod 64.
// @since 0.5.0 or earlier
int.bshr(a: Int, n: Int) -> Int

// Leading zero bits of the 64-bit word; 64 for 0.
// @since 0.16.0 or earlier
int.count_leading_zeros(x: Int) -> Int

// Trailing zero bits; 64 for 0.
// @since 0.16.0 or earlier
int.count_trailing_zeros(x: Int) -> Int

// Number of set bits; 64 for -1.
// @since 0.16.0 or earlier
int.pop_count(n: Int) -> Int   (deprecated — use int.count_ones)

// Number of set bits; 64 for -1.
// @since unreleased
int.count_ones(x: Int) -> Int

// The 64 bits in reverse order; 1 gives min_value.
// @since 0.16.0 or earlier
int.bit_reverse(x: Int) -> Int

// The 8 bytes in reverse order (endian flip).
// @since 0.16.0 or earlier
int.byte_swap(x: Int) -> Int

// Bits needed to hold n; 0 for 0, 64 if negative.
// @since 0.16.0 or earlier
int.bit_width(n: Int) -> Int   (deprecated — use int.bit_length)

// Bits needed to hold x; 0 for 0, 64 if negative.
// @since unreleased
int.bit_length(x: Int) -> Int

// Floor of log2(x); -1 when x <= 0.
// @since 0.16.0 or earlier
int.log2_floor(x: Int) -> Int

// Ceiling of log2(x); 0 when x <= 1.
// @since 0.16.0 or earlier
int.log2_ceil(x: Int) -> Int

// Least power of 2 >= x; 1 if x <= 1; min_value past 2^62.
// @since 0.16.0 or earlier
int.next_power_of_two(x: Int) -> Int

// Greatest power of 2 <= x; 0 when x <= 0.
// @since 0.16.0 or earlier
int.prev_power_of_two(x: Int) -> Int

// a + b wrapped to 64 bits.
// @since 0.5.0 or earlier
int.wrap_add(a: Int, b: Int) -> Int

// a * b wrapped to 64 bits.
// @since 0.5.0 or earlier
int.wrap_mul(a: Int, b: Int) -> Int

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since 0.5.0 or earlier
int.rotate_right(a: Int, n: Int) -> Int

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since 0.5.0 or earlier
int.rotate_left(a: Int, n: Int) -> Int

// Smallest Int: -2^63.
// @since 0.38.0
int.min_value() -> Int

// Largest Int: 2^63-1.
// @since 0.38.0
int.max_value() -> Int

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int.compare(a: Int, b: Int) -> Int

// -1, 0 or 1 by the sign of x; 0 for 0.
// @since unreleased
int.sign(x: Int) -> Int

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
int.pow(base: Int, exp: Int) -> Int

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int.floor_div(a: Int, b: Int) -> Int

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int.modulo(a: Int, b: Int) -> Int

// x mod 2^bits, non-negative when bits < 64; bits <= 0 aborts, bits >= 64 keeps x.
// @since unreleased
int.wrap(x: Int, bits: Int) -> Int

// a - b wrapped to 64 bits.
// @since unreleased
int.wrap_sub(a: Int, b: Int) -> Int

// Zero-filling right shift of the 64 bits; n is taken mod 64.
// @since unreleased
int.bshr_logical(a: Int, n: Int) -> Int

// Signed digits of x in radix, lowercase; -255 in 16 is "-ff"; radix outside 2..36 aborts.
// @since unreleased
int.to_string_radix(x: Int, radix: Int) -> String

// Int from trimmed s in radix: a + or - sign, any case, and 0x, 0o, 0b allowed for 16, 8, 2;
// err on junk or overflow; radix outside 2..36 aborts.
// @since unreleased
int.parse_radix(s: String, radix: Int) -> Result[Int, String]

// x as Int, toward zero; NaN, an infinity or a value outside Int aborts.
// @since unreleased
int.from(x: S) -> Int

// some(x as Int) when Int holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int.from_checked(x: S) -> Option[Int]

// x as Int, toward zero, clamped to Int's range; NaN is 0.
// @since unreleased
int.from_saturating(x: S) -> Int

// The low 64 bits of integer x as Int (two's complement).
// @since unreleased
int.from_wrapping(x: S) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
