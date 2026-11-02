# math

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](math.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](math.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](math.zh-CN.md)



> 原文：[math.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/math.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。
>
> 原文上の注意: libm の精度・ターゲット一致は原文の主張であり、この翻訳による独立証明ではありません。正しい丸めと 1 ulp 以内の誤差は区別しています。

数学関数。自動 import され、`import math` は任意です。

### `math.pi() -> Float`

数学定数 pi（3.14159...）を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.pi()))
}
```
```output
3.141592653589793
```

### `math.e() -> Float`

ネイピア数 e（2.71828...）を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.e()))
}
```
```output
2.718281828459045
```

### `math.sin(x: Float) -> Float`

ラジアン角の正弦を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.sin(0.0)))
}
```
```output
0.0
```

### `math.cos(x: Float) -> Float`

ラジアン角の余弦を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.cos(0.0)))
}
```
```output
1.0
```

### `math.tan(x: Float) -> Float`

ラジアン角の正接を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.tan(0.0)))
}
```
```output
0.0
```

### `math.log(x: Float) -> Float`

float の自然対数（底 e）を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log(1.0)))
}
```
```output
0.0
```

### `math.exp(x: Float) -> Float`

e の指定べき乗を返します。同梱 libm は、結果が真値から 1 ulp 以内で、ターゲット間でバイト同一であることを保証します（C-305）。正しい丸めの保証ではありません。`math.exp(1.0)` は、正しく丸められた定数 `math.e()` より 1 ulp 上です。

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

float の平方根を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.sqrt(16.0)))
}
```
```output
4.0
```

### `math.log10(x: Float) -> Float`

float の底 10 の対数を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log10(100.0)))
}
```
```output
2.0
```

### `math.log2(x: Float) -> Float`

float の底 2 の対数を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(math.log2(8.0)))
}
```
```output
3.0
```

### `math.factorial(n: Int) -> Int`

非負整数の階乗を返します（n <= 1 なら 1）。すべての Int の積と同じく積は modulo 2^64 で wrap するため、20 を超えると正しくなく、n >= 66 なら常に 0 です。それ以降の n! は少なくとも 64 個の因子 2 を持つ（v2(66!) = 66 - popcount(66) = 64）ため、n 回の乗算をせず直ちに答えを返します。

```almd run
fn main() -> Unit = {
  println("${math.factorial(5)}")
}
```
```output
120
```

### `math.choose(n: Int, k: Int) -> Int`

二項係数 C(n, k) = n! / (k! * (n-k)!) を返します。Int に収まるなら正確です（`choose(62, 31)` = 465428353255261088）。それより大きい結果は、すべての Int の積と同じく modulo 2^64 で wrap します。`k < 0` または `k > n` は 0 を返します。

```almd run
fn main() -> Unit = {
  println("${math.choose(5, 2)}")
}
```
```output
10
```

### `math.log_gamma(x: Float) -> Float`

実数全域（負の非整数を含む）で、ガンマ関数の絶対値の自然対数 `ln|Γ(x)|` を返します。非正の整数は極であり、`inf` を返します。

```almd run
fn main() -> Unit = {
  println(float.to_fixed(math.log_gamma(5.0), 3))
}
```
```output
3.178
```



<a id="signature-index-27-functions"></a>

## 固定スナップショットの API 索引 (27 関数)

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



## API 説明の訳（上記署名順）

- `math.atan(x: Float) -> Float` — ラジアンの逆正接。-pi/2..pi/2 内。
- `math.choose(n: Int, k: Int) -> Int` — 二項係数 n over k。k < 0 または k > n は 0。overflow は wrap。
- `math.cos(x: Float) -> Float` — x ラジアンの余弦。無限大の x は NaN。
- `math.e() -> Float` — ネイピア数 2.718281828459045。
- `math.exp(x: Float) -> Float` — e の x 乗。約 709.78 超で inf に overflow。
- `math.factorial(n: Int) -> Int` — 1..n の積。n <= 0 は 1。20 超で wrap、66 以降は 0。
- `math.log(x: Float) -> Float` — 自然対数。0 は -inf、負の x は NaN。
- `math.log10(x: Float) -> Float` — 底 10 の対数。0 は -inf、負の x は NaN。
- `math.log2(x: Float) -> Float` — 底 2 の対数。0 は -inf、負の x は NaN。
- `math.log_gamma(x: Float) -> Float` — 実数全域の ln|Gamma(x)|。極では inf。
- `math.pi() -> Float` — 円周率 3.141592653589793。
- `math.sin(x: Float) -> Float` — x ラジアンの正弦。無限大の x は NaN。
- `math.sqrt(x: Float) -> Float` — 平方根。0 未満は NaN、-0.0 は -0.0 のまま。
- `math.tan(x: Float) -> Float` — x ラジアンの正接。無限大の x は NaN。
- `math.tanh(x: Float) -> Float` — -1..1 の双曲線正接。inf は 1.0。
- `math.asin(x: Float) -> Float` — ラジアンの逆正弦。-pi/2..pi/2 内。-1..1 外は NaN。
- `math.acos(x: Float) -> Float` — ラジアンの逆余弦。0..pi 内。-1..1 外は NaN。
- `math.atan2(y: Float, x: Float) -> Float` — 点 (x, y) の角度をラジアンで返します。-pi..pi 内。
- `math.expm1(x: Float) -> Float` — e の x 乗から 1 を引いた値。0 付近で精度を保ち、約 -38 未満は -1.0。
- `math.log1p(x: Float) -> Float` — 1 + x の自然対数。0 付近で精度を保ち、-1 は -inf、それ未満は NaN。
- `math.sinh(x: Float) -> Float` — 双曲線正弦。約 710 を超えると +-inf に overflow。
- `math.cosh(x: Float) -> Float` — 双曲線余弦。少なくとも 1.0。約 710 を超えると inf に overflow。
- `math.hypot(a: Float, b: Float) -> Float` — 途中で overflow や underflow を起こさず sqrt(a*a + b*b) を求めます。
- `math.cbrt(x: Float) -> Float` — 実立方根。負の x には負の根。
- `math.asinh(x: Float) -> Float` — 逆双曲線正弦。
- `math.acosh(x: Float) -> Float` — 逆双曲線余弦。少なくとも 0.0。1.0 未満は NaN。
- `math.atanh(x: Float) -> Float` — 逆双曲線正接。+-1 は +-inf、-1..1 外は NaN。
