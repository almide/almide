# uint8

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint8.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](uint8.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](uint8.zh-CN.md)



> 原文：[docs/stdlib/uint8.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint8.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されない。コード・署名・コメント・版注記は原文保持し、API 説明訳を外に添える。翻訳時に独立した実測・実装検証は行っていない。

> 原文の変換関数群の導入は数値ソース全般を述べるが、from_wrapping の表・索引には整数ソースのみという限定がある。個別関数の条件を優先して読むこと。この訳では限定を維持する。

`UInt8`（8 ビット符号なし、0 … 255）：上下限、算術、ビット操作、テキスト、この型への変換。自動 import され、`import` は不要。

<a id="conversions"></a>

## 変換

変換は生成先の型にちなんで命名する（R-L0-40）。`uint8.from(x)` と同系統の関数は、`Int`、幅付き整数、`Float`、幅付き浮動小数点数という任意の数値ソースを受け取る。

```almd
let x: UInt8 = uint8.from(12)
let wide = int.from(x)                 // leaving UInt8: the destination's `from`
let low = uint8.from_wrapping(300)     // 44: the low 8 bits, two's complement
let fits = uint8.from_checked(256)     // none: 256 is not a UInt8
let top = uint8.from_saturating(float.infinity()) // 255: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `uint8.from(x)` | x をゼロ方向へ丸める。NaN、無限大、UInt8 の範囲外の値は abort |
| `uint8.from_checked(x)` | UInt8 が x を正確に保持できれば `some`、それ以外は `none`（小数部分、NaN、範囲外） |
| `uint8.from_saturating(x)` | x をゼロ方向へ丸め、UInt8 の範囲に収める。NaN は 0 |
| `uint8.from_wrapping(x)` | 整数ソースのみ。下位 8 ビット、2 の補数 |

`UInt8` から別の型へ変換するには、`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` など変換先の関数群を呼ぶ。変換先が元の型のすべての値を保持できるなら `from` が適切である。狭い変換先では、abort、`none`、範囲制限、折り返しを名前で選び、暗黙には行わない。

> **`Int` を取る関数へ渡す前に `Int` へ変換すること。** 幅付きの値は暗黙に拡張されず、checker は型の不一致を拒否する。両ターゲットで `int.from(x)` を使い、明示的に変換を要求する。範囲外の値は abort する（checked、saturating、wrapping が意図した方針なら、その変種を使う）。

幅付き型のリテラルはコンパイル時に範囲検査する。`let a: UInt8 = 256` は黙って折り返すのではなく E024 エラーになる。

`min_value()` / `max_value()` がこの型の上下限を与える。正確な公開面は下記の機械管理されたシグネチャ索引にある。

> 以下は原文の生成索引の固定コピーであり、この訳は生成対象ではない。

<a id="signature-index-35-functions"></a>

## シグネチャ索引（35 関数）

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint8.to_string(x: UInt8) -> String

// Smallest UInt8: 0.
// @since 0.38.0
uint8.min_value() -> UInt8

// Largest UInt8: 255.
// @since 0.38.0
uint8.max_value() -> UInt8

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint8.compare(a: UInt8, b: UInt8) -> Int

// Lesser of a and b.
// @since unreleased
uint8.min(a: UInt8, b: UInt8) -> UInt8

// Greater of a and b.
// @since unreleased
uint8.max(a: UInt8, b: UInt8) -> UInt8

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint8.floor_div(a: UInt8, b: UInt8) -> UInt8

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint8.modulo(a: UInt8, b: UInt8) -> UInt8

// base raised to exp, wrapping to 8 bits like *; exp < 0 aborts.
// @since unreleased
uint8.pow(base: UInt8, exp: Int) -> UInt8

// True when x is a multiple of 2.
// @since unreleased
uint8.is_even(x: UInt8) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint8.is_odd(x: UInt8) -> Bool

// Bitwise AND.
// @since unreleased
uint8.band(a: UInt8, b: UInt8) -> UInt8

// Bitwise OR.
// @since unreleased
uint8.bor(a: UInt8, b: UInt8) -> UInt8

// Bitwise XOR.
// @since unreleased
uint8.bxor(a: UInt8, b: UInt8) -> UInt8

// Bitwise complement of all 8 bits.
// @since unreleased
uint8.bnot(x: UInt8) -> UInt8

// a shifted left by n bits; n is taken mod 8.
// @since unreleased
uint8.bshl(a: UInt8, n: Int) -> UInt8

// Zero-filling right shift; n is taken mod 8.
// @since unreleased
uint8.bshr(a: UInt8, n: Int) -> UInt8

// Zero bits above the highest set bit, counted in 8 bits; 8 for 0.
// @since unreleased
uint8.count_leading_zeros(x: UInt8) -> Int

// Zero bits below the lowest set bit; 8 for 0.
// @since unreleased
uint8.count_trailing_zeros(x: UInt8) -> Int

// Number of set bits among the 8.
// @since unreleased
uint8.count_ones(x: UInt8) -> Int

// The bytes in reverse order; one byte, so x itself.
// @since unreleased
uint8.byte_swap(x: UInt8) -> UInt8

// a + b wrapped to 8 bits.
// @since unreleased
uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8

// a - b wrapped to 8 bits.
// @since unreleased
uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8

// a * b wrapped to 8 bits.
// @since unreleased
uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8

// The 8 bits of a rotated left by n; n is taken mod 8.
// @since unreleased
uint8.rotate_left(a: UInt8, n: Int) -> UInt8

// The 8 bits of a rotated right by n; n is taken mod 8.
// @since unreleased
uint8.rotate_right(a: UInt8, n: Int) -> UInt8

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint8.bit_length(x: UInt8) -> Int

// Decimal UInt8 from ASCII-trimmed s; err on junk, a sign of -, or above 255.
// @since unreleased
uint8.parse(s: String) -> Result[UInt8, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint8.to_string_radix(x: UInt8, radix: Int) -> String

// UInt8 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 255.
// @since unreleased
uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]

// x as UInt8, toward zero; NaN, an infinity or a value outside UInt8 aborts.
// @since unreleased
uint8.from(x: S) -> UInt8

// some(x as UInt8) when UInt8 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint8.from_checked(x: S) -> Option[UInt8]

// x as UInt8, toward zero, clamped to UInt8's range; NaN is 0.
// @since unreleased
uint8.from_saturating(x: S) -> UInt8

// The low 8 bits of integer x as UInt8 (two's complement).
// @since unreleased
uint8.from_wrapping(x: S) -> UInt8
```

## API コメントの説明訳

- `uint8.to_string(x: UInt8) -> String` — 10 進数字。符号は付かない。
- `uint8.min_value() -> UInt8` — 最小の UInt8：0。
- `uint8.max_value() -> UInt8` — 最大の UInt8：255。
- `uint8.compare(a: UInt8, b: UInt8) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint8.min(a: UInt8, b: UInt8) -> UInt8` — a と b の小さい方。
- `uint8.max(a: UInt8, b: UInt8) -> UInt8` — a と b の大きい方。
- `uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` — x を lo..hi に収める。lo > hi なら abort。
- `uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint8.modulo(a: UInt8, b: UInt8) -> UInt8` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint8.pow(base: UInt8, exp: Int) -> UInt8` — base の exp 乗。* と同様に 8 ビットへ折り返す。exp < 0 なら abort。
- `uint8.is_even(x: UInt8) -> Bool` — x が 2 の倍数なら true。
- `uint8.is_odd(x: UInt8) -> Bool` — x が 2 の倍数でなければ true。
- `uint8.band(a: UInt8, b: UInt8) -> UInt8` — ビット単位 AND。
- `uint8.bor(a: UInt8, b: UInt8) -> UInt8` — ビット単位 OR。
- `uint8.bxor(a: UInt8, b: UInt8) -> UInt8` — ビット単位 XOR。
- `uint8.bnot(x: UInt8) -> UInt8` — 8 ビットすべてをビット反転。
- `uint8.bshl(a: UInt8, n: Int) -> UInt8` — a を n ビット左シフト。n は 8 を法とする。
- `uint8.bshr(a: UInt8, n: Int) -> UInt8` — ゼロ埋め右シフト。n は 8 を法とする。
- `uint8.count_leading_zeros(x: UInt8) -> Int` — 8 ビット中の最上位の立ったビットより上のゼロ数。0 なら 8。
- `uint8.count_trailing_zeros(x: UInt8) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 8。
- `uint8.count_ones(x: UInt8) -> Int` — 8 ビット中の立っているビット数。
- `uint8.byte_swap(x: UInt8) -> UInt8` — バイト順を反転。1 バイトなので x 自身。
- `uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` — a + b を 8 ビットへ折り返す。
- `uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` — a - b を 8 ビットへ折り返す。
- `uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` — a * b を 8 ビットへ折り返す。
- `uint8.rotate_left(a: UInt8, n: Int) -> UInt8` — a の 8 ビットを n だけ左回転。n は 8 を法とする。
- `uint8.rotate_right(a: UInt8, n: Int) -> UInt8` — a の 8 ビットを n だけ右回転。n は 8 を法とする。
- `uint8.bit_length(x: UInt8) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint8.parse(s: String) -> Result[UInt8, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt8 を解析。不正内容、- 符号、255 超なら err。
- `uint8.to_string_radix(x: UInt8, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。radix が 2..36 外なら abort。
- `uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` — 前後の空白を除いた s を radix 進の UInt8 として解析。文法は int.parse_radix。不正内容、- 符号、255 超なら err。
- `uint8.from(x: S) -> UInt8` — x をゼロ方向に丸め UInt8 にする。NaN、無限大、UInt8 範囲外なら abort。
- `uint8.from_checked(x: S) -> Option[UInt8]` — UInt8 が x を正確に保持できれば some(x as UInt8)、それ以外（小数部分、NaN、範囲外）は none。
- `uint8.from_saturating(x: S) -> UInt8` — x をゼロ方向に丸め UInt8 にし、UInt8 の範囲に収める。NaN は 0。
- `uint8.from_wrapping(x: S) -> UInt8` — 整数 x の下位 8 ビットを UInt8 とする（2 の補数）。
