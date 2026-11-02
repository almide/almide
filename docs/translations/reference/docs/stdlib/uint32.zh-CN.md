# uint32

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint32.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](uint32.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](uint32.zh-CN.md)



> 原文：[docs/stdlib/uint32.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint32.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步。代码、签名、注释和版本注记保留原文，在外部附上 API 说明译文。翻译时未独立测量或验证实现。

> 原文转换函数族的引言泛指数值来源，但 from_wrapping 的表格及索引明确限定为整数来源。请按具体函数的条件理解；本译文保留此限制。

`UInt32`（32 位无符号，0 … 4,294,967,295）：边界、算术、位操作、文本，以及转换到此类型。自动导入，无需 `import`。

## 转换

转换按构造的目标类型命名（R-L0-40）：`uint32.from(x)` 及其同族函数接受任意数值来源——`Int`、指定位宽整数、`Float` 或指定位宽浮点数。

```almd
let x: UInt32 = uint32.from(12)
let wide = int.from(x)                 // leaving UInt32: the destination's `from`
let low = uint32.from_wrapping(4294967340)// 44: the low 32 bits, two's complement
let fits = uint32.from_checked(4294967296)// none: 4294967296 is not a UInt32
let top = uint32.from_saturating(float.infinity()) // 4294967295: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `uint32.from(x)` | x 向零舍入；NaN、无穷大或超出 UInt32 范围的值会 abort |
| `uint32.from_checked(x)` | UInt32 能精确容纳 x 时为 `some`，否则 `none`（小数部分、NaN、越界） |
| `uint32.from_saturating(x)` | x 向零舍入，限制在 UInt32 范围；NaN 为 0 |
| `uint32.from_wrapping(x)` | 仅整数来源：低 32 位，二进制补码 |

要从 `UInt32` 转到其他类型，请调用目标类型的函数族——`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` 等。目标能容纳来源的所有值时，`from` 是正确选择；缩窄的目标通过名称选择 abort、`none`、限幅或回绕，绝不静默进行。

> **传给接收 `Int` 的函数前，先转为 `Int`。** 指定位宽的值不会隐式扩展：checker 拒绝不匹配的类型。两个目标都用 `int.from(x)` 显式请求转换；越界值会 abort（若预期策略是 checked、saturating 或 wrapping，则使用对应变体）。

指定位宽类型的字面量在编译时检查范围——`let a: UInt32 = 5000000000` 是 E024 错误，不会静默折叠。

`min_value()` / `max_value()` 给出此类型的边界。准确接口见下方由机器维护的签名索引。

> 以下是原文生成索引的固定副本，本译文不是生成目标。

## 签名索引（35 个函数）

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint32.to_string(x: UInt32) -> String

// Smallest UInt32: 0.
// @since 0.38.0
uint32.min_value() -> UInt32

// Largest UInt32: 4294967295.
// @since 0.38.0
uint32.max_value() -> UInt32

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint32.compare(a: UInt32, b: UInt32) -> Int

// Lesser of a and b.
// @since unreleased
uint32.min(a: UInt32, b: UInt32) -> UInt32

// Greater of a and b.
// @since unreleased
uint32.max(a: UInt32, b: UInt32) -> UInt32

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint32.floor_div(a: UInt32, b: UInt32) -> UInt32

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint32.modulo(a: UInt32, b: UInt32) -> UInt32

// base raised to exp, wrapping to 32 bits like *; exp < 0 aborts.
// @since unreleased
uint32.pow(base: UInt32, exp: Int) -> UInt32

// True when x is a multiple of 2.
// @since unreleased
uint32.is_even(x: UInt32) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint32.is_odd(x: UInt32) -> Bool

// Bitwise AND.
// @since unreleased
uint32.band(a: UInt32, b: UInt32) -> UInt32

// Bitwise OR.
// @since unreleased
uint32.bor(a: UInt32, b: UInt32) -> UInt32

// Bitwise XOR.
// @since unreleased
uint32.bxor(a: UInt32, b: UInt32) -> UInt32

// Bitwise complement of all 32 bits.
// @since unreleased
uint32.bnot(x: UInt32) -> UInt32

// a shifted left by n bits; n is taken mod 32.
// @since unreleased
uint32.bshl(a: UInt32, n: Int) -> UInt32

// Zero-filling right shift; n is taken mod 32.
// @since unreleased
uint32.bshr(a: UInt32, n: Int) -> UInt32

// Zero bits above the highest set bit, counted in 32 bits; 32 for 0.
// @since unreleased
uint32.count_leading_zeros(x: UInt32) -> Int

// Zero bits below the lowest set bit; 32 for 0.
// @since unreleased
uint32.count_trailing_zeros(x: UInt32) -> Int

// Number of set bits among the 32.
// @since unreleased
uint32.count_ones(x: UInt32) -> Int

// The 4 bytes in reverse order (endian flip).
// @since unreleased
uint32.byte_swap(x: UInt32) -> UInt32

// a + b wrapped to 32 bits.
// @since unreleased
uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32

// a - b wrapped to 32 bits.
// @since unreleased
uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32

// a * b wrapped to 32 bits.
// @since unreleased
uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32

// The 32 bits of a rotated left by n; n is taken mod 32.
// @since unreleased
uint32.rotate_left(a: UInt32, n: Int) -> UInt32

// The 32 bits of a rotated right by n; n is taken mod 32.
// @since unreleased
uint32.rotate_right(a: UInt32, n: Int) -> UInt32

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint32.bit_length(x: UInt32) -> Int

// Decimal UInt32 from ASCII-trimmed s; err on junk, a sign of -, or above 4294967295.
// @since unreleased
uint32.parse(s: String) -> Result[UInt32, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint32.to_string_radix(x: UInt32, radix: Int) -> String

// UInt32 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 4294967295.
// @since unreleased
uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]

// x as UInt32, toward zero; NaN, an infinity or a value outside UInt32 aborts.
// @since unreleased
uint32.from(x: S) -> UInt32

// some(x as UInt32) when UInt32 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint32.from_checked(x: S) -> Option[UInt32]

// x as UInt32, toward zero, clamped to UInt32's range; NaN is 0.
// @since unreleased
uint32.from_saturating(x: S) -> UInt32

// The low 32 bits of integer x as UInt32 (two's complement).
// @since unreleased
uint32.from_wrapping(x: S) -> UInt32
```

## API 注释说明译文

- `uint32.to_string(x: UInt32) -> String` — 十进制数字，绝不带符号。
- `uint32.min_value() -> UInt32` — 最小 UInt32：0。
- `uint32.max_value() -> UInt32` — 最大 UInt32：4294967295。
- `uint32.compare(a: UInt32, b: UInt32) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint32.min(a: UInt32, b: UInt32) -> UInt32` — a 和 b 的较小者。
- `uint32.max(a: UInt32, b: UInt32) -> UInt32` — a 和 b 的较大者。
- `uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint32.modulo(a: UInt32, b: UInt32) -> UInt32` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint32.pow(base: UInt32, exp: Int) -> UInt32` — base 的 exp 次幂，像 * 一样回绕到 32 位；exp < 0 时 abort。
- `uint32.is_even(x: UInt32) -> Bool` — x 是 2 的倍数时为 true。
- `uint32.is_odd(x: UInt32) -> Bool` — x 不是 2 的倍数时为 true。
- `uint32.band(a: UInt32, b: UInt32) -> UInt32` — 按位 AND。
- `uint32.bor(a: UInt32, b: UInt32) -> UInt32` — 按位 OR。
- `uint32.bxor(a: UInt32, b: UInt32) -> UInt32` — 按位 XOR。
- `uint32.bnot(x: UInt32) -> UInt32` — 全部 32 位的按位取反。
- `uint32.bshl(a: UInt32, n: Int) -> UInt32` — a 左移 n 位；n 对 32 取模。
- `uint32.bshr(a: UInt32, n: Int) -> UInt32` — 零填充右移；n 对 32 取模。
- `uint32.count_leading_zeros(x: UInt32) -> Int` — 在 32 位中最高置位位之上的零位数；0 时为 32。
- `uint32.count_trailing_zeros(x: UInt32) -> Int` — 最低置位位之下的零位数；0 时为 32。
- `uint32.count_ones(x: UInt32) -> Int` — 32 位中置位的数量。
- `uint32.byte_swap(x: UInt32) -> UInt32` — 反转 4 字节顺序（翻转字节序）。
- `uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` — a + b 回绕到 32 位。
- `uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` — a - b 回绕到 32 位。
- `uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` — a * b 回绕到 32 位。
- `uint32.rotate_left(a: UInt32, n: Int) -> UInt32` — 将 a 的 32 位向左旋转 n；n 对 32 取模。
- `uint32.rotate_right(a: UInt32, n: Int) -> UInt32` — 将 a 的 32 位向右旋转 n；n 对 32 取模。
- `uint32.bit_length(x: UInt32) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint32.parse(s: String) -> Result[UInt32, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt32；无效内容、- 符号或超过 4294967295 为 err。
- `uint32.to_string_radix(x: UInt32, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；radix 不在 2..36 时 abort。
- `uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` — 去除 s 两侧空白后按 radix 解析 UInt32，语法同 int.parse_radix；无效内容、- 符号或超过 4294967295 为 err。
- `uint32.from(x: S) -> UInt32` — x 向零舍入为 UInt32；NaN、无穷大或超出 UInt32 范围时 abort。
- `uint32.from_checked(x: S) -> Option[UInt32]` — UInt32 能精确容纳 x 时为 some(x as UInt32)，否则（小数部分、NaN、越界）为 none。
- `uint32.from_saturating(x: S) -> UInt32` — x 向零舍入为 UInt32，限制在 UInt32 范围；NaN 为 0。
- `uint32.from_wrapping(x: S) -> UInt32` — 将整数 x 的低 32 位作为 UInt32（二进制补码）。
