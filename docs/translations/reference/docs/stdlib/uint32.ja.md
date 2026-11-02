# uint32

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint32.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](uint32.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](uint32.zh-CN.md)



> 原文：[docs/stdlib/uint32.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint32.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されない。コード・署名・コメント・版注記は原文保持し、API 説明訳を外に添える。翻訳時に独立した実測・実装検証は行っていない。

> 原文の変換関数群の導入は数値ソース全般を述べるが、from_wrapping の表・索引には整数ソースのみという限定がある。個別関数の条件を優先して読むこと。この訳では限定を維持する。

`UInt32`（32 ビット符号なし、0 … 4,294,967,295）：上下限、算術、ビット操作、テキスト、この型への変換。自動 import され、`import` は不要。

<a id="conversions"></a>

## 変換

変換は生成先の型にちなんで命名する（R-L0-40）。`uint32.from(x)` と同系統の関数は、`Int`、幅付き整数、`Float`、幅付き浮動小数点数という任意の数値ソースを受け取る。

```almd
let x: UInt32 = uint32.from(12)
let wide = int.from(x)                 // leaving UInt32: the destination's `from`
let low = uint32.from_wrapping(4294967340)// 44: the low 32 bits, two's complement
let fits = uint32.from_checked(4294967296)// none: 4294967296 is not a UInt32
let top = uint32.from_saturating(float.infinity()) // 4294967295: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `uint32.from(x)` | x をゼロ方向へ丸める。NaN、無限大、UInt32 の範囲外の値は abort |
| `uint32.from_checked(x)` | UInt32 が x を正確に保持できれば `some`、それ以外は `none`（小数部分、NaN、範囲外） |
| `uint32.from_saturating(x)` | x をゼロ方向へ丸め、UInt32 の範囲に収める。NaN は 0 |
| `uint32.from_wrapping(x)` | 整数ソースのみ。下位 32 ビット、2 の補数 |

`UInt32` から別の型へ変換するには、`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` など変換先の関数群を呼ぶ。変換先が元の型のすべての値を保持できるなら `from` が適切である。狭い変換先では、abort、`none`、範囲制限、折り返しを名前で選び、暗黙には行わない。

> **`Int` を取る関数へ渡す前に `Int` へ変換すること。** 幅付きの値は暗黙に拡張されず、checker は型の不一致を拒否する。両ターゲットで `int.from(x)` を使い、明示的に変換を要求する。範囲外の値は abort する（checked、saturating、wrapping が意図した方針なら、その変種を使う）。

幅付き型のリテラルはコンパイル時に範囲検査する。`let a: UInt32 = 5000000000` は黙って折り返すのではなく E024 エラーになる。

`min_value()` / `max_value()` がこの型の上下限を与える。正確な公開面は下記の機械管理されたシグネチャ索引にある。

> 以下は原文の生成索引の固定コピーであり、この訳は生成対象ではない。

<a id="signature-index-35-functions"></a>

## シグネチャ索引（35 関数）

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint32.to_string(x: UInt32) -> String

// Smallest UInt32: 0.
// @since 0.38.0
uint32.min_value() -> UInt32

// Largest UInt32: 4294967295.
// @since 0.38.0
uint32.max_value() -> UInt32

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint32.compare(a: UInt32, b: UInt32) -> Int

// Lesser of a and b.
// @since unreleased
uint32.min(a: UInt32, b: UInt32) -> UInt32

// Greater of a and b.
// @since unreleased
uint32.max(a: UInt32, b: UInt32) -> UInt32

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint32.floor_div(a: UInt32, b: UInt32) -> UInt32

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint32.modulo(a: UInt32, b: UInt32) -> UInt32

// base raised to exp, wrapping to 32 bits like *; exp < 0 aborts.
// @since unreleased
uint32.pow(base: UInt32, exp: Int) -> UInt32

// True when x is a multiple of 2.
// @since unreleased
uint32.is_even(x: UInt32) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint32.is_odd(x: UInt32) -> Bool

// Bitwise AND.
// @since unreleased
uint32.band(a: UInt32, b: UInt32) -> UInt32

// Bitwise OR.
// @since unreleased
uint32.bor(a: UInt32, b: UInt32) -> UInt32

// Bitwise XOR.
// @since unreleased
uint32.bxor(a: UInt32, b: UInt32) -> UInt32

// Bitwise complement of all 32 bits.
// @since unreleased
uint32.bnot(x: UInt32) -> UInt32

// a shifted left by n bits; n is taken mod 32.
// @since unreleased
uint32.bshl(a: UInt32, n: Int) -> UInt32

// Zero-filling right shift; n is taken mod 32.
// @since unreleased
uint32.bshr(a: UInt32, n: Int) -> UInt32

// Zero bits above the highest set bit, counted in 32 bits; 32 for 0.
// @since unreleased
uint32.count_leading_zeros(x: UInt32) -> Int

// Zero bits below the lowest set bit; 32 for 0.
// @since unreleased
uint32.count_trailing_zeros(x: UInt32) -> Int

// Number of set bits among the 32.
// @since unreleased
uint32.count_ones(x: UInt32) -> Int

// The 4 bytes in reverse order (endian flip).
// @since unreleased
uint32.byte_swap(x: UInt32) -> UInt32

// a + b wrapped to 32 bits.
// @since unreleased
uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32

// a - b wrapped to 32 bits.
// @since unreleased
uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32

// a * b wrapped to 32 bits.
// @since unreleased
uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32

// The 32 bits of a rotated left by n; n is taken mod 32.
// @since unreleased
uint32.rotate_left(a: UInt32, n: Int) -> UInt32

// The 32 bits of a rotated right by n; n is taken mod 32.
// @since unreleased
uint32.rotate_right(a: UInt32, n: Int) -> UInt32

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint32.bit_length(x: UInt32) -> Int

// Decimal UInt32 from ASCII-trimmed s; err on junk, a sign of -, or above 4294967295.
// @since unreleased
uint32.parse(s: String) -> Result[UInt32, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint32.to_string_radix(x: UInt32, radix: Int) -> String

// UInt32 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 4294967295.
// @since unreleased
uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]

// x as UInt32, toward zero; NaN, an infinity or a value outside UInt32 aborts.
// @since unreleased
uint32.from(x: S) -> UInt32

// some(x as UInt32) when UInt32 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint32.from_checked(x: S) -> Option[UInt32]

// x as UInt32, toward zero, clamped to UInt32's range; NaN is 0.
// @since unreleased
uint32.from_saturating(x: S) -> UInt32

// The low 32 bits of integer x as UInt32 (two's complement).
// @since unreleased
uint32.from_wrapping(x: S) -> UInt32
```

## API コメントの説明訳

- `uint32.to_string(x: UInt32) -> String` — 10 進数字。符号は付かない。
- `uint32.min_value() -> UInt32` — 最小の UInt32：0。
- `uint32.max_value() -> UInt32` — 最大の UInt32：4294967295。
- `uint32.compare(a: UInt32, b: UInt32) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint32.min(a: UInt32, b: UInt32) -> UInt32` — a と b の小さい方。
- `uint32.max(a: UInt32, b: UInt32) -> UInt32` — a と b の大きい方。
- `uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` — x を lo..hi に収める。lo > hi なら abort。
- `uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint32.modulo(a: UInt32, b: UInt32) -> UInt32` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint32.pow(base: UInt32, exp: Int) -> UInt32` — base の exp 乗。* と同様に 32 ビットへ折り返す。exp < 0 なら abort。
- `uint32.is_even(x: UInt32) -> Bool` — x が 2 の倍数なら true。
- `uint32.is_odd(x: UInt32) -> Bool` — x が 2 の倍数でなければ true。
- `uint32.band(a: UInt32, b: UInt32) -> UInt32` — ビット単位 AND。
- `uint32.bor(a: UInt32, b: UInt32) -> UInt32` — ビット単位 OR。
- `uint32.bxor(a: UInt32, b: UInt32) -> UInt32` — ビット単位 XOR。
- `uint32.bnot(x: UInt32) -> UInt32` — 32 ビットすべてをビット反転。
- `uint32.bshl(a: UInt32, n: Int) -> UInt32` — a を n ビット左シフト。n は 32 を法とする。
- `uint32.bshr(a: UInt32, n: Int) -> UInt32` — ゼロ埋め右シフト。n は 32 を法とする。
- `uint32.count_leading_zeros(x: UInt32) -> Int` — 32 ビット中の最上位の立ったビットより上のゼロ数。0 なら 32。
- `uint32.count_trailing_zeros(x: UInt32) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 32。
- `uint32.count_ones(x: UInt32) -> Int` — 32 ビット中の立っているビット数。
- `uint32.byte_swap(x: UInt32) -> UInt32` — 4 バイトを逆順にする（エンディアン反転）。
- `uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` — a + b を 32 ビットへ折り返す。
- `uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` — a - b を 32 ビットへ折り返す。
- `uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` — a * b を 32 ビットへ折り返す。
- `uint32.rotate_left(a: UInt32, n: Int) -> UInt32` — a の 32 ビットを n だけ左回転。n は 32 を法とする。
- `uint32.rotate_right(a: UInt32, n: Int) -> UInt32` — a の 32 ビットを n だけ右回転。n は 32 を法とする。
- `uint32.bit_length(x: UInt32) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint32.parse(s: String) -> Result[UInt32, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt32 を解析。不正内容、- 符号、4294967295 超なら err。
- `uint32.to_string_radix(x: UInt32, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。radix が 2..36 外なら abort。
- `uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` — 前後の空白を除いた s を radix 進の UInt32 として解析。文法は int.parse_radix。不正内容、- 符号、4294967295 超なら err。
- `uint32.from(x: S) -> UInt32` — x をゼロ方向に丸め UInt32 にする。NaN、無限大、UInt32 範囲外なら abort。
- `uint32.from_checked(x: S) -> Option[UInt32]` — UInt32 が x を正確に保持できれば some(x as UInt32)、それ以外（小数部分、NaN、範囲外）は none。
- `uint32.from_saturating(x: S) -> UInt32` — x をゼロ方向に丸め UInt32 にし、UInt32 の範囲に収める。NaN は 0。
- `uint32.from_wrapping(x: S) -> UInt32` — 整数 x の下位 32 ビットを UInt32 とする（2 の補数）。
