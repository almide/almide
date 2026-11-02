# float32

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](float32.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](float32.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](float32.zh-CN.md)



> 原文：[float32.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/float32.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

`Float32`（32ビット浮動小数点、IEEE-754 binary32）：境界、算術演算、ビット、テキスト、この型への変換。自動インポートされるため `import` は不要です。

`Float32` は binary32 の精度を持ち、変換と算術演算の結果はその精度に丸められます。binary32 の有限範囲外への変換はエラーではなく無限大になります。

<a id="conversions"></a>

## 変換

変換は生成する型にちなんで命名します（R-L0-40）。`float32.from(x)` は任意の数値入力、つまり `Int`、ビット幅を持つ整数、`Float`、ビット幅を持つ浮動小数点数を受け取ります。

```almd
let x: Float32 = float32.from(1.5)
let r = float32.from(0.1)              // the nearest Float32, rounded once
let exact = float32.from_checked(0.1)  // none: 0.1 is not a binary32 value
let n = int.from_saturating(x)         // leaving Float32: the destination's family
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `float32.from(x)` | 最も近い Float32（中間値は偶数へ）。NaN は NaN のまま |
| `float32.from_checked(x)` | Float32 が x を正確に保持できれば `some`、それ以外（丸めが生じる値、NaN）は `none` |

`Float32` から別の型にするには、変換先の関数群を呼びます。`float.from(x)` は正確に拡張します。整数への変換は名前で選びます。`int.from(x)` は NaN、無限大、範囲外の値で中断し、`int.from_checked(x)` は小数に `none` を返し、`int.from_saturating(x)` はゼロ方向に切り捨てて範囲内に制限します（NaN は0）。

正確な API 一覧は、以下に保持した原文の機械管理署名索引です（この訳は固定スナップショットであり、機械管理の対象ではありません）。

> オーバーフローで inf になるという記述はこの Float32 原文のものです。Float 全般の parse の挙動へ一般化せず、署名索引にある float.parse の構文参照も原文どおり保持しています。

<a id="signature-index-24-functions"></a>

## 原文固定スナップショットの API 索引（24関数）

```
// Shortest f32 round-trip digits, no exponent; 0.1 stays 0.1, 2.0 keeps .0.
// @since 0.15.0 or earlier
float32.to_string(x: Float32) -> String

// -1, 0 or 1 in IEEE 754 total order: -0.0 < 0.0, NaN above inf, NaN equal to NaN.
// @since unreleased
float32.compare(a: Float32, b: Float32) -> Int

// Lesser; a NaN operand is ignored; -0.0 < 0.0.
// @since unreleased
float32.min(a: Float32, b: Float32) -> Float32

// Greater; a NaN operand is ignored; 0.0 > -0.0.
// @since unreleased
float32.max(a: Float32, b: Float32) -> Float32

// x limited to lo..hi; lo > hi or a NaN bound aborts.
// @since unreleased
float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32

// True only for NaN.
// @since unreleased
float32.is_nan(x: Float32) -> Bool

// True for inf and -inf; false for NaN.
// @since unreleased
float32.is_infinite(x: Float32) -> Bool

// True unless x is inf, -inf or NaN.
// @since unreleased
float32.is_finite(x: Float32) -> Bool

// Magnitude; -0.0 gives 0.0, NaN stays NaN.
// @since unreleased
float32.abs(x: Float32) -> Float32

// Round toward -inf; -0.5 gives -1.0.
// @since unreleased
float32.floor(x: Float32) -> Float32

// Round toward +inf; -0.5 gives -0.0.
// @since unreleased
float32.ceil(x: Float32) -> Float32

// Nearest whole value; halves round away from zero.
// @since unreleased
float32.round(x: Float32) -> Float32

// Round toward 0; -0.5 gives -0.0.
// @since unreleased
float32.trunc(x: Float32) -> Float32

// 1.0 or -1.0 by sign bit (0.0 gives 1.0, -0.0 gives -1.0); NaN for NaN.
// @since unreleased
float32.signum(x: Float32) -> Float32

// The 32 IEEE-754 binary32 bits as a non-negative Int; any NaN gives 0x7fc00000.
// @since unreleased
float32.to_bits(x: Float32) -> Int

// Float32 whose binary32 bits are the low 32 bits of bits.
// @since unreleased
float32.from_bits(bits: Int) -> Float32

// Positive infinity.
// @since unreleased
float32.infinity() -> Float32

// The one quiet NaN (bits 0x7fc00000).
// @since unreleased
float32.nan() -> Float32

// Gap between 1.0 and the next Float32: 2^-23.
// @since unreleased
float32.epsilon() -> Float32

// Largest finite Float32, about 3.4028235e38.
// @since unreleased
float32.max_value() -> Float32

// Most negative finite Float32 (= -max_value()), not the smallest positive.
// @since unreleased
float32.min_value() -> Float32

// Float32 from trimmed s, the grammar of float.parse; err on junk; overflow gives inf.
// @since unreleased
float32.parse(s: String) -> Result[Float32, String]

// x as the nearest Float32 (ties to even), rounded once; NaN stays NaN.
// @since unreleased
float32.from(x: S) -> Float32

// some(x as Float32) when Float32 holds x exactly, else none (a rounded value, NaN).
// @since unreleased
float32.from_checked(x: S) -> Option[Float32]
```

## API 説明の訳（上記署名順）

- `float32.to_string(x: Float32) -> String`：f32 の往復変換可能な最短の数字列。指数表記はなく、0.1 は0.1のまま、2.0 は .0 を保ちます。
- `float32.compare(a: Float32, b: Float32) -> Int`：IEEE 754 全順序で -1、0、1：-0.0 < 0.0、NaN は inf より上で、NaN 同士は等しい。
- `float32.min(a: Float32, b: Float32) -> Float32`：小さい方。NaN のオペランドは無視します。-0.0 < 0.0。
- `float32.max(a: Float32, b: Float32) -> Float32`：大きい方。NaN のオペランドは無視します。0.0 > -0.0。
- `float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32`：x を lo..hi に制限します。lo > hi または境界が NaN の場合は中断します。
- `float32.is_nan(x: Float32) -> Bool`：NaN の場合だけ true。
- `float32.is_infinite(x: Float32) -> Bool`：inf と -inf に対して true、NaN に対して false。
- `float32.is_finite(x: Float32) -> Bool`：x が inf、-inf、NaN のいずれでもなければ true。
- `float32.abs(x: Float32) -> Float32`：絶対値。-0.0 は0.0、NaN は NaN のまま。
- `float32.floor(x: Float32) -> Float32`：-inf 方向に丸めます。-0.5 は -1.0。
- `float32.ceil(x: Float32) -> Float32`：+inf 方向に丸めます。-0.5 は -0.0。
- `float32.round(x: Float32) -> Float32`：最も近い整数値。中間値はゼロから離れる方向に丸めます。
- `float32.trunc(x: Float32) -> Float32`：0方向に丸めます。-0.5 は -0.0。
- `float32.signum(x: Float32) -> Float32`：符号ビットに応じて1.0または-1.0（0.0 は1.0、-0.0 は-1.0）。NaN には NaN。
- `float32.to_bits(x: Float32) -> Int`：IEEE-754 binary32 の32ビットを非負の Int として返します。どの NaN も0x7fc00000になります。
- `float32.from_bits(bits: Int) -> Float32`：bits の下位32ビットを binary32 のビットとする Float32。
- `float32.infinity() -> Float32`：正の無限大。
- `float32.nan() -> Float32`：単一の静かな NaN（ビットは0x7fc00000）。
- `float32.epsilon() -> Float32`：1.0 と次の Float32 の間隔：2^-23。
- `float32.max_value() -> Float32`：最大の有限 Float32。約3.4028235e38。
- `float32.min_value() -> Float32`：最も負の有限 Float32（= -max_value()）。最小の正数ではありません。
- `float32.parse(s: String) -> Result[Float32, String]`：前後の空白を除去した s を Float32 にします。構文は float.parse と同じです。不正な文字は err、オーバーフローは inf。
- `float32.from(x: S) -> Float32`：x を最も近い Float32 に1回丸めます（中間値は偶数へ）。NaN は NaN のまま。
- `float32.from_checked(x: S) -> Option[Float32]`：Float32 が x を正確に保持できれば some(x as Float32)、それ以外（丸めが生じる値、NaN）は none。
