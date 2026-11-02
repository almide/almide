# uint8

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint8.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](uint8.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](uint8.zh-CN.md)



> 原文：[docs/stdlib/uint8.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint8.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步。代码、签名、注释和版本注记保留原文，在外部附上 API 说明译文。翻译时未独立测量或验证实现。

> 原文转换函数族的引言泛指数值来源，但 from_wrapping 的表格及索引明确限定为整数来源。请按具体函数的条件理解；本译文保留此限制。

`UInt8`（8 位无符号，0 … 255）：边界、算术、位操作、文本，以及转换到此类型。自动导入，无需 `import`。

## 转换

转换按构造的目标类型命名（R-L0-40）：`uint8.from(x)` 及其同族函数接受任意数值来源——`Int`、指定位宽整数、`Float` 或指定位宽浮点数。

```almd
let x: UInt8 = uint8.from(12)
let wide = int.from(x)                 // leaving UInt8: the destination's `from`
let low = uint8.from_wrapping(300)     // 44: the low 8 bits, two's complement
let fits = uint8.from_checked(256)     // none: 256 is not a UInt8
let top = uint8.from_saturating(float.infinity()) // 255: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `uint8.from(x)` | x 向零舍入；NaN、无穷大或超出 UInt8 范围的值会 abort |
| `uint8.from_checked(x)` | UInt8 能精确容纳 x 时为 `some`，否则 `none`（小数部分、NaN、越界） |
| `uint8.from_saturating(x)` | x 向零舍入，限制在 UInt8 范围；NaN 为 0 |
| `uint8.from_wrapping(x)` | 仅整数来源：低 8 位，二进制补码 |

要从 `UInt8` 转到其他类型，请调用目标类型的函数族——`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` 等。目标能容纳来源的所有值时，`from` 是正确选择；缩窄的目标通过名称选择 abort、`none`、限幅或回绕，绝不静默进行。

> **传给接收 `Int` 的函数前，先转为 `Int`。** 指定位宽的值不会隐式扩展：checker 拒绝不匹配的类型。两个目标都用 `int.from(x)` 显式请求转换；越界值会 abort（若预期策略是 checked、saturating 或 wrapping，则使用对应变体）。

指定位宽类型的字面量在编译时检查范围——`let a: UInt8 = 256` 是 E024 错误，不会静默折叠。

`min_value()` / `max_value()` 给出此类型的边界。准确接口见下方由机器维护的签名索引。

> 以下是原文生成索引的固定副本，本译文不是生成目标。

## 签名索引（35 个函数）

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint8.to_string(x: UInt8) -> String

// Smallest UInt8: 0.
// @since 0.38.0
uint8.min_value() -> UInt8

// Largest UInt8: 255.
// @since 0.38.0
uint8.max_value() -> UInt8

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint8.compare(a: UInt8, b: UInt8) -> Int

// Lesser of a and b.
// @since unreleased
uint8.min(a: UInt8, b: UInt8) -> UInt8

// Greater of a and b.
// @since unreleased
uint8.max(a: UInt8, b: UInt8) -> UInt8

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint8.floor_div(a: UInt8, b: UInt8) -> UInt8

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint8.modulo(a: UInt8, b: UInt8) -> UInt8

// base raised to exp, wrapping to 8 bits like *; exp < 0 aborts.
// @since unreleased
uint8.pow(base: UInt8, exp: Int) -> UInt8

// True when x is a multiple of 2.
// @since unreleased
uint8.is_even(x: UInt8) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint8.is_odd(x: UInt8) -> Bool

// Bitwise AND.
// @since unreleased
uint8.band(a: UInt8, b: UInt8) -> UInt8

// Bitwise OR.
// @since unreleased
uint8.bor(a: UInt8, b: UInt8) -> UInt8

// Bitwise XOR.
// @since unreleased
uint8.bxor(a: UInt8, b: UInt8) -> UInt8

// Bitwise complement of all 8 bits.
// @since unreleased
uint8.bnot(x: UInt8) -> UInt8

// a shifted left by n bits; n is taken mod 8.
// @since unreleased
uint8.bshl(a: UInt8, n: Int) -> UInt8

// Zero-filling right shift; n is taken mod 8.
// @since unreleased
uint8.bshr(a: UInt8, n: Int) -> UInt8

// Zero bits above the highest set bit, counted in 8 bits; 8 for 0.
// @since unreleased
uint8.count_leading_zeros(x: UInt8) -> Int

// Zero bits below the lowest set bit; 8 for 0.
// @since unreleased
uint8.count_trailing_zeros(x: UInt8) -> Int

// Number of set bits among the 8.
// @since unreleased
uint8.count_ones(x: UInt8) -> Int

// The bytes in reverse order; one byte, so x itself.
// @since unreleased
uint8.byte_swap(x: UInt8) -> UInt8

// a + b wrapped to 8 bits.
// @since unreleased
uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8

// a - b wrapped to 8 bits.
// @since unreleased
uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8

// a * b wrapped to 8 bits.
// @since unreleased
uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8

// The 8 bits of a rotated left by n; n is taken mod 8.
// @since unreleased
uint8.rotate_left(a: UInt8, n: Int) -> UInt8

// The 8 bits of a rotated right by n; n is taken mod 8.
// @since unreleased
uint8.rotate_right(a: UInt8, n: Int) -> UInt8

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint8.bit_length(x: UInt8) -> Int

// Decimal UInt8 from ASCII-trimmed s; err on junk, a sign of -, or above 255.
// @since unreleased
uint8.parse(s: String) -> Result[UInt8, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint8.to_string_radix(x: UInt8, radix: Int) -> String

// UInt8 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 255.
// @since unreleased
uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]

// x as UInt8, toward zero; NaN, an infinity or a value outside UInt8 aborts.
// @since unreleased
uint8.from(x: S) -> UInt8

// some(x as UInt8) when UInt8 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint8.from_checked(x: S) -> Option[UInt8]

// x as UInt8, toward zero, clamped to UInt8's range; NaN is 0.
// @since unreleased
uint8.from_saturating(x: S) -> UInt8

// The low 8 bits of integer x as UInt8 (two's complement).
// @since unreleased
uint8.from_wrapping(x: S) -> UInt8
```

## API 注释说明译文

- `uint8.to_string(x: UInt8) -> String` — 十进制数字，绝不带符号。
- `uint8.min_value() -> UInt8` — 最小 UInt8：0。
- `uint8.max_value() -> UInt8` — 最大 UInt8：255。
- `uint8.compare(a: UInt8, b: UInt8) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint8.min(a: UInt8, b: UInt8) -> UInt8` — a 和 b 的较小者。
- `uint8.max(a: UInt8, b: UInt8) -> UInt8` — a 和 b 的较大者。
- `uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint8.modulo(a: UInt8, b: UInt8) -> UInt8` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint8.pow(base: UInt8, exp: Int) -> UInt8` — base 的 exp 次幂，像 * 一样回绕到 8 位；exp < 0 时 abort。
- `uint8.is_even(x: UInt8) -> Bool` — x 是 2 的倍数时为 true。
- `uint8.is_odd(x: UInt8) -> Bool` — x 不是 2 的倍数时为 true。
- `uint8.band(a: UInt8, b: UInt8) -> UInt8` — 按位 AND。
- `uint8.bor(a: UInt8, b: UInt8) -> UInt8` — 按位 OR。
- `uint8.bxor(a: UInt8, b: UInt8) -> UInt8` — 按位 XOR。
- `uint8.bnot(x: UInt8) -> UInt8` — 全部 8 位的按位取反。
- `uint8.bshl(a: UInt8, n: Int) -> UInt8` — a 左移 n 位；n 对 8 取模。
- `uint8.bshr(a: UInt8, n: Int) -> UInt8` — 零填充右移；n 对 8 取模。
- `uint8.count_leading_zeros(x: UInt8) -> Int` — 在 8 位中最高置位位之上的零位数；0 时为 8。
- `uint8.count_trailing_zeros(x: UInt8) -> Int` — 最低置位位之下的零位数；0 时为 8。
- `uint8.count_ones(x: UInt8) -> Int` — 8 位中置位的数量。
- `uint8.byte_swap(x: UInt8) -> UInt8` — 反转字节顺序；仅一个字节，因此为 x 自身。
- `uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` — a + b 回绕到 8 位。
- `uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` — a - b 回绕到 8 位。
- `uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` — a * b 回绕到 8 位。
- `uint8.rotate_left(a: UInt8, n: Int) -> UInt8` — 将 a 的 8 位向左旋转 n；n 对 8 取模。
- `uint8.rotate_right(a: UInt8, n: Int) -> UInt8` — 将 a 的 8 位向右旋转 n；n 对 8 取模。
- `uint8.bit_length(x: UInt8) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint8.parse(s: String) -> Result[UInt8, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt8；无效内容、- 符号或超过 255 为 err。
- `uint8.to_string_radix(x: UInt8, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；radix 不在 2..36 时 abort。
- `uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` — 去除 s 两侧空白后按 radix 解析 UInt8，语法同 int.parse_radix；无效内容、- 符号或超过 255 为 err。
- `uint8.from(x: S) -> UInt8` — x 向零舍入为 UInt8；NaN、无穷大或超出 UInt8 范围时 abort。
- `uint8.from_checked(x: S) -> Option[UInt8]` — UInt8 能精确容纳 x 时为 some(x as UInt8)，否则（小数部分、NaN、越界）为 none。
- `uint8.from_saturating(x: S) -> UInt8` — x 向零舍入为 UInt8，限制在 UInt8 范围；NaN 为 0。
- `uint8.from_wrapping(x: S) -> UInt8` — 将整数 x 的低 8 位作为 UInt8（二进制补码）。
