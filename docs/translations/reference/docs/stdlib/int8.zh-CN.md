# int8

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int8.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](int8.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](int8.zh-CN.md)



> 原文：[int8.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int8.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

`Int8`（8位有符号, -128 … 127）：边界、算术、位运算、文本以及到该类型的转换。自动导入，无需 `import`。

<a id="conversions"></a>

## 转换

转换以其构造的类型命名（R-L0-40）：`int8.from(x)` 及其同族函数接受任意数值输入，包括 `Int`、有指定位宽的整数、`Float` 或有指定位宽的浮点数。

```almd
let x: Int8 = int8.from(12)
let wide = int.from(x)                 // leaving Int8: the destination's `from`
let low = int8.from_wrapping(172)      // -84: the low 8 bits, two's complement
let fits = int8.from_checked(128)      // none: 128 is not an Int8
let top = int8.from_saturating(float.infinity()) // 127: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `int8.from(x)` | x 向零舍入；NaN、无穷大或超出 Int8 范围的值会中止 |
| `int8.from_checked(x)` | Int8 能精确容纳 x 时为 `some`，否则（小数、NaN、越界）为 `none` |
| `int8.from_saturating(x)` | x 向零舍入，限制在 Int8 范围内；NaN 为0 |
| `int8.from_wrapping(x)` | 仅整数输入：低8位，二进制补码 |

要从 `Int8` 转出，请调用目标类型的转换族，例如 `int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)`。只要目标能容纳源类型的全部值，就适合调用 `from`；缩窄范围时按名称选择中止、`none`、限制范围或回绕，绝不默默处理。

> **传给接受 `Int` 的函数前，先转换为 `Int`。** 有指定位宽的值不会隐式拓宽，检查器会拒绝不匹配的类型。在两个目标上均使用 `int.from(x)` 显式请求转换；越界值会中止（若希望采用相应策略，请用 checked、saturating 或 wrapping 版本）。

有指定位宽的类型的字面量在编译时检查范围：`let a: Int8 = 200` 会产生 E024 错误，而不是静默折叠。

`min_value()` / `max_value()` 给出该类型的边界。确切的 API 列表见下方保留的原文机器管理签名索引（本译文是固定快照，不由机器维护）。

<a id="signature-index-37-functions"></a>

## 原文固定快照 API 索引（37个函数）

```
// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int8.to_string(x: Int8) -> String

// Smallest Int8: -128.
// @since 0.38.0
int8.min_value() -> Int8

// Largest Int8: 127.
// @since 0.38.0
int8.max_value() -> Int8

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int8.compare(a: Int8, b: Int8) -> Int

// Lesser of a and b.
// @since unreleased
int8.min(a: Int8, b: Int8) -> Int8

// Greater of a and b.
// @since unreleased
int8.max(a: Int8, b: Int8) -> Int8

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int8.floor_div(a: Int8, b: Int8) -> Int8

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int8.modulo(a: Int8, b: Int8) -> Int8

// base raised to exp, wrapping to 8 bits like *; exp < 0 aborts.
// @since unreleased
int8.pow(base: Int8, exp: Int) -> Int8

// True when x is a multiple of 2.
// @since unreleased
int8.is_even(x: Int8) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int8.is_odd(x: Int8) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int8.abs(x: Int8) -> Int8

// -1, 0 or 1 by the sign of x.
// @since unreleased
int8.sign(x: Int8) -> Int

// Bitwise AND.
// @since unreleased
int8.band(a: Int8, b: Int8) -> Int8

// Bitwise OR.
// @since unreleased
int8.bor(a: Int8, b: Int8) -> Int8

// Bitwise XOR.
// @since unreleased
int8.bxor(a: Int8, b: Int8) -> Int8

// Bitwise complement of all 8 bits.
// @since unreleased
int8.bnot(x: Int8) -> Int8

// a shifted left by n bits; n is taken mod 8.
// @since unreleased
int8.bshl(a: Int8, n: Int) -> Int8

// Sign-filling right shift; n is taken mod 8.
// @since unreleased
int8.bshr(a: Int8, n: Int) -> Int8

// Zero bits above the highest set bit, counted in 8 bits; 8 for 0.
// @since unreleased
int8.count_leading_zeros(x: Int8) -> Int

// Zero bits below the lowest set bit; 8 for 0.
// @since unreleased
int8.count_trailing_zeros(x: Int8) -> Int

// Number of set bits among the 8.
// @since unreleased
int8.count_ones(x: Int8) -> Int

// The bytes in reverse order; one byte, so x itself.
// @since unreleased
int8.byte_swap(x: Int8) -> Int8

// a + b wrapped to 8 bits.
// @since unreleased
int8.wrap_add(a: Int8, b: Int8) -> Int8

// a - b wrapped to 8 bits.
// @since unreleased
int8.wrap_sub(a: Int8, b: Int8) -> Int8

// a * b wrapped to 8 bits.
// @since unreleased
int8.wrap_mul(a: Int8, b: Int8) -> Int8

// The 8 bits of a rotated left by n; n is taken mod 8.
// @since unreleased
int8.rotate_left(a: Int8, n: Int) -> Int8

// The 8 bits of a rotated right by n; n is taken mod 8.
// @since unreleased
int8.rotate_right(a: Int8, n: Int) -> Int8

// Bits needed to hold x; 0 for 0, 8 if negative.
// @since unreleased
int8.bit_length(x: Int8) -> Int

// Decimal Int8 from ASCII-trimmed s; err on junk or outside -128..127.
// @since unreleased
int8.parse(s: String) -> Result[Int8, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int8.to_string_radix(x: Int8, radix: Int) -> String

// Int8 from trimmed s in radix, the grammar of int.parse_radix; err on junk or outside -128..127.
// @since unreleased
int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]

// x as Int8, toward zero; NaN, an infinity or a value outside Int8 aborts.
// @since unreleased
int8.from(x: S) -> Int8

// some(x as Int8) when Int8 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int8.from_checked(x: S) -> Option[Int8]

// x as Int8, toward zero, clamped to Int8's range; NaN is 0.
// @since unreleased
int8.from_saturating(x: S) -> Int8

// The low 8 bits of integer x as Int8 (two's complement).
// @since unreleased
int8.from_wrapping(x: S) -> Int8
```

## API 说明译文（按上述签名顺序）

- `int8.to_string(x: Int8) -> String`：十进制数字，负数带 -。
- `int8.min_value() -> Int8`：最小 Int8：-128。
- `int8.max_value() -> Int8`：最大 Int8：127。
- `int8.compare(a: Int8, b: Int8) -> Int`：a 小于、等于、大于 b 时分别为 -1、0、1。
- `int8.min(a: Int8, b: Int8) -> Int8`：a 与 b 中较小者。
- `int8.max(a: Int8, b: Int8) -> Int8`：a 与 b 中较大者。
- `int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8`：将 x 限制在 lo..hi；lo > hi 时中止。
- `int8.floor_div(a: Int8, b: Int8) -> Int8`：商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int8.modulo(a: Int8, b: Int8) -> Int8`：余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int8.pow(base: Int8, exp: Int) -> Int8`：base 的 exp 次幂，像 * 一样回绕到8位；exp < 0 时中止。
- `int8.is_even(x: Int8) -> Bool`：x 是2的倍数时为 true。
- `int8.is_odd(x: Int8) -> Bool`：x 不是2的倍数时为 true。
- `int8.abs(x: Int8) -> Int8`：x 的大小；min_value 回绕为自身。
- `int8.sign(x: Int8) -> Int`：按 x 的符号返回 -1、0或1。
- `int8.band(a: Int8, b: Int8) -> Int8`：按位 AND。
- `int8.bor(a: Int8, b: Int8) -> Int8`：按位 OR。
- `int8.bxor(a: Int8, b: Int8) -> Int8`：按位 XOR。
- `int8.bnot(x: Int8) -> Int8`：对全部8位按位取反。
- `int8.bshl(a: Int8, n: Int) -> Int8`：将 a 左移 n 位；n 对8取模。
- `int8.bshr(a: Int8, n: Int) -> Int8`：填充符号位的右移；n 对8取模。
- `int8.count_leading_zeros(x: Int8) -> Int`：在8位内计数的最高置位之上的零位数；0时为8。
- `int8.count_trailing_zeros(x: Int8) -> Int`：最低置位之下的零位数；0时为8。
- `int8.count_ones(x: Int8) -> Int`：8位中置位的位数。
- `int8.byte_swap(x: Int8) -> Int8`：反转字节顺序；只有1字节，因此为 x 本身。
- `int8.wrap_add(a: Int8, b: Int8) -> Int8`：a + b 回绕到8位。
- `int8.wrap_sub(a: Int8, b: Int8) -> Int8`：a - b 回绕到8位。
- `int8.wrap_mul(a: Int8, b: Int8) -> Int8`：a * b 回绕到8位。
- `int8.rotate_left(a: Int8, n: Int) -> Int8`：将 a 的8位向左旋转 n 位；n 对8取模。
- `int8.rotate_right(a: Int8, n: Int) -> Int8`：将 a 的8位向右旋转 n 位；n 对8取模。
- `int8.bit_length(x: Int8) -> Int`：容纳 x 所需的位数；0为0，负数为8。
- `int8.parse(s: String) -> Result[Int8, String]`：去除 s 两侧 ASCII 空白后解析为十进制 Int8；无效内容或超出 -128..127 返回 err。
- `int8.to_string_radix(x: Int8, radix: Int) -> String`：以 radix 进制的小写数字表示 x，负数带 -；radix 不在2..36时中止。
- `int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]`：将去除两侧空白的 s 按 radix 解析为 Int8，语法同 int.parse_radix；无效内容或超出 -128..127 返回 err。
- `int8.from(x: S) -> Int8`：将 x 向零舍入为 Int8；NaN、无穷大或超出 Int8 范围的值会中止。
- `int8.from_checked(x: S) -> Option[Int8]`：若 Int8 能精确容纳 x，则 some(x as Int8)，否则（小数、NaN、越界）为 none。
- `int8.from_saturating(x: S) -> Int8`：将 x 向零舍入为 Int8，并限制在 Int8 范围内；NaN 为0。
- `int8.from_wrapping(x: S) -> Int8`：将整数 x 的低8位作为 Int8（二进制补码）。
