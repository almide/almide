# uint16

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint16.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](uint16.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](uint16.zh-CN.md)



> 原文：[docs/stdlib/uint16.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint16.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步。代码、签名、注释和版本注记保留原文，在外部附上 API 说明译文。翻译时未独立测量或验证实现。

> 原文转换函数族的引言泛指数值来源，但 from_wrapping 的表格及索引明确限定为整数来源。请按具体函数的条件理解；本译文保留此限制。

`UInt16`（16 位无符号，0 … 65,535）：边界、算术、位操作、文本，以及转换到此类型。自动导入，无需 `import`。

## 转换

转换按构造的目标类型命名（R-L0-40）：`uint16.from(x)` 及其同族函数接受任意数值来源——`Int`、指定位宽整数、`Float` 或指定位宽浮点数。

```almd
let x: UInt16 = uint16.from(12)
let wide = int.from(x)                 // leaving UInt16: the destination's `from`
let low = uint16.from_wrapping(65580)  // 44: the low 16 bits, two's complement
let fits = uint16.from_checked(65536)  // none: 65536 is not a UInt16
let top = uint16.from_saturating(float.infinity()) // 65535: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `uint16.from(x)` | x 向零舍入；NaN、无穷大或超出 UInt16 范围的值会 abort |
| `uint16.from_checked(x)` | UInt16 能精确容纳 x 时为 `some`，否则 `none`（小数部分、NaN、越界） |
| `uint16.from_saturating(x)` | x 向零舍入，限制在 UInt16 范围；NaN 为 0 |
| `uint16.from_wrapping(x)` | 仅整数来源：低 16 位，二进制补码 |

要从 `UInt16` 转到其他类型，请调用目标类型的函数族——`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` 等。目标能容纳来源的所有值时，`from` 是正确选择；缩窄的目标通过名称选择 abort、`none`、限幅或回绕，绝不静默进行。

> **传给接收 `Int` 的函数前，先转为 `Int`。** 指定位宽的值不会隐式扩展：checker 拒绝不匹配的类型。两个目标都用 `int.from(x)` 显式请求转换；越界值会 abort（若预期策略是 checked、saturating 或 wrapping，则使用对应变体）。

指定位宽类型的字面量在编译时检查范围——`let a: UInt16 = 70000` 是 E024 错误，不会静默折叠。

`min_value()` / `max_value()` 给出此类型的边界。准确接口见下方由机器维护的签名索引。

> 以下是原文生成索引的固定副本，本译文不是生成目标。

## 签名索引（35 个函数）

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint16.to_string(x: UInt16) -> String

// Smallest UInt16: 0.
// @since 0.38.0
uint16.min_value() -> UInt16

// Largest UInt16: 65535.
// @since 0.38.0
uint16.max_value() -> UInt16

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint16.compare(a: UInt16, b: UInt16) -> Int

// Lesser of a and b.
// @since unreleased
uint16.min(a: UInt16, b: UInt16) -> UInt16

// Greater of a and b.
// @since unreleased
uint16.max(a: UInt16, b: UInt16) -> UInt16

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint16.floor_div(a: UInt16, b: UInt16) -> UInt16

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint16.modulo(a: UInt16, b: UInt16) -> UInt16

// base raised to exp, wrapping to 16 bits like *; exp < 0 aborts.
// @since unreleased
uint16.pow(base: UInt16, exp: Int) -> UInt16

// True when x is a multiple of 2.
// @since unreleased
uint16.is_even(x: UInt16) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint16.is_odd(x: UInt16) -> Bool

// Bitwise AND.
// @since unreleased
uint16.band(a: UInt16, b: UInt16) -> UInt16

// Bitwise OR.
// @since unreleased
uint16.bor(a: UInt16, b: UInt16) -> UInt16

// Bitwise XOR.
// @since unreleased
uint16.bxor(a: UInt16, b: UInt16) -> UInt16

// Bitwise complement of all 16 bits.
// @since unreleased
uint16.bnot(x: UInt16) -> UInt16

// a shifted left by n bits; n is taken mod 16.
// @since unreleased
uint16.bshl(a: UInt16, n: Int) -> UInt16

// Zero-filling right shift; n is taken mod 16.
// @since unreleased
uint16.bshr(a: UInt16, n: Int) -> UInt16

// Zero bits above the highest set bit, counted in 16 bits; 16 for 0.
// @since unreleased
uint16.count_leading_zeros(x: UInt16) -> Int

// Zero bits below the lowest set bit; 16 for 0.
// @since unreleased
uint16.count_trailing_zeros(x: UInt16) -> Int

// Number of set bits among the 16.
// @since unreleased
uint16.count_ones(x: UInt16) -> Int

// The 2 bytes in reverse order (endian flip).
// @since unreleased
uint16.byte_swap(x: UInt16) -> UInt16

// a + b wrapped to 16 bits.
// @since unreleased
uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16

// a - b wrapped to 16 bits.
// @since unreleased
uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16

// a * b wrapped to 16 bits.
// @since unreleased
uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16

// The 16 bits of a rotated left by n; n is taken mod 16.
// @since unreleased
uint16.rotate_left(a: UInt16, n: Int) -> UInt16

// The 16 bits of a rotated right by n; n is taken mod 16.
// @since unreleased
uint16.rotate_right(a: UInt16, n: Int) -> UInt16

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint16.bit_length(x: UInt16) -> Int

// Decimal UInt16 from ASCII-trimmed s; err on junk, a sign of -, or above 65535.
// @since unreleased
uint16.parse(s: String) -> Result[UInt16, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint16.to_string_radix(x: UInt16, radix: Int) -> String

// UInt16 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 65535.
// @since unreleased
uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]

// x as UInt16, toward zero; NaN, an infinity or a value outside UInt16 aborts.
// @since unreleased
uint16.from(x: S) -> UInt16

// some(x as UInt16) when UInt16 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint16.from_checked(x: S) -> Option[UInt16]

// x as UInt16, toward zero, clamped to UInt16's range; NaN is 0.
// @since unreleased
uint16.from_saturating(x: S) -> UInt16

// The low 16 bits of integer x as UInt16 (two's complement).
// @since unreleased
uint16.from_wrapping(x: S) -> UInt16
```

## API 注释说明译文

- `uint16.to_string(x: UInt16) -> String` — 十进制数字，绝不带符号。
- `uint16.min_value() -> UInt16` — 最小 UInt16：0。
- `uint16.max_value() -> UInt16` — 最大 UInt16：65535。
- `uint16.compare(a: UInt16, b: UInt16) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `uint16.min(a: UInt16, b: UInt16) -> UInt16` — a 和 b 的较小者。
- `uint16.max(a: UInt16, b: UInt16) -> UInt16` — a 和 b 的较大者。
- `uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` — 商；对无符号类型与 a / b 相同；b == 0 时 abort。
- `uint16.modulo(a: UInt16, b: UInt16) -> UInt16` — 余数；对无符号类型与 a % b 相同；b == 0 时 abort。
- `uint16.pow(base: UInt16, exp: Int) -> UInt16` — base 的 exp 次幂，像 * 一样回绕到 16 位；exp < 0 时 abort。
- `uint16.is_even(x: UInt16) -> Bool` — x 是 2 的倍数时为 true。
- `uint16.is_odd(x: UInt16) -> Bool` — x 不是 2 的倍数时为 true。
- `uint16.band(a: UInt16, b: UInt16) -> UInt16` — 按位 AND。
- `uint16.bor(a: UInt16, b: UInt16) -> UInt16` — 按位 OR。
- `uint16.bxor(a: UInt16, b: UInt16) -> UInt16` — 按位 XOR。
- `uint16.bnot(x: UInt16) -> UInt16` — 全部 16 位的按位取反。
- `uint16.bshl(a: UInt16, n: Int) -> UInt16` — a 左移 n 位；n 对 16 取模。
- `uint16.bshr(a: UInt16, n: Int) -> UInt16` — 零填充右移；n 对 16 取模。
- `uint16.count_leading_zeros(x: UInt16) -> Int` — 在 16 位中最高置位位之上的零位数；0 时为 16。
- `uint16.count_trailing_zeros(x: UInt16) -> Int` — 最低置位位之下的零位数；0 时为 16。
- `uint16.count_ones(x: UInt16) -> Int` — 16 位中置位的数量。
- `uint16.byte_swap(x: UInt16) -> UInt16` — 反转 2 字节顺序（翻转字节序）。
- `uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` — a + b 回绕到 16 位。
- `uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` — a - b 回绕到 16 位。
- `uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` — a * b 回绕到 16 位。
- `uint16.rotate_left(a: UInt16, n: Int) -> UInt16` — 将 a 的 16 位向左旋转 n；n 对 16 取模。
- `uint16.rotate_right(a: UInt16, n: Int) -> UInt16` — 将 a 的 16 位向右旋转 n；n 对 16 取模。
- `uint16.bit_length(x: UInt16) -> Int` — 容纳 x 所需的位数；0 时为 0。
- `uint16.parse(s: String) -> Result[UInt16, String]` — 去除 s 两侧 ASCII 空白后解析十进制 UInt16；无效内容、- 符号或超过 65535 为 err。
- `uint16.to_string_radix(x: UInt16, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；无符号；radix 不在 2..36 时 abort。
- `uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` — 去除 s 两侧空白后按 radix 解析 UInt16，语法同 int.parse_radix；无效内容、- 符号或超过 65535 为 err。
- `uint16.from(x: S) -> UInt16` — x 向零舍入为 UInt16；NaN、无穷大或超出 UInt16 范围时 abort。
- `uint16.from_checked(x: S) -> Option[UInt16]` — UInt16 能精确容纳 x 时为 some(x as UInt16)，否则（小数部分、NaN、越界）为 none。
- `uint16.from_saturating(x: S) -> UInt16` — x 向零舍入为 UInt16，限制在 UInt16 范围；NaN 为 0。
- `uint16.from_wrapping(x: S) -> UInt16` — 将整数 x 的低 16 位作为 UInt16（二进制补码）。
