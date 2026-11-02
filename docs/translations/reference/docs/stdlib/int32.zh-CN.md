# int32

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int32.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](int32.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](int32.zh-CN.md)



> 原文：[int32.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int32.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

`Int32`（32位有符号）：边界、算术、位运算、文本以及到该类型的转换。自动导入，无需 `import`。

## 转换

转换以其构造的类型命名（R-L0-40）：`int32.from(x)` 及其同族函数接受任意数值输入，包括 `Int`、有指定位宽的整数、`Float` 或有指定位宽的浮点数。

```almd
let x: Int32 = int32.from(12)
let wide = int.from(x)                 // leaving Int32: the destination's `from`
let low = int32.from_wrapping(2147483692)// -2147483604: the low 32 bits, two's complement
let fits = int32.from_checked(2147483648)// none: 2147483648 is not an Int32
let top = int32.from_saturating(float.infinity()) // 2147483647: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `int32.from(x)` | x 向零舍入；NaN、无穷大或超出 Int32 范围的值会中止 |
| `int32.from_checked(x)` | Int32 能精确容纳 x 时为 `some`，否则（小数、NaN、越界）为 `none` |
| `int32.from_saturating(x)` | x 向零舍入，限制在 Int32 范围内；NaN 为0 |
| `int32.from_wrapping(x)` | 仅整数输入：低32位，二进制补码 |

要从 `Int32` 转出，请调用目标类型的转换族，例如 `int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)`。只要目标能容纳源类型的全部值，就适合调用 `from`；缩窄范围时按名称选择中止、`none`、限制范围或回绕，绝不默默处理。

> **传给接受 `Int` 的函数前，先转换为 `Int`。** 有指定位宽的值不会隐式拓宽，检查器会拒绝不匹配的类型。在两个目标上均使用 `int.from(x)` 显式请求转换；越界值会中止（若希望采用相应策略，请用 checked、saturating 或 wrapping 版本）。

有指定位宽的类型的字面量在编译时检查范围：`let a: Int32 = 3000000000` 会产生 E024 错误，而不是静默折叠。

`min_value()` / `max_value()` 给出该类型的边界。确切的 API 列表见下方保留的原文机器管理签名索引（本译文是固定快照，不由机器维护）。

## 原文固定快照 API 索引（37个函数）

```
// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int32.to_string(x: Int32) -> String

// Smallest Int32: -2147483648.
// @since 0.38.0
int32.min_value() -> Int32

// Largest Int32: 2147483647.
// @since 0.38.0
int32.max_value() -> Int32

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int32.compare(a: Int32, b: Int32) -> Int

// Lesser of a and b.
// @since unreleased
int32.min(a: Int32, b: Int32) -> Int32

// Greater of a and b.
// @since unreleased
int32.max(a: Int32, b: Int32) -> Int32

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int32.floor_div(a: Int32, b: Int32) -> Int32

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int32.modulo(a: Int32, b: Int32) -> Int32

// base raised to exp, wrapping to 32 bits like *; exp < 0 aborts.
// @since unreleased
int32.pow(base: Int32, exp: Int) -> Int32

// True when x is a multiple of 2.
// @since unreleased
int32.is_even(x: Int32) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int32.is_odd(x: Int32) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int32.abs(x: Int32) -> Int32

// -1, 0 or 1 by the sign of x.
// @since unreleased
int32.sign(x: Int32) -> Int

// Bitwise AND.
// @since unreleased
int32.band(a: Int32, b: Int32) -> Int32

// Bitwise OR.
// @since unreleased
int32.bor(a: Int32, b: Int32) -> Int32

// Bitwise XOR.
// @since unreleased
int32.bxor(a: Int32, b: Int32) -> Int32

// Bitwise complement of all 32 bits.
// @since unreleased
int32.bnot(x: Int32) -> Int32

// a shifted left by n bits; n is taken mod 32.
// @since unreleased
int32.bshl(a: Int32, n: Int) -> Int32

// Sign-filling right shift; n is taken mod 32.
// @since unreleased
int32.bshr(a: Int32, n: Int) -> Int32

// Zero bits above the highest set bit, counted in 32 bits; 32 for 0.
// @since unreleased
int32.count_leading_zeros(x: Int32) -> Int

// Zero bits below the lowest set bit; 32 for 0.
// @since unreleased
int32.count_trailing_zeros(x: Int32) -> Int

// Number of set bits among the 32.
// @since unreleased
int32.count_ones(x: Int32) -> Int

// The 4 bytes in reverse order (endian flip).
// @since unreleased
int32.byte_swap(x: Int32) -> Int32

// a + b wrapped to 32 bits.
// @since unreleased
int32.wrap_add(a: Int32, b: Int32) -> Int32

// a - b wrapped to 32 bits.
// @since unreleased
int32.wrap_sub(a: Int32, b: Int32) -> Int32

// a * b wrapped to 32 bits.
// @since unreleased
int32.wrap_mul(a: Int32, b: Int32) -> Int32

// The 32 bits of a rotated left by n; n is taken mod 32.
// @since unreleased
int32.rotate_left(a: Int32, n: Int) -> Int32

// The 32 bits of a rotated right by n; n is taken mod 32.
// @since unreleased
int32.rotate_right(a: Int32, n: Int) -> Int32

// Bits needed to hold x; 0 for 0, 32 if negative.
// @since unreleased
int32.bit_length(x: Int32) -> Int

// Decimal Int32 from ASCII-trimmed s; err on junk or outside -2147483648..2147483647.
// @since unreleased
int32.parse(s: String) -> Result[Int32, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int32.to_string_radix(x: Int32, radix: Int) -> String

// Int32 from trimmed s in radix, the grammar of int.parse_radix; err on junk or outside -2147483648..2147483647.
// @since unreleased
int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]

// x as Int32, toward zero; NaN, an infinity or a value outside Int32 aborts.
// @since unreleased
int32.from(x: S) -> Int32

// some(x as Int32) when Int32 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int32.from_checked(x: S) -> Option[Int32]

// x as Int32, toward zero, clamped to Int32's range; NaN is 0.
// @since unreleased
int32.from_saturating(x: S) -> Int32

// The low 32 bits of integer x as Int32 (two's complement).
// @since unreleased
int32.from_wrapping(x: S) -> Int32
```

## API 说明译文（按上述签名顺序）

- `int32.to_string(x: Int32) -> String`：十进制数字，负数带 -。
- `int32.min_value() -> Int32`：最小 Int32：-2147483648。
- `int32.max_value() -> Int32`：最大 Int32：2147483647。
- `int32.compare(a: Int32, b: Int32) -> Int`：a 小于、等于、大于 b 时分别为 -1、0、1。
- `int32.min(a: Int32, b: Int32) -> Int32`：a 与 b 中较小者。
- `int32.max(a: Int32, b: Int32) -> Int32`：a 与 b 中较大者。
- `int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32`：将 x 限制在 lo..hi；lo > hi 时中止。
- `int32.floor_div(a: Int32, b: Int32) -> Int32`：商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int32.modulo(a: Int32, b: Int32) -> Int32`：余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int32.pow(base: Int32, exp: Int) -> Int32`：base 的 exp 次幂，像 * 一样回绕到32位；exp < 0 时中止。
- `int32.is_even(x: Int32) -> Bool`：x 是2的倍数时为 true。
- `int32.is_odd(x: Int32) -> Bool`：x 不是2的倍数时为 true。
- `int32.abs(x: Int32) -> Int32`：x 的大小；min_value 回绕为自身。
- `int32.sign(x: Int32) -> Int`：按 x 的符号返回 -1、0或1。
- `int32.band(a: Int32, b: Int32) -> Int32`：按位 AND。
- `int32.bor(a: Int32, b: Int32) -> Int32`：按位 OR。
- `int32.bxor(a: Int32, b: Int32) -> Int32`：按位 XOR。
- `int32.bnot(x: Int32) -> Int32`：对全部32位按位取反。
- `int32.bshl(a: Int32, n: Int) -> Int32`：将 a 左移 n 位；n 对32取模。
- `int32.bshr(a: Int32, n: Int) -> Int32`：填充符号位的右移；n 对32取模。
- `int32.count_leading_zeros(x: Int32) -> Int`：在32位内计数的最高置位之上的零位数；0时为32。
- `int32.count_trailing_zeros(x: Int32) -> Int`：最低置位之下的零位数；0时为32。
- `int32.count_ones(x: Int32) -> Int`：32位中置位的位数。
- `int32.byte_swap(x: Int32) -> Int32`：反转4字节的顺序（翻转字节序）。
- `int32.wrap_add(a: Int32, b: Int32) -> Int32`：a + b 回绕到32位。
- `int32.wrap_sub(a: Int32, b: Int32) -> Int32`：a - b 回绕到32位。
- `int32.wrap_mul(a: Int32, b: Int32) -> Int32`：a * b 回绕到32位。
- `int32.rotate_left(a: Int32, n: Int) -> Int32`：将 a 的32位向左旋转 n 位；n 对32取模。
- `int32.rotate_right(a: Int32, n: Int) -> Int32`：将 a 的32位向右旋转 n 位；n 对32取模。
- `int32.bit_length(x: Int32) -> Int`：容纳 x 所需的位数；0为0，负数为32。
- `int32.parse(s: String) -> Result[Int32, String]`：去除 s 两侧 ASCII 空白后解析为十进制 Int32；无效内容或超出 -2147483648..2147483647 返回 err。
- `int32.to_string_radix(x: Int32, radix: Int) -> String`：以 radix 进制的小写数字表示 x，负数带 -；radix 不在2..36时中止。
- `int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]`：将去除两侧空白的 s 按 radix 解析为 Int32，语法同 int.parse_radix；无效内容或超出 -2147483648..2147483647 返回 err。
- `int32.from(x: S) -> Int32`：将 x 向零舍入为 Int32；NaN、无穷大或超出 Int32 范围的值会中止。
- `int32.from_checked(x: S) -> Option[Int32]`：若 Int32 能精确容纳 x，则 some(x as Int32)，否则（小数、NaN、越界）为 none。
- `int32.from_saturating(x: S) -> Int32`：将 x 向零舍入为 Int32，并限制在 Int32 范围内；NaN 为0。
- `int32.from_wrapping(x: S) -> Int32`：将整数 x 的低32位作为 Int32（二进制补码）。
