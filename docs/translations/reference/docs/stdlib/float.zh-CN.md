# float

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](float.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](float.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](float.zh-CN.md)



> 原文：[float.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

> 原文关于“NaN 使所有比较为 false”的理由，与 API 索引中全序比较（NaN 等于自身）等的适用范围不同。本译文保留而不统一这些说明差异，不独立保证所有比较运算或安全性。

浮点运算。自动导入。

### `float.to_string(x: Float) -> String`

将浮点数转换为字符串表示。

```almd run
fn main() -> Unit = {
  println(float.to_string(3.14))
}
```
```output
3.14
```

### `float.round(n: Float) -> Float`

将浮点数舍入到最近的整数值（Float 类型），中点向远离零的方向舍入（`round(2.5) = 3.0`、`round(-2.5) = -3.0`），准确依据实际值：`round(0.49999999999999994)` 为 `0.0`，大于2^52的奇整数保持不变。`-0.0` 和 `(-0.5, 0)` 内的值舍入为 `-0.0`。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.round(3.6)))
}
```
```output
4.0
```

### `float.floor(x: Float) -> Float`

将浮点数向下舍入到最近的整数值（Float 类型）。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.floor(3.9)))
}
```
```output
3.0
```

### `float.ceil(x: Float) -> Float`

将浮点数向上舍入到最近的整数值（Float 类型）。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.ceil(3.1)))
}
```
```output
4.0
```

### `float.abs(x: Float) -> Float`

返回浮点数的绝对值。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.abs(-2.5)))
}
```
```output
2.5
```

### `float.parse(s: String) -> Result[Float, String]`

将字符串解析为有限浮点数。若字符串不是有效数字，则返回 err。

仅忽略数字两侧的 ASCII 空白（空格、制表符、LF、CR、换页符）；其他空白（例如不换行空格）会导致错误。结果总是有限值：`"inf"`、`"Infinity"` 和 `"NaN"` 无论大小写都得到 `err("invalid float literal")`，对 Float 而言过大的数（例如 `"1e400"`）得到 `err("float literal out of range")`。过小的数舍入为 `0.0`。许多语言接受这些写法；Almide 拒绝它们，原文给出的理由是 NaN 使所有比较为 false，从而会通过 `if amount > limit then reject` 这样的检查。需要无穷大时请写 `1.0 / 0.0`。

```almd run
fn show(r: Result[Float, String]) -> String = match r {
  ok(x) => "ok(${float.to_string(x)})",
  err(e) => "err(${e})",
}

fn main() -> Unit = {
  println(show(float.parse("3.14")))
  println(show(float.parse("abc")))
}
```
```output
ok(3.14)
err(invalid float literal)
```

### `float.min(a: Float, b: Float) -> Float`

返回两个浮点数中较小的值。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.min(1.5, 2.5)))
}
```
```output
1.5
```

### `float.max(a: Float, b: Float) -> Float`

返回两个浮点数中较大的值。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.max(1.5, 2.5)))
}
```
```output
2.5
```

### `float.to_fixed(x: Float, decimals: Int) -> String`

按固定小数位数格式化浮点数。

```almd run
fn main() -> Unit = {
  println(float.to_fixed(3.14159, 2))
}
```
```output
3.14
```

### `float.clamp(x: Float, lo: Float, hi: Float) -> Float`

将浮点数限制在 [lo, hi] 范围内。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.clamp(15.0, 0.0, 10.0)))
}
```
```output
10.0
```

### `float.signum(n: Float) -> Float`

按符号位（f64 的 `signum`）返回浮点数的符号，值为1.0或-1.0：`signum(0.0)` 为1.0，`signum(-0.0)` 为-1.0。NaN 输入返回 NaN。绝不返回0.0。`float.sign` 是已弃用的别名。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.signum(-3.5)))
}
```
```output
-1.0
```

### `float.is_nan(x: Float) -> Bool`

检查浮点数是否为 NaN（非数）。

```almd run
fn main() -> Unit = {
  println("${float.is_nan(0.0 / 0.0)}")
}
```
```output
true
```

### `float.is_infinite(x: Float) -> Bool`

检查浮点数是否为正无穷或负无穷。

```almd run
fn main() -> Unit = {
  println("${float.is_infinite(1.0 / 0.0)}")
}
```
```output
true
```

### `float.to_bits(f: Float) -> Int`

将 IEEE 754 binary64 位表示作为 Int 返回；所有 NaN 都会被规范化。

```almd run
fn main() -> Unit = {
  println("${float.to_bits(1.0)}")
}
```
```output
4607182418800017408
```


<a id="signature-index-27-functions"></a>

## 原文固定快照 API 索引（27个函数）

```
// Shortest round-trip digits, no exponent; 2.0 keeps .0.
// @since 0.5.0 or earlier
float.to_string(x: Float) -> String

// Finite Float from s trimmed of ASCII space; err on junk, inf, NaN, overflow.
// @since 0.5.0 or earlier
float.parse(s: String) -> Result[Float, String]

// Fixed-point, ties to even; decimals outside 0..4096 abort.
// @since 0.5.0 or earlier
float.to_fixed(x: Float, decimals: Int) -> String

// IEEE-754 bits as Int; any NaN gives the canonical NaN.
// @since 0.12.1 or earlier
float.to_bits(f: Float) -> Int

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since 0.5.0 or earlier
float.abs(x: Float) -> Float

// Round toward -inf; -0.5 gives -1.0.
// @since 0.5.0 or earlier
float.floor(x: Float) -> Float

// Round toward +inf; -0.5 gives -0.0.
// @since 0.5.0 or earlier
float.ceil(x: Float) -> Float

// Nearest whole value; halves round away from zero.
// @since 0.5.0 or earlier
float.round(x: Float) -> Float

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since 0.5.0 or earlier
float.min(a: Float, b: Float) -> Float

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since 0.5.0 or earlier
float.max(a: Float, b: Float) -> Float

// x limited to lo..hi; lo > hi or a NaN bound aborts.
// @since 0.5.0 or earlier
float.clamp(x: Float, lo: Float, hi: Float) -> Float

// 1.0 or -1.0 by sign bit (0.0 gives 1.0); NaN for NaN.
// @since 0.5.13 or earlier
float.sign(n: Float) -> Float   (deprecated — use float.signum)

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float.signum(x: Float) -> Float

// True only for NaN (unequal to itself).
// @since 0.6.0 or earlier
float.is_nan(x: Float) -> Bool

// True for inf and -inf; false for NaN.
// @since 0.6.0 or earlier
float.is_infinite(x: Float) -> Bool

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float.trunc(x: Float) -> Float

// True unless x is inf, -inf or NaN.
// @since unreleased
float.is_finite(x: Float) -> Bool

// base to the exp; NaN for a negative base with a fractional exp.
// @since unreleased
float.pow(base: Float, exp: Float) -> Float

// -1, 0 or 1 in IEEE-754 total order: -0.0 < 0.0, NaN above inf and equal to itself.
// @since unreleased
float.compare(a: Float, b: Float) -> Int

// Float whose IEEE-754 binary64 bits are bits; any NaN gives the one NaN.
// @since unreleased
float.from_bits(bits: Int) -> Float

// Positive infinity.
// @since unreleased
float.infinity() -> Float

// The one quiet NaN (bits 0x7ff8000000000000).
// @since unreleased
float.nan() -> Float

// Gap between 1.0 and the next Float: 2^-52.
// @since unreleased
float.epsilon() -> Float

// Largest finite Float, about 1.7976931348623157e308.
// @since unreleased
float.max_value() -> Float

// Most negative finite Float (= -max_value()), not the smallest positive.
// @since unreleased
float.min_value() -> Float

// x as the nearest Float (ties to even); NaN stays NaN.
// @since unreleased
float.from(x: S) -> Float

// some(x as Float) when Float holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float.from_checked(x: S) -> Option[Float]
```


## API 说明译文（按上述签名顺序）

- `float.to_string(x: Float) -> String`：可往返转换的最短数字串，无指数；2.0保留 .0。
- `float.parse(s: String) -> Result[Float, String]`：去除 s 两侧 ASCII 空白后解析为有限 Float；无效内容、inf、NaN、溢出返回 err。
- `float.to_fixed(x: Float, decimals: Int) -> String`：定点格式，中点舍入到偶数；decimals 不在0..4096时中止。
- `float.to_bits(f: Float) -> Int`：以 Int 返回 IEEE-754 位；任何 NaN 都得到规范 NaN。
- `float.abs(x: Float) -> Float`：绝对值；-0.0得到0.0，NaN 保持 NaN。
- `float.floor(x: Float) -> Float`：向 -inf 舍入；-0.5得到 -1.0。
- `float.ceil(x: Float) -> Float`：向 +inf 舍入；-0.5得到 -0.0。
- `float.round(x: Float) -> Float`：最近的整数值；中点向远离零的方向舍入。
- `float.min(a: Float, b: Float) -> Float`：较小者；忽略 NaN 操作数；-0.0 < 0.0。
- `float.max(a: Float, b: Float) -> Float`：较大者；忽略 NaN 操作数；0.0 > -0.0。
- `float.clamp(x: Float, lo: Float, hi: Float) -> Float`：将 x 限制在 lo..hi；lo > hi 或边界为 NaN 时中止。
- `float.sign(n: Float) -> Float   (deprecated — use float.signum)`：按符号位返回1.0或-1.0（0.0得到1.0）；NaN 得到 NaN。
- `float.signum(x: Float) -> Float`：按符号位返回1.0或-1.0（0.0得到1.0，-0.0得到-1.0）；NaN 得到 NaN。
- `float.is_nan(x: Float) -> Bool`：仅对 NaN（不等于自身）为 true。
- `float.is_infinite(x: Float) -> Bool`：对 inf 和 -inf 为 true，对 NaN 为 false。
- `float.trunc(x: Float) -> Float`：向0舍入；-0.5得到 -0.0。
- `float.is_finite(x: Float) -> Bool`：除非 x 为 inf、-inf 或 NaN，否则为 true。
- `float.pow(base: Float, exp: Float) -> Float`：base 的 exp 次幂；负 base 与小数 exp 得到 NaN。
- `float.compare(a: Float, b: Float) -> Int`：按 IEEE-754 全序返回 -1、0或1：-0.0 < 0.0，NaN 高于 inf 且等于自身。
- `float.from_bits(bits: Int) -> Float`：IEEE-754 binary64 位为 bits 的 Float；任何 NaN 都得到同一个 NaN。
- `float.infinity() -> Float`：正无穷。
- `float.nan() -> Float`：唯一的 quiet NaN（位为0x7ff8000000000000）。
- `float.epsilon() -> Float`：1.0与下一个 Float 的间隔：2^-52。
- `float.max_value() -> Float`：最大的有限 Float，约1.7976931348623157e308。
- `float.min_value() -> Float`：最负的有限 Float（= -max_value()），不是最小正数。
- `float.from(x: S) -> Float`：将 x 转为最近的 Float（中点到偶数）；NaN 保持 NaN。
- `float.from_checked(x: S) -> Option[Float]`：若 Float 能精确容纳 x，则 some(x as Float)，否则（被舍入的值、NaN）为 none。
