# uint64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](uint64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](uint64.zh-CN.md)



> 原文：[docs/stdlib/uint64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint64.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步。代码、签名、注释和版本注记保留原文，在外部附上 API 说明译文。翻译时未独立测量或验证实现。

> 原文转换函数族的引言泛指数值来源，但 from_wrapping 的表格及索引明确限定为整数来源。请按具体函数的条件理解；本译文保留此限制。

`UInt64`（64 位无符号，0 … 18,446,744,073,709,551,615）：边界、算术、位操作、文本，以及转换到此类型。自动导入，无需 `import`。

<a id="conversions"></a>

## 转换

转换按构造的目标类型命名（R-L0-40）：`uint64.from(x)` 及其同族函数接受任意数值来源——`Int`、指定位宽整数、`Float` 或指定位宽浮点数。

```almd
let x: UInt64 = uint64.from(12)
let wide = int.from(x)                 // leaving UInt64: the destination's `from`
let low = uint64.from_wrapping(-1)     // 18446744073709551615: the low 64 bits, two's complement
let fits = uint64.from_checked(-1)     // none: -1 is not a UInt64
let top = uint64.from_saturating(float.infinity()) // 18446744073709551615: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `uint64.from(x)` | x 向零舍入；NaN、无穷大或超出 UInt64 范围的值会 abort |
| `uint64.from_checked(x)` | UInt64 能精确容纳 x 时为 `some`，否则 `none`（小数部分、NaN、越界） |
| `uint64.from_saturating(x)` | x 向零舍入，限制在 UInt64 范围；NaN 为 0 |
| `uint64.from_wrapping(x)` | 仅整数来源：低 64 位，二进制补码 |

要从 `UInt64` 转到其他类型，请调用目标类型的函数族——`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` 等。目标能容纳来源的所有值时，`from` 是正确选择；缩窄的目标通过名称选择 abort、`none`、限幅或回绕，绝不静默进行。

> **传给接收 `Int` 的函数前，先转为 `Int`。** 指定位宽的值不会隐式扩展：checker 拒绝不匹配的类型。两个目标都用 `int.from(x)` 显式请求转换；越界值会 abort（若预期策略是 checked、saturating 或 wrapping，则使用对应变体）。

`min_value()` / `max_value()` 给出此类型的边界。准确接口见下方由机器维护的签名索引。

> 以下是原文生成索引的固定副本，本译文不是生成目标。

<a id="signature-index-35-functions"></a>

## 签名索引（35 个函数）

```
// Decimal digits, no sign; exact up to 2^64-1.
// @since 0.15.0 or earlier
uint64.to_string(x: UInt64) -> String

// Smallest UInt64: 0.
// @since 0.38.0
uint64.min_value() -> UInt64

// Largest UInt64: 2^64-1.
// @since 0.38.0
uint64.max_value() -> UInt64

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint64.compare(a: UInt64, b: UInt64) -> Int

// Lesser of a and b.
// @since unreleased
uint64.min(a: UInt64, b: UInt64) -> UInt64

// Greater of a and b.
// @since unreleased
uint64.max(a: UInt64, b: UInt64) -> UInt64

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint64.floor_div(a: UInt64, b: UInt64) -> UInt64

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint64.modulo(a: UInt64, b: UInt64) -> UInt64

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
uint64.pow(base: UInt64, exp: Int) -> UInt64

// True when x is a multiple of 2.
// @since unreleased
uint64.is_even(x: UInt64) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint64.is_odd(x: UInt64) -> Bool

// Bitwise AND.
// @since unreleased
uint64.band(a: UInt64, b: UInt64) -> UInt64

// Bitwise OR.
// @since unreleased
uint64.bor(a: UInt64, b: UInt64) -> UInt64

// Bitwise XOR.
// @since unreleased
uint64.bxor(a: UInt64, b: UInt64) -> UInt64

// Bitwise complement of all 64 bits.
// @since unreleased
uint64.bnot(x: UInt64) -> UInt64

// a shifted left by n bits; n is taken mod 64.
// @since unreleased
uint64.bshl(a: UInt64, n: Int) -> UInt64

// Zero-filling right shift; n is taken mod 64.
// @since unreleased
uint64.bshr(a: UInt64, n: Int) -> UInt64

// Zero bits above the highest set bit, counted in 64 bits; 64 for 0.
// @since unreleased
uint64.count_leading_zeros(x: UInt64) -> Int

// Zero bits below the lowest set bit; 64 for 0.
// @since unreleased
uint64.count_trailing_zeros(x: UInt64) -> Int

// Number of set bits among the 64.
// @since unreleased
uint64.count_ones(x: UInt64) -> Int

// The 8 bytes in reverse order (endian flip).
// @since unreleased
uint64.byte_swap(x: UInt64) -> UInt64

// a + b wrapped to 64 bits.
// @since unreleased
uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64

// a - b wrapped to 64 bits.
// @since unreleased
uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64

// a * b wrapped to 64 bits.
// @since unreleased
uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since unreleased
uint64.rotate_left(a: UInt64, n: Int) -> UInt64

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since unreleased
uint64.rotate_right(a: UInt64, n: Int) -> UInt64

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint64.bit_length(x: UInt64) -> Int

// Decimal UInt64 from ASCII-trimmed s; err on junk, a sign of -, or above 2^64-1.
// @since unreleased
uint64.parse(s: String) -> Result[UInt64, String]

// Digits of x in radix, lowercase, no sign; exact up to 2^64-1; radix outside 2..36 aborts.
// @since unreleased
uint64.to_string_radix(x: UInt64, radix: Int) -> String

// UInt64 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 2^64-1.
// @since unreleased
uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]

// x as UInt64, toward zero; NaN, an infinity or a value outside UInt64 aborts.
// @since unreleased
uint64.from(x: S) -> UInt64

// some(x as UInt64) when UInt64 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint64.from_checked(x: S) -> Option[UInt64]

// x as UInt64, toward zero, clamped to UInt64's range; NaN is 0.
// @since unreleased
uint64.from_saturating(x: S) -> UInt64

// The low 64 bits of integer x as UInt64 (two's complement).
// @since unreleased
uint64.from_wrapping(x: S) -> UInt64
```

## API 注释说明译文

- `uint64.to_string(x: UInt64) -> String` — 十进制数字，无符号；精确至 2^64-1。
- `uint64.min_value() -> UInt64` — 最小 UInt64：0。
- `uint64.max_value() -> UInt64` — 最大 UInt64：2^64-1。
- `uint64.compare(a: UInt64, b: UInt64) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint64.min(a: UInt64, b: UInt64) -> UInt64` — a 和 b 的较小者。
- `uint64.max(a: UInt64, b: UInt64) -> UInt64` — a 和 b 的较大者。
- `uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint64.modulo(a: UInt64, b: UInt64) -> UInt64` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint64.pow(base: UInt64, exp: Int) -> UInt64` — base 的 exp 次幂，像 * 一样回绕到 64 位；exp < 0 时 abort。
- `uint64.is_even(x: UInt64) -> Bool` — x 是 2 的倍数时为 true。
- `uint64.is_odd(x: UInt64) -> Bool` — x 不是 2 的倍数时为 true。
- `uint64.band(a: UInt64, b: UInt64) -> UInt64` — 按位 AND。
- `uint64.bor(a: UInt64, b: UInt64) -> UInt64` — 按位 OR。
- `uint64.bxor(a: UInt64, b: UInt64) -> UInt64` — 按位 XOR。
- `uint64.bnot(x: UInt64) -> UInt64` — 全部 64 位的按位取反。
- `uint64.bshl(a: UInt64, n: Int) -> UInt64` — a 左移 n 位；n 对 64 取模。
- `uint64.bshr(a: UInt64, n: Int) -> UInt64` — 零填充右移；n 对 64 取模。
- `uint64.count_leading_zeros(x: UInt64) -> Int` — 在 64 位中最高置位位之上的零位数；0 时为 64。
- `uint64.count_trailing_zeros(x: UInt64) -> Int` — 最低置位位之下的零位数；0 时为 64。
- `uint64.count_ones(x: UInt64) -> Int` — 64 位中置位的数量。
- `uint64.byte_swap(x: UInt64) -> UInt64` — 反转 8 字节顺序（翻转字节序）。
- `uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` — a + b 回绕到 64 位。
- `uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` — a - b 回绕到 64 位。
- `uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` — a * b 回绕到 64 位。
- `uint64.rotate_left(a: UInt64, n: Int) -> UInt64` — 将 a 的 64 位向左旋转 n；n 对 64 取模。
- `uint64.rotate_right(a: UInt64, n: Int) -> UInt64` — 将 a 的 64 位向右旋转 n；n 对 64 取模。
- `uint64.bit_length(x: UInt64) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint64.parse(s: String) -> Result[UInt64, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt64；无效内容、- 符号或超过 2^64-1 为 err。
- `uint64.to_string_radix(x: UInt64, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；精确至 2^64-1；radix 不在 2..36 时 abort。
- `uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` — 去除 s 两侧空白后按 radix 解析 UInt64，语法同 int.parse_radix；无效内容、- 符号或超过 2^64-1 为 err。
- `uint64.from(x: S) -> UInt64` — x 向零舍入为 UInt64；NaN、无穷大或超出 UInt64 范围时 abort。
- `uint64.from_checked(x: S) -> Option[UInt64]` — UInt64 能精确容纳 x 时为 some(x as UInt64)，否则（小数部分、NaN、越界）为 none。
- `uint64.from_saturating(x: S) -> UInt64` — x 向零舍入为 UInt64，限制在 UInt64 范围；NaN 为 0。
- `uint64.from_wrapping(x: S) -> UInt64` — 将整数 x 的低 64 位作为 UInt64（二进制补码）。
