# int64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](int64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](int64.zh-CN.md)



> 原文：[docs/stdlib/int64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int64.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步。代码、签名、注释和版本注记保留原文，在外部附上 API 说明译文。翻译时未独立测量或验证实现。

> 原文转换函数族的引言泛指数值来源，但 from_wrapping 的表格及索引明确限定为整数来源。请按具体函数的条件理解；本译文保留此限制。

`Int64`（64 位有符号整数）：边界、算术、位操作、文本，以及转换到此类型。自动导入，无需 `import`。

`Int64` 与规范的 `Int` 共享表示，但属于不同类型。因此 `Int64` 与 `int32` 一样有自己的模块：该类型的边界、算术、文本和转换位于其名称所指的位置。

<a id="conversions"></a>

## 转换

转换按构造的目标类型命名（R-L0-40）：`int64.from(x)` 及其同族函数接受任意数值来源——`Int`、指定位宽整数、`Float` 或指定位宽浮点数。

```almd
let x: Int64 = int64.from(12)
let wide = int.from(x)                 // leaving Int64: the destination's `from`
let same = int64.from_wrapping(-1)     // -1: Int is already 64 bits wide
let fits = int64.from_checked(0.5)     // none: 0.5 has a fraction
let top = int64.from_saturating(float.infinity()) // 9223372036854775807: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `int64.from(x)` | x 向零舍入；NaN、无穷大或超出 Int64 范围的值会 abort |
| `int64.from_checked(x)` | Int64 能精确容纳 x 时为 `some`，否则 `none`（小数部分、NaN、越界） |
| `int64.from_saturating(x)` | x 向零舍入，限制在 Int64 范围；NaN 为 0 |
| `int64.from_wrapping(x)` | 仅整数来源：低 64 位，二进制补码 |

要从 `Int64` 转到其他类型，请调用目标类型的函数族——`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` 等。目标能容纳来源的所有值时，`from` 是正确选择；缩窄的目标通过名称选择 abort、`none`、限幅或回绕，绝不静默进行。

> **传给接收 `Int` 的函数前，先转为 `Int`。** 指定位宽的值不会隐式扩展：checker 拒绝不匹配的类型。两个目标都用 `int.from(x)` 显式请求转换；越界值会 abort（若预期策略是 checked、saturating 或 wrapping，则使用对应变体）。

`min_value()` / `max_value()` 给出此类型的边界。准确接口见下方由机器维护的签名索引。

> 以下是原文生成索引的固定副本，本译文不是生成目标。

<a id="signature-index-37-functions"></a>

## 签名索引（37 个函数）

```
// Decimal digits, with - when negative.
// @since 0.37.0 or earlier
int64.to_string(x: Int64) -> String

// Smallest Int64: -2^63.
// @since 0.38.0
int64.min_value() -> Int64

// Largest Int64: 2^63-1.
// @since 0.38.0
int64.max_value() -> Int64

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int64.compare(a: Int64, b: Int64) -> Int

// Lesser of a and b.
// @since unreleased
int64.min(a: Int64, b: Int64) -> Int64

// Greater of a and b.
// @since unreleased
int64.max(a: Int64, b: Int64) -> Int64

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int64.floor_div(a: Int64, b: Int64) -> Int64

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int64.modulo(a: Int64, b: Int64) -> Int64

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
int64.pow(base: Int64, exp: Int) -> Int64

// True when x is a multiple of 2.
// @since unreleased
int64.is_even(x: Int64) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int64.is_odd(x: Int64) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int64.abs(x: Int64) -> Int64

// -1, 0 or 1 by the sign of x.
// @since unreleased
int64.sign(x: Int64) -> Int

// Bitwise AND.
// @since unreleased
int64.band(a: Int64, b: Int64) -> Int64

// Bitwise OR.
// @since unreleased
int64.bor(a: Int64, b: Int64) -> Int64

// Bitwise XOR.
// @since unreleased
int64.bxor(a: Int64, b: Int64) -> Int64

// Bitwise complement of all 64 bits.
// @since unreleased
int64.bnot(x: Int64) -> Int64

// a shifted left by n bits; n is taken mod 64.
// @since unreleased
int64.bshl(a: Int64, n: Int) -> Int64

// Sign-filling right shift; n is taken mod 64.
// @since unreleased
int64.bshr(a: Int64, n: Int) -> Int64

// Zero bits above the highest set bit, counted in 64 bits; 64 for 0.
// @since unreleased
int64.count_leading_zeros(x: Int64) -> Int

// Zero bits below the lowest set bit; 64 for 0.
// @since unreleased
int64.count_trailing_zeros(x: Int64) -> Int

// Number of set bits among the 64.
// @since unreleased
int64.count_ones(x: Int64) -> Int

// The 8 bytes in reverse order (endian flip).
// @since unreleased
int64.byte_swap(x: Int64) -> Int64

// a + b wrapped to 64 bits.
// @since unreleased
int64.wrap_add(a: Int64, b: Int64) -> Int64

// a - b wrapped to 64 bits.
// @since unreleased
int64.wrap_sub(a: Int64, b: Int64) -> Int64

// a * b wrapped to 64 bits.
// @since unreleased
int64.wrap_mul(a: Int64, b: Int64) -> Int64

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since unreleased
int64.rotate_left(a: Int64, n: Int) -> Int64

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since unreleased
int64.rotate_right(a: Int64, n: Int) -> Int64

// Bits needed to hold x; 0 for 0, 64 if negative.
// @since unreleased
int64.bit_length(x: Int64) -> Int

// Decimal Int64 from ASCII-trimmed s; err on junk or overflow.
// @since unreleased
int64.parse(s: String) -> Result[Int64, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int64.to_string_radix(x: Int64, radix: Int) -> String

// Int64 from trimmed s in radix, the grammar of int.parse_radix; err on junk or overflow.
// @since unreleased
int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]

// x as Int64, toward zero; NaN, an infinity or a value outside Int64 aborts.
// @since unreleased
int64.from(x: S) -> Int64

// some(x as Int64) when Int64 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int64.from_checked(x: S) -> Option[Int64]

// x as Int64, toward zero, clamped to Int64's range; NaN is 0.
// @since unreleased
int64.from_saturating(x: S) -> Int64

// The low 64 bits of integer x as Int64 (two's complement).
// @since unreleased
int64.from_wrapping(x: S) -> Int64
```

## API 注释说明译文

- `int64.to_string(x: Int64) -> String` — 十进制数字，负数带 -。
- `int64.min_value() -> Int64` — 最小 Int64：-2^63。
- `int64.max_value() -> Int64` — 最大 Int64：2^63-1。
- `int64.compare(a: Int64, b: Int64) -> Int` — a 小于、等于、大于 b 时分别为 -1、0、1。
- `int64.min(a: Int64, b: Int64) -> Int64` — a 和 b 的较小者。
- `int64.max(a: Int64, b: Int64) -> Int64` — a 和 b 的较大者。
- `int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` — 将 x 限制在 lo..hi；lo > hi 时 abort。
- `int64.floor_div(a: Int64, b: Int64) -> Int64` — 商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时 abort。
- `int64.modulo(a: Int64, b: Int64) -> Int64` — 余数与 b 同号，满足 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时 abort。
- `int64.pow(base: Int64, exp: Int) -> Int64` — base 的 exp 次幂，像 * 一样回绕到 64 位；exp < 0 时 abort。
- `int64.is_even(x: Int64) -> Bool` — x 是 2 的倍数时为 true。
- `int64.is_odd(x: Int64) -> Bool` — x 不是 2 的倍数时为 true。
- `int64.abs(x: Int64) -> Int64` — x 的大小；min_value 回绕为自身。
- `int64.sign(x: Int64) -> Int` — 按 x 的符号返回 -1、0、1。
- `int64.band(a: Int64, b: Int64) -> Int64` — 按位 AND。
- `int64.bor(a: Int64, b: Int64) -> Int64` — 按位 OR。
- `int64.bxor(a: Int64, b: Int64) -> Int64` — 按位 XOR。
- `int64.bnot(x: Int64) -> Int64` — 全部 64 位的按位取反。
- `int64.bshl(a: Int64, n: Int) -> Int64` — a 左移 n 位；n 对 64 取模。
- `int64.bshr(a: Int64, n: Int) -> Int64` — 填充符号位右移；n 对 64 取模。
- `int64.count_leading_zeros(x: Int64) -> Int` — 在 64 位中最高置位位之上的零位数；0 时为 64。
- `int64.count_trailing_zeros(x: Int64) -> Int` — 最低置位位之下的零位数；0 时为 64。
- `int64.count_ones(x: Int64) -> Int` — 64 位中置位的数量。
- `int64.byte_swap(x: Int64) -> Int64` — 反转 8 字节顺序（翻转字节序）。
- `int64.wrap_add(a: Int64, b: Int64) -> Int64` — a + b 回绕到 64 位。
- `int64.wrap_sub(a: Int64, b: Int64) -> Int64` — a - b 回绕到 64 位。
- `int64.wrap_mul(a: Int64, b: Int64) -> Int64` — a * b 回绕到 64 位。
- `int64.rotate_left(a: Int64, n: Int) -> Int64` — 将 a 的 64 位向左旋转 n；n 对 64 取模。
- `int64.rotate_right(a: Int64, n: Int) -> Int64` — 将 a 的 64 位向右旋转 n；n 对 64 取模。
- `int64.bit_length(x: Int64) -> Int` — 容纳 x 所需的位数；0 时为 0，负数为 64。
- `int64.parse(s: String) -> Result[Int64, String]` — 去除 s 两侧 ASCII 空白后解析十进制 Int64；无效内容或溢出为 err。
- `int64.to_string_radix(x: Int64, radix: Int) -> String` — 用 radix 进制的小写数字表示 x；负数带 -；radix 不在 2..36 时 abort。
- `int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` — 去除 s 两侧空白后按 radix 解析 Int64，语法同 int.parse_radix；无效内容或溢出为 err。
- `int64.from(x: S) -> Int64` — x 向零舍入为 Int64；NaN、无穷大或超出 Int64 范围时 abort。
- `int64.from_checked(x: S) -> Option[Int64]` — Int64 能精确容纳 x 时为 some(x as Int64)，否则（小数部分、NaN、越界）为 none。
- `int64.from_saturating(x: S) -> Int64` — x 向零舍入为 Int64，限制在 Int64 范围；NaN 为 0。
- `int64.from_wrapping(x: S) -> Int64` — 将整数 x 的低 64 位作为 Int64（二进制补码）。
