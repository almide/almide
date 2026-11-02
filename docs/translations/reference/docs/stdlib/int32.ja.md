# int32

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int32.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](int32.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](int32.zh-CN.md)



> 原文：[int32.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int32.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

`Int32`（32ビット符号付き）：境界、算術演算、ビット演算、テキスト、この型への変換。自動インポートされるため `import` は不要です。

<a id="conversions"></a>

## 変換

変換は生成する型にちなんで命名します（R-L0-40）。`int32.from(x)` と同系列の関数は任意の数値入力、つまり `Int`、ビット幅を持つ整数、`Float`、ビット幅を持つ浮動小数点数を受け取ります。

```almd
let x: Int32 = int32.from(12)
let wide = int.from(x)                 // leaving Int32: the destination's `from`
let low = int32.from_wrapping(2147483692)// -2147483604: the low 32 bits, two's complement
let fits = int32.from_checked(2147483648)// none: 2147483648 is not an Int32
let top = int32.from_saturating(float.infinity()) // 2147483647: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `int32.from(x)` | x をゼロ方向に丸めます。NaN、無限大、Int32 の範囲外の値は中断 |
| `int32.from_checked(x)` | Int32 が x を正確に保持できれば `some`、それ以外（小数、NaN、範囲外）は `none` |
| `int32.from_saturating(x)` | x をゼロ方向に丸め、Int32 の範囲に制限。NaN は0 |
| `int32.from_wrapping(x)` | 整数入力のみ。下位32ビット、2の補数 |

`Int32` から別の型にするには、変換先の関数群、たとえば `int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` などを呼びます。変換先が変換元のすべての値を保持できる場合は `from` が適切です。狭い範囲への変換では中断、`none`、範囲制限、折り返しを名前で選び、暗黙には行いません。

> **`Int` を受け取る関数へ渡す前に `Int` に変換してください。** ビット幅を持つ値は暗黙には拡張されず、検査器は型の不一致を拒否します。両ターゲットで `int.from(x)` を使い、明示的に変換を要求します。範囲外の値は中断します（意図する方針に応じて checked、saturating、wrapping 版を使ってください）。

ビット幅を持つ型のリテラルはコンパイル時に範囲検査されます。`let a: Int32 = 3000000000` は暗黙の折り畳みではなく E024 エラーになります。

`min_value()` / `max_value()` がこの型の境界を返します。正確な API 一覧は、以下に保持した原文の機械管理署名索引です（この訳は固定スナップショットであり、機械管理の対象ではありません）。

<a id="signature-index-37-functions"></a>

## 原文固定スナップショットの API 索引（37関数）

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

## API 説明の訳（上記署名順）

- `int32.to_string(x: Int32) -> String`：10進数字。負の場合は - を付けます。
- `int32.min_value() -> Int32`：最小の Int32：-2147483648。
- `int32.max_value() -> Int32`：最大の Int32：2147483647。
- `int32.compare(a: Int32, b: Int32) -> Int`：a が b より小さい、等しい、大きい順に -1、0、1。
- `int32.min(a: Int32, b: Int32) -> Int32`：a と b の小さい方。
- `int32.max(a: Int32, b: Int32) -> Int32`：a と b の大きい方。
- `int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32`：x を lo..hi に制限します。lo > hi のとき中断します。
- `int32.floor_div(a: Int32, b: Int32) -> Int32`：商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int32.modulo(a: Int32, b: Int32) -> Int32`：b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int32.pow(base: Int32, exp: Int) -> Int32`：base の exp 乗。* と同様に32ビットに折り返します。exp < 0 は中断します。
- `int32.is_even(x: Int32) -> Bool`：x が2の倍数なら true。
- `int32.is_odd(x: Int32) -> Bool`：x が2の倍数でなければ true。
- `int32.abs(x: Int32) -> Int32`：x の大きさ。min_value は折り返して自身になります。
- `int32.sign(x: Int32) -> Int`：x の符号に応じて -1、0、1。
- `int32.band(a: Int32, b: Int32) -> Int32`：ビット単位 AND。
- `int32.bor(a: Int32, b: Int32) -> Int32`：ビット単位 OR。
- `int32.bxor(a: Int32, b: Int32) -> Int32`：ビット単位 XOR。
- `int32.bnot(x: Int32) -> Int32`：32ビットすべてをビット反転します。
- `int32.bshl(a: Int32, n: Int) -> Int32`：a を n ビット左シフトします。n は32を法とします。
- `int32.bshr(a: Int32, n: Int) -> Int32`：符号埋めの右シフト。n は32を法とします。
- `int32.count_leading_zeros(x: Int32) -> Int`：32ビット内で、最上位の立っているビットより上のゼロビット数。0 の場合は32。
- `int32.count_trailing_zeros(x: Int32) -> Int`：最下位の立っているビットより下のゼロビット数。0 の場合は32。
- `int32.count_ones(x: Int32) -> Int`：32ビット中の立っているビット数。
- `int32.byte_swap(x: Int32) -> Int32`：4バイトの順序を反転します（エンディアン反転）。
- `int32.wrap_add(a: Int32, b: Int32) -> Int32`：a + b を32ビットに折り返します。
- `int32.wrap_sub(a: Int32, b: Int32) -> Int32`：a - b を32ビットに折り返します。
- `int32.wrap_mul(a: Int32, b: Int32) -> Int32`：a * b を32ビットに折り返します。
- `int32.rotate_left(a: Int32, n: Int) -> Int32`：a の32ビットを左に n 回転します。n は32を法とします。
- `int32.rotate_right(a: Int32, n: Int) -> Int32`：a の32ビットを右に n 回転します。n は32を法とします。
- `int32.bit_length(x: Int32) -> Int`：x を格納するのに必要なビット数。0 は0、負数は32。
- `int32.parse(s: String) -> Result[Int32, String]`：s の前後の ASCII 空白を除去して10進 Int32 にします。不正な文字や -2147483648..2147483647 の範囲外は err。
- `int32.to_string_radix(x: Int32, radix: Int) -> String`：x を radix 進の小文字数字で表し、負数には - を付けます。radix が2..36の範囲外なら中断します。
- `int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]`：前後の空白を除去した s を radix 進の Int32 にします。構文は int.parse_radix と同じです。不正な文字や -2147483648..2147483647 の範囲外は err。
- `int32.from(x: S) -> Int32`：x をゼロ方向に丸めて Int32 にします。NaN、無限大、Int32 の範囲外の値は中断します。
- `int32.from_checked(x: S) -> Option[Int32]`：Int32 が x を正確に保持できれば some(x as Int32)、それ以外（小数、NaN、範囲外）は none。
- `int32.from_saturating(x: S) -> Int32`：x をゼロ方向に丸めて Int32 にし、Int32 の範囲に制限します。NaN は0。
- `int32.from_wrapping(x: S) -> Int32`：整数 x の下位32ビットを Int32 として扱います（2の補数）。
