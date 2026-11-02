# float64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](float64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](float64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](float64.zh-CN.md)



> 原文：[float64.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float64.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

`Float64`（64ビット浮動小数点、IEEE-754 binary64）：境界、算術演算、ビット、テキスト、この型への変換。自動インポートされるため `import` は不要です。

`Float64` と標準の `Float` は表現を共有しますが別の型なので、`float32` と同じ理由で `Float64` にも専用モジュールがあります。

<a id="conversions"></a>

## 変換

変換は生成する型にちなんで命名します（R-L0-40）。`float64.from(x)` は任意の数値入力、つまり `Int`、ビット幅を持つ整数、`Float`、ビット幅を持つ浮動小数点数を受け取ります。

```almd
let x: Float64 = float64.from(1.5)
let r = float64.from(0.1)              // the same value, re-tagged
let exact = float64.from_checked(16777217) // some: every Int up to 2^53 is exact
let n = int.from_saturating(x)         // leaving Float64: the destination's family
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `float64.from(x)` | 最も近い Float64（中間値は偶数へ）。NaN は NaN のまま |
| `float64.from_checked(x)` | Float64 が x を正確に保持できれば `some`、それ以外（丸めが生じる値、NaN）は `none` |

`Float64` から別の型にするには、変換先の関数群を呼びます。`float.from(x)` は正確に拡張します。整数への変換は名前で選びます。`int.from(x)` は NaN、無限大、範囲外の値で中断し、`int.from_checked(x)` は小数に `none` を返し、`int.from_saturating(x)` はゼロ方向に切り捨てて範囲内に制限します（NaN は0）。

正確な API 一覧は、以下に保持した原文の機械管理署名索引です（この訳は固定スナップショットであり、機械管理の対象ではありません）。

> 原文は Float64 と Float が同じ表現であると説明する一方、変換を “widens” と表現しています。この説明差は統一せず保持しています。

<a id="signature-index-24-functions"></a>

## 原文固定スナップショットの API 索引（24関数）

```
// Plain decimal, never an exponent; 2.0 keeps .0.
// @since 0.37.0 or earlier
float64.to_string(x: Float64) -> String

// -1, 0 or 1 in IEEE 754 total order: -0.0 < 0.0, NaN above inf, NaN equal to NaN.
// @since unreleased
float64.compare(a: Float64, b: Float64) -> Int

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since unreleased
float64.min(a: Float64, b: Float64) -> Float64

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since unreleased
float64.max(a: Float64, b: Float64) -> Float64

// x limited to lo..hi; lo > hi or a NaN bound aborts.
// @since unreleased
float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64

// True only for NaN.
// @since unreleased
float64.is_nan(x: Float64) -> Bool

// True for inf and -inf; false for NaN.
// @since unreleased
float64.is_infinite(x: Float64) -> Bool

// True unless x is inf, -inf or NaN.
// @since unreleased
float64.is_finite(x: Float64) -> Bool

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since unreleased
float64.abs(x: Float64) -> Float64

// Round toward -inf; -0.5 gives -1.0.
// @since unreleased
float64.floor(x: Float64) -> Float64

// Round toward +inf; -0.5 gives -0.0.
// @since unreleased
float64.ceil(x: Float64) -> Float64

// Nearest whole value; halves round away from zero.
// @since unreleased
float64.round(x: Float64) -> Float64

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float64.trunc(x: Float64) -> Float64

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float64.signum(x: Float64) -> Float64

// The 64 IEEE-754 binary64 bits as Int; any NaN gives the canonical NaN.
// @since unreleased
float64.to_bits(x: Float64) -> Int

// Float64 whose IEEE-754 binary64 bits are bits.
// @since unreleased
float64.from_bits(bits: Int) -> Float64

// Positive infinity.
// @since unreleased
float64.infinity() -> Float64

// The one quiet NaN (bits 0x7ff8000000000000).
// @since unreleased
float64.nan() -> Float64

// Gap between 1.0 and the next Float64: 2^-52.
// @since unreleased
float64.epsilon() -> Float64

// Largest finite Float64, about 1.7976931348623157e308.
// @since unreleased
float64.max_value() -> Float64

// Most negative finite Float64 (= -max_value()), not the smallest positive.
// @since unreleased
float64.min_value() -> Float64

// Float64 from ASCII-trimmed s, the grammar of float.parse; err on junk, inf, NaN.
// @since unreleased
float64.parse(s: String) -> Result[Float64, String]

// x as the nearest Float64 (ties to even); NaN stays NaN.
// @since unreleased
float64.from(x: S) -> Float64

// some(x as Float64) when Float64 holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float64.from_checked(x: S) -> Option[Float64]
```

## API 説明の訳（上記署名順）

- `float64.to_string(x: Float64) -> String`：通常の10進表記で、指数表記は使いません。2.0 は .0 を保ちます。
- `float64.compare(a: Float64, b: Float64) -> Int`：IEEE 754 全順序で -1、0、1：-0.0 < 0.0、NaN は inf より上で、NaN 同士は等しい。
- `float64.min(a: Float64, b: Float64) -> Float64`：小さい方。NaN のオペランドは無視します。-0.0 < 0.0。
- `float64.max(a: Float64, b: Float64) -> Float64`：大きい方。NaN のオペランドは無視します。0.0 > -0.0。
- `float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64`：x を lo..hi に制限します。lo > hi または境界が NaN の場合は中断します。
- `float64.is_nan(x: Float64) -> Bool`：NaN の場合だけ true。
- `float64.is_infinite(x: Float64) -> Bool`：inf と -inf に対して true、NaN に対して false。
- `float64.is_finite(x: Float64) -> Bool`：x が inf、-inf、NaN のいずれでもなければ true。
- `float64.abs(x: Float64) -> Float64`：絶対値。-0.0 は0.0、NaN は NaN のまま。
- `float64.floor(x: Float64) -> Float64`：-inf 方向に丸めます。-0.5 は -1.0。
- `float64.ceil(x: Float64) -> Float64`：+inf 方向に丸めます。-0.5 は -0.0。
- `float64.round(x: Float64) -> Float64`：最も近い整数値。中間値はゼロから離れる方向に丸めます。
- `float64.trunc(x: Float64) -> Float64`：0方向に丸めます。-0.5 は -0.0。
- `float64.signum(x: Float64) -> Float64`：符号ビットに応じて1.0または-1.0（0.0 は1.0、-0.0 は-1.0）。NaN には NaN。
- `float64.to_bits(x: Float64) -> Int`：IEEE-754 binary64 の64ビットを Int として返します。どの NaN も正規 NaN になります。
- `float64.from_bits(bits: Int) -> Float64`：IEEE-754 binary64 のビットが bits である Float64。
- `float64.infinity() -> Float64`：正の無限大。
- `float64.nan() -> Float64`：単一の静かな NaN（ビットは0x7ff8000000000000）。
- `float64.epsilon() -> Float64`：1.0 と次の Float64 の間隔：2^-52。
- `float64.max_value() -> Float64`：最大の有限 Float64。約1.7976931348623157e308。
- `float64.min_value() -> Float64`：最も負の有限 Float64（= -max_value()）。最小の正数ではありません。
- `float64.parse(s: String) -> Result[Float64, String]`：s の前後の ASCII 空白を除去して Float64 にします。構文は float.parse と同じです。不正な文字、inf、NaN は err。
- `float64.from(x: S) -> Float64`：x を最も近い Float64 にします（中間値は偶数へ）。NaN は NaN のまま。
- `float64.from_checked(x: S) -> Option[Float64]`：Float64 が x を正確に保持できれば some(x as Float64)、それ以外（丸めが生じる値、NaN）は none。
