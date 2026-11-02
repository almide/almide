# int16

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int16.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](int16.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](int16.zh-CN.md)



> 原文：[int16.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int16.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

`Int16`（16位有符号, -32,768 … 32,767）：边界、算术、位运算、文本以及到该类型的转换。自动导入，无需 `import`。

<a id="conversions"></a>

## 转换

转换以其构造的类型命名（R-L0-40）：`int16.from(x)` 及其同族函数接受任意数值输入，包括 `Int`、有指定位宽的整数、`Float` 或有指定位宽的浮点数。

```almd
let x: Int16 = int16.from(12)
let wide = int.from(x)                 // leaving Int16: the destination's `from`
let low = int16.from_wrapping(32812)   // -32724: the low 16 bits, two's complement
let fits = int16.from_checked(32768)   // none: 32768 is not an Int16
let top = int16.from_saturating(float.infinity()) // 32767: clamped to the range
let text = x.to_string()
```

| 函数 | 行为 |
|---|---|
| `int16.from(x)` | x 向零舍入；NaN、无穷大或超出 Int16 范围的值会中止 |
| `int16.from_checked(x)` | Int16 能精确容纳 x 时为 `some`，否则（小数、NaN、越界）为 `none` |
| `int16.from_saturating(x)` | x 向零舍入，限制在 Int16 范围内；NaN 为0 |
| `int16.from_wrapping(x)` | 仅整数输入：低16位，二进制补码 |

要从 `Int16` 转出，请调用目标类型的转换族，例如 `int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)`。只要目标能容纳源类型的全部值，就适合调用 `from`；缩窄范围时按名称选择中止、`none`、限制范围或回绕，绝不默默处理。

> **传给接受 `Int` 的函数前，先转换为 `Int`。** 有指定位宽的值不会隐式拓宽，检查器会拒绝不匹配的类型。在两个目标上均使用 `int.from(x)` 显式请求转换；越界值会中止（若希望采用相应策略，请用 checked、saturating 或 wrapping 版本）。

有指定位宽的类型的字面量在编译时检查范围：`let a: Int16 = 40000` 会产生 E024 错误，而不是静默折叠。

`min_value()` / `max_value()` 给出该类型的边界。确切的 API 列表见下方保留的原文机器管理签名索引（本译文是固定快照，不由机器维护）。

> 原文在从 Int16 转出的例子中也列出了 int16.from_wrapping。这里保留这个指向同一类型的例子，没有将它替换成其他目标类型。

<a id="signature-index-37-functions"></a>

## 原文固定快照 API 索引（37个函数）

```
// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int16.to_string(x: Int16) -> String

// Smallest Int16: -32768.
// @since 0.38.0
int16.min_value() -> Int16

// Largest Int16: 32767.
// @since 0.38.0
int16.max_value() -> Int16

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int16.compare(a: Int16, b: Int16) -> Int

// Lesser of a and b.
// @since unreleased
int16.min(a: Int16, b: Int16) -> Int16

// Greater of a and b.
// @since unreleased
int16.max(a: Int16, b: Int16) -> Int16

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int16.floor_div(a: Int16, b: Int16) -> Int16

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int16.modulo(a: Int16, b: Int16) -> Int16

// base raised to exp, wrapping to 16 bits like *; exp < 0 aborts.
// @since unreleased
int16.pow(base: Int16, exp: Int) -> Int16

// True when x is a multiple of 2.
// @since unreleased
int16.is_even(x: Int16) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int16.is_odd(x: Int16) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int16.abs(x: Int16) -> Int16

// -1, 0 or 1 by the sign of x.
// @since unreleased
int16.sign(x: Int16) -> Int

// Bitwise AND.
// @since unreleased
int16.band(a: Int16, b: Int16) -> Int16

// Bitwise OR.
// @since unreleased
int16.bor(a: Int16, b: Int16) -> Int16

// Bitwise XOR.
// @since unreleased
int16.bxor(a: Int16, b: Int16) -> Int16

// Bitwise complement of all 16 bits.
// @since unreleased
int16.bnot(x: Int16) -> Int16

// a shifted left by n bits; n is taken mod 16.
// @since unreleased
int16.bshl(a: Int16, n: Int) -> Int16

// Sign-filling right shift; n is taken mod 16.
// @since unreleased
int16.bshr(a: Int16, n: Int) -> Int16

// Zero bits above the highest set bit, counted in 16 bits; 16 for 0.
// @since unreleased
int16.count_leading_zeros(x: Int16) -> Int

// Zero bits below the lowest set bit; 16 for 0.
// @since unreleased
int16.count_trailing_zeros(x: Int16) -> Int

// Number of set bits among the 16.
// @since unreleased
int16.count_ones(x: Int16) -> Int

// The 2 bytes in reverse order (endian flip).
// @since unreleased
int16.byte_swap(x: Int16) -> Int16

// a + b wrapped to 16 bits.
// @since unreleased
int16.wrap_add(a: Int16, b: Int16) -> Int16

// a - b wrapped to 16 bits.
// @since unreleased
int16.wrap_sub(a: Int16, b: Int16) -> Int16

// a * b wrapped to 16 bits.
// @since unreleased
int16.wrap_mul(a: Int16, b: Int16) -> Int16

// The 16 bits of a rotated left by n; n is taken mod 16.
// @since unreleased
int16.rotate_left(a: Int16, n: Int) -> Int16

// The 16 bits of a rotated right by n; n is taken mod 16.
// @since unreleased
int16.rotate_right(a: Int16, n: Int) -> Int16

// Bits needed to hold x; 0 for 0, 16 if negative.
// @since unreleased
int16.bit_length(x: Int16) -> Int

// Decimal Int16 from ASCII-trimmed s; err on junk or outside -32768..32767.
// @since unreleased
int16.parse(s: String) -> Result[Int16, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int16.to_string_radix(x: Int16, radix: Int) -> String

// Int16 from trimmed s in radix, the grammar of int.parse_radix; err on junk or outside -32768..32767.
// @since unreleased
int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]

// x as Int16, toward zero; NaN, an infinity or a value outside Int16 aborts.
// @since unreleased
int16.from(x: S) -> Int16

// some(x as Int16) when Int16 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int16.from_checked(x: S) -> Option[Int16]

// x as Int16, toward zero, clamped to Int16's range; NaN is 0.
// @since unreleased
int16.from_saturating(x: S) -> Int16

// The low 16 bits of integer x as Int16 (two's complement).
// @since unreleased
int16.from_wrapping(x: S) -> Int16
```

## API 说明译文（按上述签名顺序）

- `int16.to_string(x: Int16) -> String`：十进制数字，负数带 -。
- `int16.min_value() -> Int16`：最小 Int16：-32768。
- `int16.max_value() -> Int16`：最大 Int16：32767。
- `int16.compare(a: Int16, b: Int16) -> Int`：a 小于、等于、大于 b 时分别为 -1、0、1。
- `int16.min(a: Int16, b: Int16) -> Int16`：a 与 b 中较小者。
- `int16.max(a: Int16, b: Int16) -> Int16`：a 与 b 中较大者。
- `int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16`：将 x 限制在 lo..hi；lo > hi 时中止。
- `int16.floor_div(a: Int16, b: Int16) -> Int16`：商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int16.modulo(a: Int16, b: Int16) -> Int16`：余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int16.pow(base: Int16, exp: Int) -> Int16`：base 的 exp 次幂，像 * 一样回绕到16位；exp < 0 时中止。
- `int16.is_even(x: Int16) -> Bool`：x 是2的倍数时为 true。
- `int16.is_odd(x: Int16) -> Bool`：x 不是2的倍数时为 true。
- `int16.abs(x: Int16) -> Int16`：x 的大小；min_value 回绕为自身。
- `int16.sign(x: Int16) -> Int`：按 x 的符号返回 -1、0或1。
- `int16.band(a: Int16, b: Int16) -> Int16`：按位 AND。
- `int16.bor(a: Int16, b: Int16) -> Int16`：按位 OR。
- `int16.bxor(a: Int16, b: Int16) -> Int16`：按位 XOR。
- `int16.bnot(x: Int16) -> Int16`：对全部16位按位取反。
- `int16.bshl(a: Int16, n: Int) -> Int16`：将 a 左移 n 位；n 对16取模。
- `int16.bshr(a: Int16, n: Int) -> Int16`：填充符号位的右移；n 对16取模。
- `int16.count_leading_zeros(x: Int16) -> Int`：在16位内计数的最高置位之上的零位数；0时为16。
- `int16.count_trailing_zeros(x: Int16) -> Int`：最低置位之下的零位数；0时为16。
- `int16.count_ones(x: Int16) -> Int`：16位中置位的位数。
- `int16.byte_swap(x: Int16) -> Int16`：反转2字节的顺序（翻转字节序）。
- `int16.wrap_add(a: Int16, b: Int16) -> Int16`：a + b 回绕到16位。
- `int16.wrap_sub(a: Int16, b: Int16) -> Int16`：a - b 回绕到16位。
- `int16.wrap_mul(a: Int16, b: Int16) -> Int16`：a * b 回绕到16位。
- `int16.rotate_left(a: Int16, n: Int) -> Int16`：将 a 的16位向左旋转 n 位；n 对16取模。
- `int16.rotate_right(a: Int16, n: Int) -> Int16`：将 a 的16位向右旋转 n 位；n 对16取模。
- `int16.bit_length(x: Int16) -> Int`：容纳 x 所需的位数；0为0，负数为16。
- `int16.parse(s: String) -> Result[Int16, String]`：去除 s 两侧 ASCII 空白后解析为十进制 Int16；无效内容或超出 -32768..32767 返回 err。
- `int16.to_string_radix(x: Int16, radix: Int) -> String`：以 radix 进制的小写数字表示 x，负数带 -；radix 不在2..36时中止。
- `int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]`：将去除两侧空白的 s 按 radix 解析为 Int16，语法同 int.parse_radix；无效内容或超出 -32768..32767 返回 err。
- `int16.from(x: S) -> Int16`：将 x 向零舍入为 Int16；NaN、无穷大或超出 Int16 范围的值会中止。
- `int16.from_checked(x: S) -> Option[Int16]`：若 Int16 能精确容纳 x，则 some(x as Int16)，否则（小数、NaN、越界）为 none。
- `int16.from_saturating(x: S) -> Int16`：将 x 向零舍入为 Int16，并限制在 Int16 范围内；NaN 为0。
- `int16.from_wrapping(x: S) -> Int16`：将整数 x 的低16位作为 Int16（二进制补码）。
