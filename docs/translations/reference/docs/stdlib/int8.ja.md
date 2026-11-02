# int8

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int8.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](int8.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](int8.zh-CN.md)



> 原文：[int8.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int8.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

`Int8`（8ビット符号付き, -128 … 127）：境界、算術演算、ビット演算、テキスト、この型への変換。自動インポートされるため `import` は不要です。

<a id="conversions"></a>

## 変換

変換は生成する型にちなんで命名します（R-L0-40）。`int8.from(x)` と同系列の関数は任意の数値入力、つまり `Int`、ビット幅を持つ整数、`Float`、ビット幅を持つ浮動小数点数を受け取ります。

```almd
let x: Int8 = int8.from(12)
let wide = int.from(x)                 // leaving Int8: the destination's `from`
let low = int8.from_wrapping(172)      // -84: the low 8 bits, two's complement
let fits = int8.from_checked(128)      // none: 128 is not an Int8
let top = int8.from_saturating(float.infinity()) // 127: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `int8.from(x)` | x をゼロ方向に丸めます。NaN、無限大、Int8 の範囲外の値は中断 |
| `int8.from_checked(x)` | Int8 が x を正確に保持できれば `some`、それ以外（小数、NaN、範囲外）は `none` |
| `int8.from_saturating(x)` | x をゼロ方向に丸め、Int8 の範囲に制限。NaN は0 |
| `int8.from_wrapping(x)` | 整数入力のみ。下位8ビット、2の補数 |

`Int8` から別の型にするには、変換先の関数群、たとえば `int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` などを呼びます。変換先が変換元のすべての値を保持できる場合は `from` が適切です。狭い範囲への変換では中断、`none`、範囲制限、折り返しを名前で選び、暗黙には行いません。

> **`Int` を受け取る関数へ渡す前に `Int` に変換してください。** ビット幅を持つ値は暗黙には拡張されず、検査器は型の不一致を拒否します。両ターゲットで `int.from(x)` を使い、明示的に変換を要求します。範囲外の値は中断します（意図する方針に応じて checked、saturating、wrapping 版を使ってください）。

ビット幅を持つ型のリテラルはコンパイル時に範囲検査されます。`let a: Int8 = 200` は暗黙の折り畳みではなく E024 エラーになります。

`min_value()` / `max_value()` がこの型の境界を返します。正確な API 一覧は、以下に保持した原文の機械管理署名索引です（この訳は固定スナップショットであり、機械管理の対象ではありません）。

<a id="signature-index-37-functions"></a>

## 原文固定スナップショットの API 索引（37関数）

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

## API 説明の訳（上記署名順）

- `int8.to_string(x: Int8) -> String`：10進数字。負の場合は - を付けます。
- `int8.min_value() -> Int8`：最小の Int8：-128。
- `int8.max_value() -> Int8`：最大の Int8：127。
- `int8.compare(a: Int8, b: Int8) -> Int`：a が b より小さい、等しい、大きい順に -1、0、1。
- `int8.min(a: Int8, b: Int8) -> Int8`：a と b の小さい方。
- `int8.max(a: Int8, b: Int8) -> Int8`：a と b の大きい方。
- `int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8`：x を lo..hi に制限します。lo > hi のとき中断します。
- `int8.floor_div(a: Int8, b: Int8) -> Int8`：商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int8.modulo(a: Int8, b: Int8) -> Int8`：b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int8.pow(base: Int8, exp: Int) -> Int8`：base の exp 乗。* と同様に8ビットに折り返します。exp < 0 は中断します。
- `int8.is_even(x: Int8) -> Bool`：x が2の倍数なら true。
- `int8.is_odd(x: Int8) -> Bool`：x が2の倍数でなければ true。
- `int8.abs(x: Int8) -> Int8`：x の大きさ。min_value は折り返して自身になります。
- `int8.sign(x: Int8) -> Int`：x の符号に応じて -1、0、1。
- `int8.band(a: Int8, b: Int8) -> Int8`：ビット単位 AND。
- `int8.bor(a: Int8, b: Int8) -> Int8`：ビット単位 OR。
- `int8.bxor(a: Int8, b: Int8) -> Int8`：ビット単位 XOR。
- `int8.bnot(x: Int8) -> Int8`：8ビットすべてをビット反転します。
- `int8.bshl(a: Int8, n: Int) -> Int8`：a を n ビット左シフトします。n は8を法とします。
- `int8.bshr(a: Int8, n: Int) -> Int8`：符号埋めの右シフト。n は8を法とします。
- `int8.count_leading_zeros(x: Int8) -> Int`：8ビット内で、最上位の立っているビットより上のゼロビット数。0 の場合は8。
- `int8.count_trailing_zeros(x: Int8) -> Int`：最下位の立っているビットより下のゼロビット数。0 の場合は8。
- `int8.count_ones(x: Int8) -> Int`：8ビット中の立っているビット数。
- `int8.byte_swap(x: Int8) -> Int8`：バイト順を反転します。1バイトなので x 自身です。
- `int8.wrap_add(a: Int8, b: Int8) -> Int8`：a + b を8ビットに折り返します。
- `int8.wrap_sub(a: Int8, b: Int8) -> Int8`：a - b を8ビットに折り返します。
- `int8.wrap_mul(a: Int8, b: Int8) -> Int8`：a * b を8ビットに折り返します。
- `int8.rotate_left(a: Int8, n: Int) -> Int8`：a の8ビットを左に n 回転します。n は8を法とします。
- `int8.rotate_right(a: Int8, n: Int) -> Int8`：a の8ビットを右に n 回転します。n は8を法とします。
- `int8.bit_length(x: Int8) -> Int`：x を格納するのに必要なビット数。0 は0、負数は8。
- `int8.parse(s: String) -> Result[Int8, String]`：s の前後の ASCII 空白を除去して10進 Int8 にします。不正な文字や -128..127 の範囲外は err。
- `int8.to_string_radix(x: Int8, radix: Int) -> String`：x を radix 進の小文字数字で表し、負数には - を付けます。radix が2..36の範囲外なら中断します。
- `int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]`：前後の空白を除去した s を radix 進の Int8 にします。構文は int.parse_radix と同じです。不正な文字や -128..127 の範囲外は err。
- `int8.from(x: S) -> Int8`：x をゼロ方向に丸めて Int8 にします。NaN、無限大、Int8 の範囲外の値は中断します。
- `int8.from_checked(x: S) -> Option[Int8]`：Int8 が x を正確に保持できれば some(x as Int8)、それ以外（小数、NaN、範囲外）は none。
- `int8.from_saturating(x: S) -> Int8`：x をゼロ方向に丸めて Int8 にし、Int8 の範囲に制限します。NaN は0。
- `int8.from_wrapping(x: S) -> Int8`：整数 x の下位8ビットを Int8 として扱います（2の補数）。
