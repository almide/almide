# math

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](math.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](math.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](math.zh-CN.md)



> 原文：[math.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/math.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。
>
> 原文注意：libm 精度和目标一致性是原文主张，并非本译文独立证明。正确舍入与 1 ulp 以内误差是不同概念。

数学函数。自动导入；`import math` 可选。

### `math.pi() -> Float`

返回数学常数 pi（3.14159...）。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.pi()))
}
```
```output
3.141592653589793
```

### `math.e() -> Float`

返回欧拉数 e（2.71828...）。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.e()))
}
```
```output
2.718281828459045
```

### `math.sin(x: Float) -> Float`

返回弧度角的正弦。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.sin(0.0)))
}
```
```output
0.0
```

### `math.cos(x: Float) -> Float`

返回弧度角的余弦。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.cos(0.0)))
}
```
```output
1.0
```

### `math.tan(x: Float) -> Float`

返回弧度角的正切。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.tan(0.0)))
}
```
```output
0.0
```

### `math.log(x: Float) -> Float`

返回 float 的自然对数（底 e）。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log(1.0)))
}
```
```output
0.0
```

### `math.exp(x: Float) -> Float`

返回 e 的给定次方。同捆 libm 保证结果距真值不超过 1 ulp，且跨目标逐字节相同（C-305），但不是正确舍入：`math.exp(1.0)` 比正确舍入的常数 `math.e()` 高 1 ulp。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.exp(0.0)))
  println(float.to_string(math.exp(1.0)))
  println(float.to_string(math.e()))
}
```
```output
1.0
2.7182818284590455
2.718281828459045
```

### `math.sqrt(x: Float) -> Float`

返回 float 的平方根。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.sqrt(16.0)))
}
```
```output
4.0
```

### `math.log10(x: Float) -> Float`

返回 float 的底 10 对数。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log10(100.0)))
}
```
```output
2.0
```

### `math.log2(x: Float) -> Float`

返回 float 的底 2 对数。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log2(8.0)))
}
```
```output
3.0
```

### `math.factorial(n: Int) -> Int`

返回非负整数的阶乘（n <= 1 时为 1）。与所有 Int 乘积一样，乘积按 modulo 2^64 回绕，因此超过 20 后不再正确，n >= 66 时全为 0：从这里起 n! 至少包含 64 个因子 2（v2(66!) = 66 - popcount(66) = 64），故立即返回答案，无需 n 次乘法。

```almd run
fn main() -> Unit = {
  println("${math.factorial(5)}")
}
```
```output
120
```

### `math.choose(n: Int, k: Int) -> Int`

返回二项式系数 C(n, k) = n! / (k! * (n-k)!)，只要能容纳于 Int 就精确（`choose(62, 31)` = 465428353255261088）；更大结果与所有 Int 乘积一样按 modulo 2^64 回绕。`k < 0` 或 `k > n` 返回 0。

```almd run
fn main() -> Unit = {
  println("${math.choose(5, 2)}")
}
```
```output
10
```

### `math.log_gamma(x: Float) -> Float`

在整个实数轴（含负非整数）上返回伽马函数绝对值的自然对数 `ln|Γ(x)|`。非正整数是极点，返回 `inf`。

```almd run
fn main() -> Unit = {
  println(float.to_fixed(math.log_gamma(5.0), 3))
}
```
```output
3.178
```



<a id="signature-index-27-functions"></a>

## 固定快照 API 索引 (27 个函数)

```
// Arctangent in radians, within -pi/2..pi/2.
// @since 0.30.0 or earlier
math.atan(x: Float) -> Float

// Binomial n over k; 0 if k < 0 or k > n; overflow wraps.
// @since 0.5.13 or earlier
math.choose(n: Int, k: Int) -> Int

// Cosine of x radians; NaN for infinite x.
// @since 0.5.0 or earlier
math.cos(x: Float) -> Float

// Euler's number, 2.718281828459045.
// @since 0.5.0 or earlier
math.e() -> Float

// e to the x; overflows to inf above ~709.78.
// @since 0.5.0 or earlier
math.exp(x: Float) -> Float

// Product 1..n; 1 for n <= 0; wraps past 20, 0 from 66 on.
// @since 0.5.13 or earlier
math.factorial(n: Int) -> Int

// Natural log; -inf at 0, NaN for negative x.
// @since 0.5.0 or earlier
math.log(x: Float) -> Float

// Base-10 log; -inf at 0, NaN for negative x.
// @since 0.5.13 or earlier
math.log10(x: Float) -> Float

// Base-2 log; -inf at 0, NaN for negative x.
// @since 0.5.13 or earlier
math.log2(x: Float) -> Float

// ln|Gamma(x)| on the whole line; inf at the poles.
// @since 0.5.13 or earlier
math.log_gamma(x: Float) -> Float

// The circle constant, 3.141592653589793.
// @since 0.5.0 or earlier
math.pi() -> Float

// Sine of x radians; NaN for infinite x.
// @since 0.5.0 or earlier
math.sin(x: Float) -> Float

// Square root; NaN below 0, -0.0 stays -0.0.
// @since 0.5.0 or earlier
math.sqrt(x: Float) -> Float

// Tangent of x radians; NaN for infinite x.
// @since 0.5.0 or earlier
math.tan(x: Float) -> Float

// Hyperbolic tangent in -1..1; inf gives 1.0.
// @since 0.30.0 or earlier
math.tanh(x: Float) -> Float

// Arcsine in radians, within -pi/2..pi/2; NaN outside -1..1.
// @since unreleased
math.asin(x: Float) -> Float

// Arccosine in radians, within 0..pi; NaN outside -1..1.
// @since unreleased
math.acos(x: Float) -> Float

// Angle of the point (x, y) in radians, within -pi..pi.
// @since unreleased
math.atan2(y: Float, x: Float) -> Float

// e to the x, minus 1, accurate near 0; -1.0 below about -38.
// @since unreleased
math.expm1(x: Float) -> Float

// Natural log of 1 + x, accurate near 0; -inf at -1, NaN below.
// @since unreleased
math.log1p(x: Float) -> Float

// Hyperbolic sine; overflows to +-inf past about 710.
// @since unreleased
math.sinh(x: Float) -> Float

// Hyperbolic cosine, at least 1.0; overflows to inf past about 710.
// @since unreleased
math.cosh(x: Float) -> Float

// sqrt(a*a + b*b) without overflow or underflow on the way.
// @since unreleased
math.hypot(a: Float, b: Float) -> Float

// Real cube root; negative x gives a negative root.
// @since unreleased
math.cbrt(x: Float) -> Float

// Inverse hyperbolic sine.
// @since unreleased
math.asinh(x: Float) -> Float

// Inverse hyperbolic cosine, at least 0.0; NaN below 1.0.
// @since unreleased
math.acosh(x: Float) -> Float

// Inverse hyperbolic tangent; +-inf at +-1, NaN outside -1..1.
// @since unreleased
math.atanh(x: Float) -> Float
```



## API 说明译文（按上述签名顺序）

- `math.atan(x: Float) -> Float` — 反正切，单位弧度，位于 -pi/2..pi/2。
- `math.choose(n: Int, k: Int) -> Int` — 二项式系数 n over k；k < 0 或 k > n 时为 0；溢出回绕。
- `math.cos(x: Float) -> Float` — x 弧度的余弦；无限 x 为 NaN。
- `math.e() -> Float` — 欧拉数 2.718281828459045。
- `math.exp(x: Float) -> Float` — e 的 x 次方；约 709.78 以上溢出为 inf。
- `math.factorial(n: Int) -> Int` — 1..n 的乘积；n <= 0 为 1；超过 20 回绕，从 66 起为 0。
- `math.log(x: Float) -> Float` — 自然对数；0 为 -inf，负 x 为 NaN。
- `math.log10(x: Float) -> Float` — 底 10 对数；0 为 -inf，负 x 为 NaN。
- `math.log2(x: Float) -> Float` — 底 2 对数；0 为 -inf，负 x 为 NaN。
- `math.log_gamma(x: Float) -> Float` — 整个实数轴上的 ln|Gamma(x)|；极点为 inf。
- `math.pi() -> Float` — 圆周率 3.141592653589793。
- `math.sin(x: Float) -> Float` — x 弧度的正弦；无限 x 为 NaN。
- `math.sqrt(x: Float) -> Float` — 平方根；低于 0 为 NaN，-0.0 保持 -0.0。
- `math.tan(x: Float) -> Float` — x 弧度的正切；无限 x 为 NaN。
- `math.tanh(x: Float) -> Float` — -1..1 内的双曲正切；inf 返回 1.0。
- `math.asin(x: Float) -> Float` — 反正弦，单位弧度，位于 -pi/2..pi/2；-1..1 外为 NaN。
- `math.acos(x: Float) -> Float` — 反余弦，单位弧度，位于 0..pi；-1..1 外为 NaN。
- `math.atan2(y: Float, x: Float) -> Float` — 点 (x, y) 的角度，单位弧度，位于 -pi..pi。
- `math.expm1(x: Float) -> Float` — e 的 x 次方减 1，在 0 附近保持精度；约 -38 以下为 -1.0。
- `math.log1p(x: Float) -> Float` — 1 + x 的自然对数，在 0 附近保持精度；-1 为 -inf，更低为 NaN。
- `math.sinh(x: Float) -> Float` — 双曲正弦；约 710 以外溢出为 +-inf。
- `math.cosh(x: Float) -> Float` — 双曲余弦，至少 1.0；约 710 以外溢出为 inf。
- `math.hypot(a: Float, b: Float) -> Float` — sqrt(a*a + b*b)，中间过程不溢出或下溢。
- `math.cbrt(x: Float) -> Float` — 实立方根；负 x 得到负根。
- `math.asinh(x: Float) -> Float` — 反双曲正弦。
- `math.acosh(x: Float) -> Float` — 反双曲余弦，至少 0.0；1.0 以下为 NaN。
- `math.atanh(x: Float) -> Float` — 反双曲正切；+-1 为 +-inf，-1..1 外为 NaN。
