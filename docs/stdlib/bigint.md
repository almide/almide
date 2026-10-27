# bigint

Arbitrary-precision integers, `BigInt`: no value is too large, nothing wraps.
Needs `import bigint`. Pure — identical on both targets.

The spellings are `int`'s: `parse` / `to_string` and their `_radix` pair,
`compare`, `min`, `max`, `clamp`, `abs`, `sign`, `pow`, `floor_div`, `modulo`,
`is_even`, `is_odd`, `bit_length` and the `b`-prefixed bitwise functions. A
negative value is two's complement with an endless run of 1 bits above it, so
`bnot(x)` is `-x - 1` and `bshr` rounds toward -inf. Two BigInts holding the same
value are `==`.

Limits (#4196). Text in a radix that is not a power of two (decimal included)
costs time quadratic in its length, so it is limited to 4300 digits, Python's
default: `parse` / `parse_radix` answer
`err("integer string conversion exceeds the limit of 4300 digits")` for more
digits (leading zeros count, the sign does not), and `to_string` /
`to_string_radix` abort with the same text when the result would be longer.
Radix 2, 4, 8, 16 and 32 convert in linear time and have no limit — print a
huge value in hex. `pow` aborts with `pow result too large` before computing a
result sure to pass 30 * 2^28 bits (2^28 limbs, the list ceiling `bshl` meets).

Not yet: the arithmetic operators `+ - * / %` on BigInt and `bigint.from` (a
conversion from any number type) wait on the language; build values with
`parse` and `pow` until then.

```almd run
import bigint

effect fn main() -> Unit = {
  let two = bigint.parse("2")!
  let p = bigint.pow(two, 100)
  println(bigint.to_string(p))
  println(bigint.to_string_radix(p, 16))
  println("${bigint.bit_length(p)} ${bigint.compare(p, two)}")
}
```
```output
1267650600228229401496703205376
10000000000000000000000000
101 1
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (24 functions)

```
// BigInt from trimmed decimal s with an optional + or -; err on junk or more than 4300 digits.
// @since unreleased
bigint.parse(s: String) -> Result[BigInt, String]

// BigInt from trimmed s in radix: a + or - sign, any case, and 0x, 0o, 0b allowed for 16, 8, 2;
// err on junk, and on more than 4300 digits in a radix that is not a power of two;
// radix outside 2..36 aborts.
// @since unreleased
bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]

// Decimal digits, with - when negative; more than 4300 digits aborts.
// @since unreleased
bigint.to_string(x: BigInt) -> String

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts, and so does
// a radix that is not a power of two when the text would pass 4300 digits.
// @since unreleased
bigint.to_string_radix(x: BigInt, radix: Int) -> String

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
bigint.compare(a: BigInt, b: BigInt) -> Int

// Lesser of a and b.
// @since unreleased
bigint.min(a: BigInt, b: BigInt) -> BigInt

// Greater of a and b.
// @since unreleased
bigint.max(a: BigInt, b: BigInt) -> BigInt

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt

// Magnitude of x.
// @since unreleased
bigint.abs(x: BigInt) -> BigInt

// -1, 0 or 1 by the sign of x; 0 for 0.
// @since unreleased
bigint.sign(x: BigInt) -> Int

// True when x is a multiple of 2.
// @since unreleased
bigint.is_even(x: BigInt) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
bigint.is_odd(x: BigInt) -> Bool

// Quotient rounded toward -inf; b == 0 aborts.
// @since unreleased
bigint.floor_div(a: BigInt, b: BigInt) -> BigInt

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
bigint.modulo(a: BigInt, b: BigInt) -> BigInt

// base raised to exp; exp < 0 aborts, and so does a result sure to pass 30 * 2^28 bits.
// @since unreleased
bigint.pow(base: BigInt, exp: Int) -> BigInt

// Bits needed to hold the magnitude of x; 0 for 0.
// @since unreleased
bigint.bit_length(x: BigInt) -> Int

// Bitwise AND.
// @since unreleased
bigint.band(a: BigInt, b: BigInt) -> BigInt

// Bitwise OR.
// @since unreleased
bigint.bor(a: BigInt, b: BigInt) -> BigInt

// Bitwise XOR.
// @since unreleased
bigint.bxor(a: BigInt, b: BigInt) -> BigInt

// Bitwise complement; equals -x - 1.
// @since unreleased
bigint.bnot(x: BigInt) -> BigInt

// a shifted left by n bits (a * 2^n); n < 0 aborts.
// @since unreleased
bigint.bshl(a: BigInt, n: Int) -> BigInt

// a shifted right by n bits, rounding toward -inf (sign-filling); n < 0 aborts.
// @since unreleased
bigint.bshr(a: BigInt, n: Int) -> BigInt

// x as BigInt, toward zero; NaN or an infinity aborts.
// @since unreleased
bigint.from(x: S) -> BigInt

// some(x as BigInt) when x is an integer, else none (a fraction, NaN, an infinity).
// @since unreleased
bigint.from_checked(x: S) -> Option[BigInt]
```

## Type index (1 types)

```
// An arbitrary-precision integer.
// @since unreleased
type bigint.BigInt = { neg: Bool, limbs: List[Int] }
```

<!-- END GENERATED SIGNATURE INDEX -->
