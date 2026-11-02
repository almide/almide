# float

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](float.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](float.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](float.zh-CN.md)



> 原文：[float.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

> 原文の「NaN はすべての比較を false にする」という理由付けは、API 索引の全順序比較（NaN は自身と等しい）などと範囲が異なります。この訳では説明差を統一せず保持しており、あらゆる比較演算や安全性を独立に保証するものではありません。

浮動小数点演算。自動インポートされます。

### `float.to_string(x: Float) -> String`

浮動小数点数を文字列表現に変換します。

```almd run
fn main() -> Unit = {
  println(float.to_string(3.14))
}
```
```output
3.14
```

### `float.round(n: Float) -> Float`

浮動小数点数を最も近い整数値（Float 型）に丸め、中間値はゼロから離れる方向に丸めます（`round(2.5) = 3.0`、`round(-2.5) = -3.0`）。実際の値に対して正確に処理します。`round(0.49999999999999994)` は `0.0` で、2^52 を超える奇数の整数は変わりません。`-0.0` と `(-0.5, 0)` の値は `-0.0` に丸めます。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.round(3.6)))
}
```
```output
4.0
```

### `float.floor(x: Float) -> Float`

浮動小数点数を下側の最も近い整数値（Float 型）に丸めます。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.floor(3.9)))
}
```
```output
3.0
```

### `float.ceil(x: Float) -> Float`

浮動小数点数を上側の最も近い整数値（Float 型）に丸めます。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.ceil(3.1)))
}
```
```output
4.0
```

### `float.abs(x: Float) -> Float`

浮動小数点数の絶対値を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.abs(-2.5)))
}
```
```output
2.5
```

### `float.parse(s: String) -> Result[Float, String]`

文字列を有限の浮動小数点数に解析します。有効な数値でない場合は err を返します。

数値の前後にある ASCII 空白（スペース、タブ、LF、CR、フォームフィード）だけを無視し、改行しないスペースなど他の空白はエラーです。結果は常に有限です。`"inf"`、`"Infinity"`、`"NaN"` は大文字・小文字にかかわらず `err("invalid float literal")` となり、`"1e400"` など Float には大きすぎる数は `err("float literal out of range")` となります。小さすぎる数は `0.0` に丸めます。多くの言語はこれらの表記を受け入れますが、Almide は拒否します。原文では、その理由を NaN がすべての比較を false にするため、`if amount > limit then reject` のような検査を通過してしまうからだと説明しています。無限大が必要なら `1.0 / 0.0` と書きます。

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

2つの浮動小数点数の小さい方を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.min(1.5, 2.5)))
}
```
```output
1.5
```

### `float.max(a: Float, b: Float) -> Float`

2つの浮動小数点数の大きい方を返します。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.max(1.5, 2.5)))
}
```
```output
2.5
```

### `float.to_fixed(x: Float, decimals: Int) -> String`

小数点以下の桁数を固定して浮動小数点数を整形します。

```almd run
fn main() -> Unit = {
  println(float.to_fixed(3.14159, 2))
}
```
```output
3.14
```

### `float.clamp(x: Float, lo: Float, hi: Float) -> Float`

浮動小数点数を範囲 [lo, hi] に収めます。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.clamp(15.0, 0.0, 10.0)))
}
```
```output
10.0
```

### `float.signum(n: Float) -> Float`

浮動小数点数の符号を1.0または-1.0で返し、符号ビット（f64 の `signum`）で判定します。`signum(0.0)` は1.0、`signum(-0.0)` は-1.0です。NaN 入力には NaN を返します。0.0を返すことはありません。`float.sign` は非推奨の別名です。

```almd run
fn main() -> Unit = {
  println(float.to_string(float.signum(-3.5)))
}
```
```output
-1.0
```

### `float.is_nan(x: Float) -> Bool`

浮動小数点数が NaN（非数）か調べます。

```almd run
fn main() -> Unit = {
  println("${float.is_nan(0.0 / 0.0)}")
}
```
```output
true
```

### `float.is_infinite(x: Float) -> Bool`

浮動小数点数が正または負の無限大か調べます。

```almd run
fn main() -> Unit = {
  println("${float.is_infinite(1.0 / 0.0)}")
}
```
```output
true
```

### `float.to_bits(f: Float) -> Int`

IEEE 754 binary64 のビット表現を Int として返します。すべての NaN は正規化されます。

```almd run
fn main() -> Unit = {
  println("${float.to_bits(1.0)}")
}
```
```output
4607182418800017408
```


<a id="signature-index-27-functions"></a>

## 原文固定スナップショットの API 索引（27関数）

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


## API 説明の訳（上記署名順）

- `float.to_string(x: Float) -> String`：往復変換可能な最短の数字列。指数表記はなく、2.0 は .0 を保ちます。
- `float.parse(s: String) -> Result[Float, String]`：s の前後の ASCII 空白を除去して有限 Float にします。不正な文字、inf、NaN、オーバーフローは err。
- `float.to_fixed(x: Float, decimals: Int) -> String`：固定小数点表記で、中間値は偶数へ丸めます。decimals が0..4096の範囲外なら中断します。
- `float.to_bits(f: Float) -> Int`：IEEE-754 ビットを Int として返します。どの NaN も正規 NaN になります。
- `float.abs(x: Float) -> Float`：絶対値。-0.0 は0.0、NaN は NaN のまま。
- `float.floor(x: Float) -> Float`：-inf 方向に丸めます。-0.5 は -1.0。
- `float.ceil(x: Float) -> Float`：+inf 方向に丸めます。-0.5 は -0.0。
- `float.round(x: Float) -> Float`：最も近い整数値。中間値はゼロから離れる方向に丸めます。
- `float.min(a: Float, b: Float) -> Float`：小さい方。NaN のオペランドは無視します。-0.0 < 0.0。
- `float.max(a: Float, b: Float) -> Float`：大きい方。NaN のオペランドは無視します。0.0 > -0.0。
- `float.clamp(x: Float, lo: Float, hi: Float) -> Float`：x を lo..hi に制限します。lo > hi または境界が NaN の場合は中断します。
- `float.sign(n: Float) -> Float   (deprecated — use float.signum)`：符号ビットに応じて1.0または-1.0（0.0 は1.0）。NaN には NaN。
- `float.signum(x: Float) -> Float`：符号ビットに応じて1.0または-1.0（0.0 は1.0、-0.0 は-1.0）。NaN には NaN。
- `float.is_nan(x: Float) -> Bool`：NaN（自身と等しくない）に対してのみ true。
- `float.is_infinite(x: Float) -> Bool`：inf と -inf に対して true、NaN に対して false。
- `float.trunc(x: Float) -> Float`：0方向に丸めます。-0.5 は -0.0。
- `float.is_finite(x: Float) -> Bool`：x が inf、-inf、NaN のいずれでもなければ true。
- `float.pow(base: Float, exp: Float) -> Float`：base の exp 乗。負の base と小数の exp の組み合わせでは NaN。
- `float.compare(a: Float, b: Float) -> Int`：IEEE-754 全順序で -1、0、1：-0.0 < 0.0、NaN は inf より上で自身と等しい。
- `float.from_bits(bits: Int) -> Float`：IEEE-754 binary64 のビットが bits である Float。どの NaN も単一の NaN になります。
- `float.infinity() -> Float`：正の無限大。
- `float.nan() -> Float`：単一の静かな NaN（ビットは0x7ff8000000000000）。
- `float.epsilon() -> Float`：1.0 と次の Float の間隔：2^-52。
- `float.max_value() -> Float`：最大の有限 Float。約1.7976931348623157e308。
- `float.min_value() -> Float`：最も負の有限 Float（= -max_value()）。最小の正数ではありません。
- `float.from(x: S) -> Float`：x を最も近い Float にします（中間値は偶数へ）。NaN は NaN のまま。
- `float.from_checked(x: S) -> Option[Float]`：Float が x を正確に保持できれば some(x as Float)、それ以外（丸めが生じる値、NaN）は none。
