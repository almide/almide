# int

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](int.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](int.zh-CN.md)



> 原文：[int.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

> 除正文中对绝对值、范围限制的简述外，API 索引还注明 min_value 的回绕和上下界逆序时中止。这里保留了原文及其限制。

整数运算与位运算。自动导入。

### `int.to_string(x: Int) -> String`

将整数转换为十进制字符串表示。

```almd run
fn main() -> Unit = {
  println(int.to_string(42))
}
```
```output
42
```

### `int.to_hex(x: Int) -> String`

将整数转换为十六进制字符串表示（小写）。

```almd run
fn main() -> Unit = {
  println(int.to_hex(255))
}
```
```output
ff
```

### `int.parse(s: String) -> Result[Int, String]`

将十进制字符串解析为整数。如果字符串不是有效整数，则返回 err。

仅忽略数字两侧的 ASCII 空白（空格、制表符、LF、CR、换页符）。其他空白，例如不换行空格（U+00A0）、全角空格（U+3000）或垂直制表符，都会导致错误：`int.parse("\u{A0}5")` 得到 `err("invalid digit found in string")`。若要接受 Unicode 空白，请先调用 `string.trim`。

```almd run
fn main() -> Unit = {
  match int.parse("42") {
    ok(n) => println("ok(${n})"),
    err(e) => println("err(${e})"),
  }
  match int.parse("4x2") {
    ok(n) => println("ok(${n})"),
    err(e) => println("err(${e})"),
  }
}
```
```output
ok(42)
err(invalid digit found in string)
```

### `int.abs(x: Int) -> Int`

返回整数的绝对值。

```almd run
fn main() -> Unit = {
  println("${int.abs(-5)}")
}
```
```output
5
```

### `int.is_even(x: Int) -> Bool`

奇偶性判定：`n` 能被2整除时为 `true`。对负数和边界值也精确（`%` 保留被除数的符号，因此判定为 `% 2 == 0`）。

```almd run
fn main() -> Unit = {
  println("${int.is_even(4)} ${int.is_even(-3)} ${int.is_even(0)}")
}
```
```output
true false true
```

### `int.is_odd(x: Int) -> Bool`

奇偶判定的另一半：`n` 不能被2整除时为 `true`。仅用于标量 `int`；有指定位宽的值通过 `int.from(x)` 转入。

```almd run
fn main() -> Unit = {
  println("${int.is_odd(7)} ${int.is_odd(-3)} ${int.is_odd(4)}")
}
```
```output
true true false
```

### `int.min(a: Int, b: Int) -> Int`

返回两个整数中较小的值。

```almd run
fn main() -> Unit = {
  println("${int.min(3, 7)}")
}
```
```output
3
```

### `int.max(a: Int, b: Int) -> Int`

返回两个整数中较大的值。

```almd run
fn main() -> Unit = {
  println("${int.max(3, 7)}")
}
```
```output
7
```

### `int.band(a: Int, b: Int) -> Int`

两个整数的按位 AND。

```almd run
fn main() -> Unit = {
  println("${int.band(0b1100, 0b1010)}") // 0b1000
}
```
```output
8
```

### `int.bor(a: Int, b: Int) -> Int`

两个整数的按位 OR。

```almd run
fn main() -> Unit = {
  println("${int.bor(0b1100, 0b1010)}") // 0b1110
}
```
```output
14
```

### `int.bxor(a: Int, b: Int) -> Int`

两个整数的按位 XOR。

```almd run
fn main() -> Unit = {
  println("${int.bxor(0b1100, 0b1010)}") // 0b0110
}
```
```output
6
```

### `int.bshl(a: Int, n: Int) -> Int`

按位左移。

```almd run
fn main() -> Unit = {
  println("${int.bshl(1, 3)}")
}
```
```output
8
```

### `int.bshr(a: Int, n: Int) -> Int`

按位右移（算术右移）。

```almd run
fn main() -> Unit = {
  println("${int.bshr(8, 2)}")
  println("${int.bshr(-8, 2)}")
}
```
```output
2
-2
```

### `int.bnot(x: Int) -> Int`

整数的按位 NOT（取反）。

```almd run
fn main() -> Unit = {
  println("${int.bnot(0)}")
}
```
```output
-1
```

### `int.wrap_add(a: Int, b: Int) -> Int`

64位 Int 的回绕加法：溢出时回绕。对于 N 位寄存器，与 `int.wrap` 组合：`int.wrap(int.wrap_add(a, b), bits)`。

```almd run
fn main() -> Unit = {
  println("${int.wrap_add(int.max_value(), 1)}")
  println("${int.wrap(int.wrap_add(255, 1), 8)}")
}
```
```output
-9223372036854775808
0
```

### `int.wrap_mul(a: Int, b: Int) -> Int`

64位 Int 的回绕乘法：溢出时回绕。对于 N 位寄存器，与 `int.wrap` 组合。

```almd run
fn main() -> Unit = {
  println("${int.wrap(int.wrap_mul(16, 16), 8)}")
}
```
```output
0
```

### `int.rotate_right(a: Int, n: Int) -> Int`

将 `a` 的64位向右旋转。次数 `n` 对64取欧几里得模，因此负数次数会向左旋转：`rotate_right(1, -1) = 2`。

```almd run
fn main() -> Unit = {
  println("${int.rotate_right(4, 1)}")
}
```
```output
2
```

### `int.rotate_left(a: Int, n: Int) -> Int`

将 `a` 的64位向左旋转。次数 `n` 对64取欧几里得模，因此负数次数会向右旋转：`rotate_left(2, -1) = 1`。

```almd run
fn main() -> Unit = {
  println("${int.rotate_left(int.min_value(), 1)}")
}
```
```output
1
```

### `int.clamp(x: Int, lo: Int, hi: Int) -> Int`

将整数限制在 [lo, hi] 范围内。

```almd run
fn main() -> Unit = {
  println("${int.clamp(15, 0, 10)}")
  println("${int.clamp(-3, 0, 10)}")
}
```
```output
10
0
```

## 转换矩阵的完整性 (#956)

转换以其构造的类型命名（R-L0-40）：`int.from(x)`（超出范围时中止）、`int.from_checked(x)`（`none`）、`int.from_saturating(x)`（限制范围，NaN 为0）和 `int.from_wrapping(x)`（整数输入，二进制补码）接受数值输入，包括高端范围无法容纳于 `Int` 的 `UInt64 → Int`。每个有指定位宽的模块都为自身类型提供相同的转换族，每个固定宽度数值类型都有 `min_value()`/`max_value()`。


## 原文固定快照 API 索引（47个函数）

```
// Decimal digits, with - when negative.
// @since 0.5.0 or earlier
int.to_string(x: Int) -> String

// Lowercase hex, no 0x; -1 gives 16 f digits.
// @since 0.5.0 or earlier
int.to_hex(x: Int) -> String

// Decimal Int from s trimmed of ASCII space/tab/LF/FF/CR; err on junk or overflow.
// @since 0.5.0 or earlier
int.parse(s: String) -> Result[Int, String]

// Magnitude of x; min_value wraps to itself.
// @since 0.5.0 or earlier
int.abs(x: Int) -> Int

// Lesser of a and b.
// @since 0.5.0 or earlier
int.min(a: Int, b: Int) -> Int

// Greater of a and b.
// @since 0.5.0 or earlier
int.max(a: Int, b: Int) -> Int

// x limited to lo..hi; aborts when lo > hi.
// @since 0.5.0 or earlier
int.clamp(x: Int, lo: Int, hi: Int) -> Int

// True when x % 2 == 0; exact for negatives.
// @since 0.62.0
int.is_even(x: Int) -> Bool

// True when x % 2 != 0; -3 counts as odd.
// @since 0.62.0
int.is_odd(x: Int) -> Bool

// Bitwise AND over all 64 bits.
// @since 0.5.0 or earlier
int.band(a: Int, b: Int) -> Int

// Bitwise OR over all 64 bits.
// @since 0.5.0 or earlier
int.bor(a: Int, b: Int) -> Int

// Bitwise XOR over all 64 bits.
// @since 0.5.0 or earlier
int.bxor(a: Int, b: Int) -> Int

// Bitwise complement; equals -x - 1.
// @since 0.5.0 or earlier
int.bnot(x: Int) -> Int

// a shifted left by n bits; n is taken mod 64.
// @since 0.5.0 or earlier
int.bshl(a: Int, n: Int) -> Int

// Arithmetic (sign-filling) right shift; n mod 64.
// @since 0.5.0 or earlier
int.bshr(a: Int, n: Int) -> Int

// Leading zero bits of the 64-bit word; 64 for 0.
// @since 0.16.0 or earlier
int.count_leading_zeros(x: Int) -> Int

// Trailing zero bits; 64 for 0.
// @since 0.16.0 or earlier
int.count_trailing_zeros(x: Int) -> Int

// Number of set bits; 64 for -1.
// @since 0.16.0 or earlier
int.pop_count(n: Int) -> Int   (deprecated — use int.count_ones)

// Number of set bits; 64 for -1.
// @since unreleased
int.count_ones(x: Int) -> Int

// The 64 bits in reverse order; 1 gives min_value.
// @since 0.16.0 or earlier
int.bit_reverse(x: Int) -> Int

// The 8 bytes in reverse order (endian flip).
// @since 0.16.0 or earlier
int.byte_swap(x: Int) -> Int

// Bits needed to hold n; 0 for 0, 64 if negative.
// @since 0.16.0 or earlier
int.bit_width(n: Int) -> Int   (deprecated — use int.bit_length)

// Bits needed to hold x; 0 for 0, 64 if negative.
// @since unreleased
int.bit_length(x: Int) -> Int

// Floor of log2(x); -1 when x <= 0.
// @since 0.16.0 or earlier
int.log2_floor(x: Int) -> Int

// Ceiling of log2(x); 0 when x <= 1.
// @since 0.16.0 or earlier
int.log2_ceil(x: Int) -> Int

// Least power of 2 >= x; 1 if x <= 1; min_value past 2^62.
// @since 0.16.0 or earlier
int.next_power_of_two(x: Int) -> Int

// Greatest power of 2 <= x; 0 when x <= 0.
// @since 0.16.0 or earlier
int.prev_power_of_two(x: Int) -> Int

// a + b wrapped to 64 bits.
// @since 0.5.0 or earlier
int.wrap_add(a: Int, b: Int) -> Int

// a * b wrapped to 64 bits.
// @since 0.5.0 or earlier
int.wrap_mul(a: Int, b: Int) -> Int

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since 0.5.0 or earlier
int.rotate_right(a: Int, n: Int) -> Int

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since 0.5.0 or earlier
int.rotate_left(a: Int, n: Int) -> Int

// Smallest Int: -2^63.
// @since 0.38.0
int.min_value() -> Int

// Largest Int: 2^63-1.
// @since 0.38.0
int.max_value() -> Int

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int.compare(a: Int, b: Int) -> Int

// -1, 0 or 1 by the sign of x; 0 for 0.
// @since unreleased
int.sign(x: Int) -> Int

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
int.pow(base: Int, exp: Int) -> Int

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int.floor_div(a: Int, b: Int) -> Int

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int.modulo(a: Int, b: Int) -> Int

// x mod 2^bits, non-negative when bits < 64; bits <= 0 aborts, bits >= 64 keeps x.
// @since unreleased
int.wrap(x: Int, bits: Int) -> Int

// a - b wrapped to 64 bits.
// @since unreleased
int.wrap_sub(a: Int, b: Int) -> Int

// Zero-filling right shift of the 64 bits; n is taken mod 64.
// @since unreleased
int.bshr_logical(a: Int, n: Int) -> Int

// Signed digits of x in radix, lowercase; -255 in 16 is "-ff"; radix outside 2..36 aborts.
// @since unreleased
int.to_string_radix(x: Int, radix: Int) -> String

// Int from trimmed s in radix: a + or - sign, any case, and 0x, 0o, 0b allowed for 16, 8, 2;
// err on junk or overflow; radix outside 2..36 aborts.
// @since unreleased
int.parse_radix(s: String, radix: Int) -> Result[Int, String]

// x as Int, toward zero; NaN, an infinity or a value outside Int aborts.
// @since unreleased
int.from(x: S) -> Int

// some(x as Int) when Int holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int.from_checked(x: S) -> Option[Int]

// x as Int, toward zero, clamped to Int's range; NaN is 0.
// @since unreleased
int.from_saturating(x: S) -> Int

// The low 64 bits of integer x as Int (two's complement).
// @since unreleased
int.from_wrapping(x: S) -> Int
```


## API 说明译文（按上述签名顺序）

- `int.to_string(x: Int) -> String`：十进制数字，负数带 -。
- `int.to_hex(x: Int) -> String`：小写十六进制，不带0x；-1得到16个 f。
- `int.parse(s: String) -> Result[Int, String]`：去除 s 两侧的 ASCII 空格/制表符/LF/FF/CR 后解析为十进制 Int；无效内容或溢出返回 err。
- `int.abs(x: Int) -> Int`：x 的大小；min_value 回绕为自身。
- `int.min(a: Int, b: Int) -> Int`：a 与 b 中较小者。
- `int.max(a: Int, b: Int) -> Int`：a 与 b 中较大者。
- `int.clamp(x: Int, lo: Int, hi: Int) -> Int`：将 x 限制在 lo..hi；lo > hi 时中止。
- `int.is_even(x: Int) -> Bool`：x % 2 == 0 时为 true；对负数也精确。
- `int.is_odd(x: Int) -> Bool`：x % 2 != 0 时为 true；-3算作奇数。
- `int.band(a: Int, b: Int) -> Int`：全部64位的按位 AND。
- `int.bor(a: Int, b: Int) -> Int`：全部64位的按位 OR。
- `int.bxor(a: Int, b: Int) -> Int`：全部64位的按位 XOR。
- `int.bnot(x: Int) -> Int`：按位取反，等于 -x - 1。
- `int.bshl(a: Int, n: Int) -> Int`：将 a 左移 n 位；n 对64取模。
- `int.bshr(a: Int, n: Int) -> Int`：算术（填充符号位）右移；n 对64取模。
- `int.count_leading_zeros(x: Int) -> Int`：64位字的前导零位数；0时为64。
- `int.count_trailing_zeros(x: Int) -> Int`：尾随零位数；0时为64。
- `int.pop_count(n: Int) -> Int   (deprecated — use int.count_ones)`：置位的位数；-1时为64。
- `int.count_ones(x: Int) -> Int`：置位的位数；-1时为64。
- `int.bit_reverse(x: Int) -> Int`：反转64位的顺序；1得到 min_value。
- `int.byte_swap(x: Int) -> Int`：反转8字节的顺序（翻转字节序）。
- `int.bit_width(n: Int) -> Int   (deprecated — use int.bit_length)`：容纳 n 所需的位数；0为0，负数为64。
- `int.bit_length(x: Int) -> Int`：容纳 x 所需的位数；0为0，负数为64。
- `int.log2_floor(x: Int) -> Int`：log2(x) 向下取整；x <= 0 时为 -1。
- `int.log2_ceil(x: Int) -> Int`：log2(x) 向上取整；x <= 1 时为0。
- `int.next_power_of_two(x: Int) -> Int`：大于等于 x 的最小2的幂；x <= 1 时为1，超过2^62时为 min_value。
- `int.prev_power_of_two(x: Int) -> Int`：小于等于 x 的最大2的幂；x <= 0 时为0。
- `int.wrap_add(a: Int, b: Int) -> Int`：a + b 回绕到64位。
- `int.wrap_mul(a: Int, b: Int) -> Int`：a * b 回绕到64位。
- `int.rotate_right(a: Int, n: Int) -> Int`：将 a 的64位向右旋转 n 位；n 对64取模。
- `int.rotate_left(a: Int, n: Int) -> Int`：将 a 的64位向左旋转 n 位；n 对64取模。
- `int.min_value() -> Int`：最小 Int：-2^63。
- `int.max_value() -> Int`：最大 Int：2^63-1。
- `int.compare(a: Int, b: Int) -> Int`：a 小于、等于、大于 b 时分别为 -1、0、1。
- `int.sign(x: Int) -> Int`：按 x 的符号返回 -1、0、1；0返回0。
- `int.pow(base: Int, exp: Int) -> Int`：base 的 exp 次幂，像 * 一样回绕到64位；exp < 0 时中止。
- `int.floor_div(a: Int, b: Int) -> Int`：商向 -inf 舍入；与 / 一样，b == 0 和 min_value / -1 时中止。
- `int.modulo(a: Int, b: Int) -> Int`：余数与 b 同号，因此 a == floor_div(a, b) * b + modulo(a, b)；b == 0 时中止。
- `int.wrap(x: Int, bits: Int) -> Int`：x mod 2^bits，bits < 64 时非负；bits <= 0 时中止，bits >= 64 时保持 x。
- `int.wrap_sub(a: Int, b: Int) -> Int`：a - b 回绕到64位。
- `int.bshr_logical(a: Int, n: Int) -> Int`：对64位进行零填充右移；n 对64取模。
- `int.to_string_radix(x: Int, radix: Int) -> String`：用 radix 进制的小写带符号数字表示 x；-255的16进制是 "-ff"；radix 不在2..36时中止。
- `int.parse_radix(s: String, radix: Int) -> Result[Int, String]`：将去除两侧空白的 s 按 radix 解析为 Int：允许 + 或 - 符号、任意大小写，以及16、8、2进制的0x、0o、0b；无效内容或溢出返回 err，radix 不在2..36时中止。
- `int.from(x: S) -> Int`：将 x 向零舍入为 Int；NaN、无穷大或超出 Int 范围的值会中止。
- `int.from_checked(x: S) -> Option[Int]`：若 Int 能精确容纳 x，则 some(x as Int)，否则（小数、NaN、越界）为 none。
- `int.from_saturating(x: S) -> Int`：将 x 向零舍入为 Int，并限制在 Int 范围内；NaN 为0。
- `int.from_wrapping(x: S) -> Int`：将整数 x 的低64位作为 Int（二进制补码）。
