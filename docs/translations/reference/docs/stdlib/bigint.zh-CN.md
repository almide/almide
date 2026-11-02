# bigint

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bigint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bigint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bigint.zh-CN.md)



> 原文：[bigint.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bigint.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。
>
> 原文注意：开头“值不会过大”不表示取消下文的转换位数、结果大小或资源限制。类型索引是固定原文副本。

任意精度整数 `BigInt`：值不会过大，也不会回绕。需要 `import bigint`。pure，两个目标行为相同。

拼写与 `int` 相同：`parse` / `to_string` 及其 `_radix` 对、`compare`、`min`、`max`、`clamp`、`abs`、`sign`、`pow`、`floor_div`、`modulo`、`is_even`、`is_odd`、`bit_length` 和带 `b` 前缀的位运算。负值是高位有无限个 1 的二进制补码，因此 `bnot(x)` 为 `-x - 1`，`bshr` 向 -inf 舍入。值相同的两个 BigInt 为 `==`。

限制（#4196）：基数不是二的幂的文本（包括十进制）转换耗时与长度平方成正比，因此限制为 4300 位，与 Python 默认值相同。超过该位数时（前导零计数，符号不计），`parse` / `parse_radix` 返回 `err("integer string conversion exceeds the limit of 4300 digits")`，而 `to_string` / `to_string_radix` 在结果会更长时以相同文本 abort。基数 2、4、8、16、32 在线性时间转换且无此限制；巨大值请用十六进制打印。`pow` 在计算必定超过 30 * 2^28 位（2^28 limbs，也是 `bshl` 会遇到的列表上限）的结果前，以 `pow result too large` abort。

BigInt 支持算术运算符 `+ - * / %`。用 `parse`、`pow` 或 `bigint.from(x)` 构造值；转换接受数值来源，向零截断，NaN 或无穷大时 abort。`bigint.from_checked(x)` 在来源不是有限整数时返回 `none`。

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



## 固定快照 API 索引 (24 个函数)

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

## 固定快照类型索引（1 个类型）

```
// An arbitrary-precision integer.
// @since unreleased
type bigint.BigInt = { neg: Bool, limbs: List[Int] }
```



## API 说明译文（按上述签名顺序）

- `bigint.parse(s: String) -> Result[BigInt, String]` — 从去掉首尾空白的十进制 s 得到 BigInt，可带 + 或 -；非法内容或超过 4300 位为 err。
- `bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` — 从去掉首尾空白的 radix 进制 s 得到 BigInt；可带 +/-，大小写均可，16/8/2 允许 0x/0o/0b；非法内容，以及非二的幂基数超过 4300 位时为 err；radix 超出 2..36 则 abort。
- `bigint.to_string(x: BigInt) -> String` — 十进制数字，负数带 -；超过 4300 位则 abort。
- `bigint.to_string_radix(x: BigInt, radix: Int) -> String` — x 的 radix 进制小写数字，负数带 -；radix 超出 2..36 则 abort，非二的幂基数文本会超过 4300 位时也 abort。
- `bigint.compare(a: BigInt, b: BigInt) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `bigint.min(a: BigInt, b: BigInt) -> BigInt` — a 与 b 中较小者。
- `bigint.max(a: BigInt, b: BigInt) -> BigInt` — a 与 b 中较大者。
- `bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt` — x 限制在 lo..hi；lo > hi 则 abort。
- `bigint.abs(x: BigInt) -> BigInt` — x 的大小（绝对值）。
- `bigint.sign(x: BigInt) -> Int` — 根据 x 符号返回 -1、0、1；0 返回 0。
- `bigint.is_even(x: BigInt) -> Bool` — x 是 2 的倍数时为 true。
- `bigint.is_odd(x: BigInt) -> Bool` — x 不是 2 的倍数时为 true。
- `bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` — 向 -inf 舍入的商；b == 0 则 abort。
- `bigint.modulo(a: BigInt, b: BigInt) -> BigInt` — 与 b 同号的余数，使 a == floor_div(a, b) * b + modulo(a, b)；b == 0 则 abort。
- `bigint.pow(base: BigInt, exp: Int) -> BigInt` — base 的 exp 次方；exp < 0 或结果必定超过 30 * 2^28 位时 abort。
- `bigint.bit_length(x: BigInt) -> Int` — 表示 x 绝对值所需位数；0 返回 0。
- `bigint.band(a: BigInt, b: BigInt) -> BigInt` — 按位 AND。
- `bigint.bor(a: BigInt, b: BigInt) -> BigInt` — 按位 OR。
- `bigint.bxor(a: BigInt, b: BigInt) -> BigInt` — 按位 XOR。
- `bigint.bnot(x: BigInt) -> BigInt` — 按位取反，等于 -x - 1。
- `bigint.bshl(a: BigInt, n: Int) -> BigInt` — a 左移 n 位（a * 2^n）；n < 0 则 abort。
- `bigint.bshr(a: BigInt, n: Int) -> BigInt` — a 右移 n 位，向 -inf 舍入（符号填充）；n < 0 则 abort。
- `bigint.from(x: S) -> BigInt` — x 向零转换为 BigInt；NaN 或无穷大则 abort。
- `bigint.from_checked(x: S) -> Option[BigInt]` — x 是整数时为 some(x as BigInt)，否则（小数、NaN、无穷大）为 none。
- `type bigint.BigInt = { neg: Bool, limbs: List[Int] }` — 任意精度整数。
