# uint64

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint64.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](uint64.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](uint64.zh-CN.md)



> 原文：[docs/stdlib/uint64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint64.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されない。コード・署名・コメント・版注記は原文保持し、API 説明訳を外に添える。翻訳時に独立した実測・実装検証は行っていない。

> 原文の変換関数群の導入は数値ソース全般を述べるが、from_wrapping の表・索引には整数ソースのみという限定がある。個別関数の条件を優先して読むこと。この訳では限定を維持する。

`UInt64`（64 ビット符号なし、0 … 18,446,744,073,709,551,615）：上下限、算術、ビット操作、テキスト、この型への変換。自動 import され、`import` は不要。

## 変換

変換は生成先の型にちなんで命名する（R-L0-40）。`uint64.from(x)` と同系統の関数は、`Int`、幅付き整数、`Float`、幅付き浮動小数点数という任意の数値ソースを受け取る。

```almd
let x: UInt64 = uint64.from(12)
let wide = int.from(x)                 // leaving UInt64: the destination's `from`
let low = uint64.from_wrapping(-1)     // 18446744073709551615: the low 64 bits, two's complement
let fits = uint64.from_checked(-1)     // none: -1 is not a UInt64
let top = uint64.from_saturating(float.infinity()) // 18446744073709551615: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `uint64.from(x)` | x をゼロ方向へ丸める。NaN、無限大、UInt64 の範囲外の値は abort |
| `uint64.from_checked(x)` | UInt64 が x を正確に保持できれば `some`、それ以外は `none`（小数部分、NaN、範囲外） |
| `uint64.from_saturating(x)` | x をゼロ方向へ丸め、UInt64 の範囲に収める。NaN は 0 |
| `uint64.from_wrapping(x)` | 整数ソースのみ。下位 64 ビット、2 の補数 |

`UInt64` から別の型へ変換するには、`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` など変換先の関数群を呼ぶ。変換先が元の型のすべての値を保持できるなら `from` が適切である。狭い変換先では、abort、`none`、範囲制限、折り返しを名前で選び、暗黙には行わない。

> **`Int` を取る関数へ渡す前に `Int` へ変換すること。** 幅付きの値は暗黙に拡張されず、checker は型の不一致を拒否する。両ターゲットで `int.from(x)` を使い、明示的に変換を要求する。範囲外の値は abort する（checked、saturating、wrapping が意図した方針なら、その変種を使う）。

`min_value()` / `max_value()` がこの型の上下限を与える。正確な公開面は下記の機械管理されたシグネチャ索引にある。

> 以下は原文の生成索引の固定コピーであり、この訳は生成対象ではない。

## シグネチャ索引（35 関数）

```
// Decimal digits, no sign; exact up to 2^64-1.
// @since 0.15.0 or earlier
uint64.to_string(x: UInt64) -> String

// Smallest UInt64: 0.
// @since 0.38.0
uint64.min_value() -> UInt64

// Largest UInt64: 2^64-1.
// @since 0.38.0
uint64.max_value() -> UInt64

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint64.compare(a: UInt64, b: UInt64) -> Int

// Lesser of a and b.
// @since unreleased
uint64.min(a: UInt64, b: UInt64) -> UInt64

// Greater of a and b.
// @since unreleased
uint64.max(a: UInt64, b: UInt64) -> UInt64

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint64.floor_div(a: UInt64, b: UInt64) -> UInt64

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint64.modulo(a: UInt64, b: UInt64) -> UInt64

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
uint64.pow(base: UInt64, exp: Int) -> UInt64

// True when x is a multiple of 2.
// @since unreleased
uint64.is_even(x: UInt64) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint64.is_odd(x: UInt64) -> Bool

// Bitwise AND.
// @since unreleased
uint64.band(a: UInt64, b: UInt64) -> UInt64

// Bitwise OR.
// @since unreleased
uint64.bor(a: UInt64, b: UInt64) -> UInt64

// Bitwise XOR.
// @since unreleased
uint64.bxor(a: UInt64, b: UInt64) -> UInt64

// Bitwise complement of all 64 bits.
// @since unreleased
uint64.bnot(x: UInt64) -> UInt64

// a shifted left by n bits; n is taken mod 64.
// @since unreleased
uint64.bshl(a: UInt64, n: Int) -> UInt64

// Zero-filling right shift; n is taken mod 64.
// @since unreleased
uint64.bshr(a: UInt64, n: Int) -> UInt64

// Zero bits above the highest set bit, counted in 64 bits; 64 for 0.
// @since unreleased
uint64.count_leading_zeros(x: UInt64) -> Int

// Zero bits below the lowest set bit; 64 for 0.
// @since unreleased
uint64.count_trailing_zeros(x: UInt64) -> Int

// Number of set bits among the 64.
// @since unreleased
uint64.count_ones(x: UInt64) -> Int

// The 8 bytes in reverse order (endian flip).
// @since unreleased
uint64.byte_swap(x: UInt64) -> UInt64

// a + b wrapped to 64 bits.
// @since unreleased
uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64

// a - b wrapped to 64 bits.
// @since unreleased
uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64

// a * b wrapped to 64 bits.
// @since unreleased
uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since unreleased
uint64.rotate_left(a: UInt64, n: Int) -> UInt64

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since unreleased
uint64.rotate_right(a: UInt64, n: Int) -> UInt64

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint64.bit_length(x: UInt64) -> Int

// Decimal UInt64 from ASCII-trimmed s; err on junk, a sign of -, or above 2^64-1.
// @since unreleased
uint64.parse(s: String) -> Result[UInt64, String]

// Digits of x in radix, lowercase, no sign; exact up to 2^64-1; radix outside 2..36 aborts.
// @since unreleased
uint64.to_string_radix(x: UInt64, radix: Int) -> String

// UInt64 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 2^64-1.
// @since unreleased
uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]

// x as UInt64, toward zero; NaN, an infinity or a value outside UInt64 aborts.
// @since unreleased
uint64.from(x: S) -> UInt64

// some(x as UInt64) when UInt64 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint64.from_checked(x: S) -> Option[UInt64]

// x as UInt64, toward zero, clamped to UInt64's range; NaN is 0.
// @since unreleased
uint64.from_saturating(x: S) -> UInt64

// The low 64 bits of integer x as UInt64 (two's complement).
// @since unreleased
uint64.from_wrapping(x: S) -> UInt64
```

## API コメントの説明訳

- `uint64.to_string(x: UInt64) -> String` — 符号なしの 10 進数字。2^64-1 まで正確。
- `uint64.min_value() -> UInt64` — 最小の UInt64：0。
- `uint64.max_value() -> UInt64` — 最大の UInt64：2^64-1。
- `uint64.compare(a: UInt64, b: UInt64) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint64.min(a: UInt64, b: UInt64) -> UInt64` — a と b の小さい方。
- `uint64.max(a: UInt64, b: UInt64) -> UInt64` — a と b の大きい方。
- `uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` — x を lo..hi に収める。lo > hi なら abort。
- `uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint64.modulo(a: UInt64, b: UInt64) -> UInt64` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint64.pow(base: UInt64, exp: Int) -> UInt64` — base の exp 乗。* と同様に 64 ビットへ折り返す。exp < 0 なら abort。
- `uint64.is_even(x: UInt64) -> Bool` — x が 2 の倍数なら true。
- `uint64.is_odd(x: UInt64) -> Bool` — x が 2 の倍数でなければ true。
- `uint64.band(a: UInt64, b: UInt64) -> UInt64` — ビット単位 AND。
- `uint64.bor(a: UInt64, b: UInt64) -> UInt64` — ビット単位 OR。
- `uint64.bxor(a: UInt64, b: UInt64) -> UInt64` — ビット単位 XOR。
- `uint64.bnot(x: UInt64) -> UInt64` — 64 ビットすべてをビット反転。
- `uint64.bshl(a: UInt64, n: Int) -> UInt64` — a を n ビット左シフト。n は 64 を法とする。
- `uint64.bshr(a: UInt64, n: Int) -> UInt64` — ゼロ埋め右シフト。n は 64 を法とする。
- `uint64.count_leading_zeros(x: UInt64) -> Int` — 64 ビット中の最上位の立ったビットより上のゼロ数。0 なら 64。
- `uint64.count_trailing_zeros(x: UInt64) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 64。
- `uint64.count_ones(x: UInt64) -> Int` — 64 ビット中の立っているビット数。
- `uint64.byte_swap(x: UInt64) -> UInt64` — 8 バイトを逆順にする（エンディアン反転）。
- `uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` — a + b を 64 ビットへ折り返す。
- `uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` — a - b を 64 ビットへ折り返す。
- `uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` — a * b を 64 ビットへ折り返す。
- `uint64.rotate_left(a: UInt64, n: Int) -> UInt64` — a の 64 ビットを n だけ左回転。n は 64 を法とする。
- `uint64.rotate_right(a: UInt64, n: Int) -> UInt64` — a の 64 ビットを n だけ右回転。n は 64 を法とする。
- `uint64.bit_length(x: UInt64) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint64.parse(s: String) -> Result[UInt64, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt64 を解析。不正内容、- 符号、2^64-1 超なら err。
- `uint64.to_string_radix(x: UInt64, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。2^64-1 まで正確。radix が 2..36 外なら abort。
- `uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` — 前後の空白を除いた s を radix 進の UInt64 として解析。文法は int.parse_radix。不正内容、- 符号、2^64-1 超なら err。
- `uint64.from(x: S) -> UInt64` — x をゼロ方向に丸め UInt64 にする。NaN、無限大、UInt64 範囲外なら abort。
- `uint64.from_checked(x: S) -> Option[UInt64]` — UInt64 が x を正確に保持できれば some(x as UInt64)、それ以外（小数部分、NaN、範囲外）は none。
- `uint64.from_saturating(x: S) -> UInt64` — x をゼロ方向に丸め UInt64 にし、UInt64 の範囲に収める。NaN は 0。
- `uint64.from_wrapping(x: S) -> UInt64` — 整数 x の下位 64 ビットを UInt64 とする（2 の補数）。
