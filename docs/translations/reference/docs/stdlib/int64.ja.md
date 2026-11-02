# int64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](int64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](int64.zh-CN.md)



> 原文：[docs/stdlib/int64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int64.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されない。コード・署名・コメント・版注記は原文保持し、API 説明訳を外に添える。翻訳時に独立した実測・実装検証は行っていない。

> 原文の変換関数群の導入は数値ソース全般を述べるが、from_wrapping の表・索引には整数ソースのみという限定がある。個別関数の条件を優先して読むこと。この訳では限定を維持する。

`Int64`（64 ビット符号付き整数）：上下限、算術、ビット操作、テキスト、この型への変換。自動 import され、`import` は不要。

`Int64` と正準の `Int` は表現を共有するが別の型である。そのため `Int64` にも `int32` と同じ理由で独自のモジュールがあり、その型の上下限、算術、テキスト、変換を名前に対応する場所へ置く。

<a id="conversions"></a>

## 変換

変換は生成先の型にちなんで命名する（R-L0-40）。`int64.from(x)` と同系統の関数は、`Int`、幅付き整数、`Float`、幅付き浮動小数点数という任意の数値ソースを受け取る。

```almd
let x: Int64 = int64.from(12)
let wide = int.from(x)                 // leaving Int64: the destination's `from`
let same = int64.from_wrapping(-1)     // -1: Int is already 64 bits wide
let fits = int64.from_checked(0.5)     // none: 0.5 has a fraction
let top = int64.from_saturating(float.infinity()) // 9223372036854775807: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `int64.from(x)` | x をゼロ方向へ丸める。NaN、無限大、Int64 の範囲外の値は abort |
| `int64.from_checked(x)` | Int64 が x を正確に保持できれば `some`、それ以外は `none`（小数部分、NaN、範囲外） |
| `int64.from_saturating(x)` | x をゼロ方向へ丸め、Int64 の範囲に収める。NaN は 0 |
| `int64.from_wrapping(x)` | 整数ソースのみ。下位 64 ビット、2 の補数 |

`Int64` から別の型へ変換するには、`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` など変換先の関数群を呼ぶ。変換先が元の型のすべての値を保持できるなら `from` が適切である。狭い変換先では、abort、`none`、範囲制限、折り返しを名前で選び、暗黙には行わない。

> **`Int` を取る関数へ渡す前に `Int` へ変換すること。** 幅付きの値は暗黙に拡張されず、checker は型の不一致を拒否する。両ターゲットで `int.from(x)` を使い、明示的に変換を要求する。範囲外の値は abort する（checked、saturating、wrapping が意図した方針なら、その変種を使う）。

`min_value()` / `max_value()` がこの型の上下限を与える。正確な公開面は下記の機械管理されたシグネチャ索引にある。

> 以下は原文の生成索引の固定コピーであり、この訳は生成対象ではない。

<a id="signature-index-37-functions"></a>

## シグネチャ索引（37 関数）

```
// Decimal digits, with - when negative.
// @since 0.37.0 or earlier
int64.to_string(x: Int64) -> String

// Smallest Int64: -2^63.
// @since 0.38.0
int64.min_value() -> Int64

// Largest Int64: 2^63-1.
// @since 0.38.0
int64.max_value() -> Int64

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int64.compare(a: Int64, b: Int64) -> Int

// Lesser of a and b.
// @since unreleased
int64.min(a: Int64, b: Int64) -> Int64

// Greater of a and b.
// @since unreleased
int64.max(a: Int64, b: Int64) -> Int64

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int64.floor_div(a: Int64, b: Int64) -> Int64

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int64.modulo(a: Int64, b: Int64) -> Int64

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
int64.pow(base: Int64, exp: Int) -> Int64

// True when x is a multiple of 2.
// @since unreleased
int64.is_even(x: Int64) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int64.is_odd(x: Int64) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int64.abs(x: Int64) -> Int64

// -1, 0 or 1 by the sign of x.
// @since unreleased
int64.sign(x: Int64) -> Int

// Bitwise AND.
// @since unreleased
int64.band(a: Int64, b: Int64) -> Int64

// Bitwise OR.
// @since unreleased
int64.bor(a: Int64, b: Int64) -> Int64

// Bitwise XOR.
// @since unreleased
int64.bxor(a: Int64, b: Int64) -> Int64

// Bitwise complement of all 64 bits.
// @since unreleased
int64.bnot(x: Int64) -> Int64

// a shifted left by n bits; n is taken mod 64.
// @since unreleased
int64.bshl(a: Int64, n: Int) -> Int64

// Sign-filling right shift; n is taken mod 64.
// @since unreleased
int64.bshr(a: Int64, n: Int) -> Int64

// Zero bits above the highest set bit, counted in 64 bits; 64 for 0.
// @since unreleased
int64.count_leading_zeros(x: Int64) -> Int

// Zero bits below the lowest set bit; 64 for 0.
// @since unreleased
int64.count_trailing_zeros(x: Int64) -> Int

// Number of set bits among the 64.
// @since unreleased
int64.count_ones(x: Int64) -> Int

// The 8 bytes in reverse order (endian flip).
// @since unreleased
int64.byte_swap(x: Int64) -> Int64

// a + b wrapped to 64 bits.
// @since unreleased
int64.wrap_add(a: Int64, b: Int64) -> Int64

// a - b wrapped to 64 bits.
// @since unreleased
int64.wrap_sub(a: Int64, b: Int64) -> Int64

// a * b wrapped to 64 bits.
// @since unreleased
int64.wrap_mul(a: Int64, b: Int64) -> Int64

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since unreleased
int64.rotate_left(a: Int64, n: Int) -> Int64

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since unreleased
int64.rotate_right(a: Int64, n: Int) -> Int64

// Bits needed to hold x; 0 for 0, 64 if negative.
// @since unreleased
int64.bit_length(x: Int64) -> Int

// Decimal Int64 from ASCII-trimmed s; err on junk or overflow.
// @since unreleased
int64.parse(s: String) -> Result[Int64, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int64.to_string_radix(x: Int64, radix: Int) -> String

// Int64 from trimmed s in radix, the grammar of int.parse_radix; err on junk or overflow.
// @since unreleased
int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]

// x as Int64, toward zero; NaN, an infinity or a value outside Int64 aborts.
// @since unreleased
int64.from(x: S) -> Int64

// some(x as Int64) when Int64 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int64.from_checked(x: S) -> Option[Int64]

// x as Int64, toward zero, clamped to Int64's range; NaN is 0.
// @since unreleased
int64.from_saturating(x: S) -> Int64

// The low 64 bits of integer x as Int64 (two's complement).
// @since unreleased
int64.from_wrapping(x: S) -> Int64
```

## API コメントの説明訳

- `int64.to_string(x: Int64) -> String` — 10 進数字。負なら - を付ける。
- `int64.min_value() -> Int64` — 最小の Int64：-2^63。
- `int64.max_value() -> Int64` — 最大の Int64：2^63-1。
- `int64.compare(a: Int64, b: Int64) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `int64.min(a: Int64, b: Int64) -> Int64` — a と b の小さい方。
- `int64.max(a: Int64, b: Int64) -> Int64` — a と b の大きい方。
- `int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` — x を lo..hi に収める。lo > hi なら abort。
- `int64.floor_div(a: Int64, b: Int64) -> Int64` — 商を -inf 方向へ丸める。/ と同様、b == 0 と min_value / -1 は abort。
- `int64.modulo(a: Int64, b: Int64) -> Int64` — b と同じ符号の剰余。a == floor_div(a, b) * b + modulo(a, b) を満たす。b == 0 なら abort。
- `int64.pow(base: Int64, exp: Int) -> Int64` — base の exp 乗。* と同様に 64 ビットへ折り返す。exp < 0 なら abort。
- `int64.is_even(x: Int64) -> Bool` — x が 2 の倍数なら true。
- `int64.is_odd(x: Int64) -> Bool` — x が 2 の倍数でなければ true。
- `int64.abs(x: Int64) -> Int64` — x の大きさ。min_value は自身へ折り返す。
- `int64.sign(x: Int64) -> Int` — x の符号に応じ -1、0、1。
- `int64.band(a: Int64, b: Int64) -> Int64` — ビット単位 AND。
- `int64.bor(a: Int64, b: Int64) -> Int64` — ビット単位 OR。
- `int64.bxor(a: Int64, b: Int64) -> Int64` — ビット単位 XOR。
- `int64.bnot(x: Int64) -> Int64` — 64 ビットすべてをビット反転。
- `int64.bshl(a: Int64, n: Int) -> Int64` — a を n ビット左シフト。n は 64 を法とする。
- `int64.bshr(a: Int64, n: Int) -> Int64` — 符号埋め右シフト。n は 64 を法とする。
- `int64.count_leading_zeros(x: Int64) -> Int` — 64 ビット中の最上位の立ったビットより上のゼロ数。0 なら 64。
- `int64.count_trailing_zeros(x: Int64) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 64。
- `int64.count_ones(x: Int64) -> Int` — 64 ビット中の立っているビット数。
- `int64.byte_swap(x: Int64) -> Int64` — 8 バイトを逆順にする（エンディアン反転）。
- `int64.wrap_add(a: Int64, b: Int64) -> Int64` — a + b を 64 ビットへ折り返す。
- `int64.wrap_sub(a: Int64, b: Int64) -> Int64` — a - b を 64 ビットへ折り返す。
- `int64.wrap_mul(a: Int64, b: Int64) -> Int64` — a * b を 64 ビットへ折り返す。
- `int64.rotate_left(a: Int64, n: Int) -> Int64` — a の 64 ビットを n だけ左回転。n は 64 を法とする。
- `int64.rotate_right(a: Int64, n: Int) -> Int64` — a の 64 ビットを n だけ右回転。n は 64 を法とする。
- `int64.bit_length(x: Int64) -> Int` — x を保持するのに必要なビット数。0 なら 0、負なら 64。
- `int64.parse(s: String) -> Result[Int64, String]` — 前後の ASCII 空白を除いた s から 10 進 Int64 を解析。不正内容や overflow なら err。
- `int64.to_string_radix(x: Int64, radix: Int) -> String` — x を radix 進の小文字数字で表す。負なら - を付ける。radix が 2..36 外なら abort。
- `int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` — 前後の空白を除いた s を radix 進の Int64 として解析。文法は int.parse_radix。不正内容や overflow なら err。
- `int64.from(x: S) -> Int64` — x をゼロ方向に丸め Int64 にする。NaN、無限大、Int64 範囲外なら abort。
- `int64.from_checked(x: S) -> Option[Int64]` — Int64 が x を正確に保持できれば some(x as Int64)、それ以外（小数部分、NaN、範囲外）は none。
- `int64.from_saturating(x: S) -> Int64` — x をゼロ方向に丸め Int64 にし、Int64 の範囲に収める。NaN は 0。
- `int64.from_wrapping(x: S) -> Int64` — 整数 x の下位 64 ビットを Int64 とする（2 の補数）。
