# uint16

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uint16.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](uint16.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](uint16.zh-CN.md)



> 原文：[docs/stdlib/uint16.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uint16.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されない。コード・署名・コメント・版注記は原文保持し、API 説明訳を外に添える。翻訳時に独立した実測・実装検証は行っていない。

> 原文の変換関数群の導入は数値ソース全般を述べるが、from_wrapping の表・索引には整数ソースのみという限定がある。個別関数の条件を優先して読むこと。この訳では限定を維持する。

`UInt16`（16 ビット符号なし、0 … 65,535）：上下限、算術、ビット操作、テキスト、この型への変換。自動 import され、`import` は不要。

## 変換

変換は生成先の型にちなんで命名する（R-L0-40）。`uint16.from(x)` と同系統の関数は、`Int`、幅付き整数、`Float`、幅付き浮動小数点数という任意の数値ソースを受け取る。

```almd
let x: UInt16 = uint16.from(12)
let wide = int.from(x)                 // leaving UInt16: the destination's `from`
let low = uint16.from_wrapping(65580)  // 44: the low 16 bits, two's complement
let fits = uint16.from_checked(65536)  // none: 65536 is not a UInt16
let top = uint16.from_saturating(float.infinity()) // 65535: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `uint16.from(x)` | x をゼロ方向へ丸める。NaN、無限大、UInt16 の範囲外の値は abort |
| `uint16.from_checked(x)` | UInt16 が x を正確に保持できれば `some`、それ以外は `none`（小数部分、NaN、範囲外） |
| `uint16.from_saturating(x)` | x をゼロ方向へ丸め、UInt16 の範囲に収める。NaN は 0 |
| `uint16.from_wrapping(x)` | 整数ソースのみ。下位 16 ビット、2 の補数 |

`UInt16` から別の型へ変換するには、`int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` など変換先の関数群を呼ぶ。変換先が元の型のすべての値を保持できるなら `from` が適切である。狭い変換先では、abort、`none`、範囲制限、折り返しを名前で選び、暗黙には行わない。

> **`Int` を取る関数へ渡す前に `Int` へ変換すること。** 幅付きの値は暗黙に拡張されず、checker は型の不一致を拒否する。両ターゲットで `int.from(x)` を使い、明示的に変換を要求する。範囲外の値は abort する（checked、saturating、wrapping が意図した方針なら、その変種を使う）。

幅付き型のリテラルはコンパイル時に範囲検査する。`let a: UInt16 = 70000` は黙って折り返すのではなく E024 エラーになる。

`min_value()` / `max_value()` がこの型の上下限を与える。正確な公開面は下記の機械管理されたシグネチャ索引にある。

> 以下は原文の生成索引の固定コピーであり、この訳は生成対象ではない。

## シグネチャ索引（35 関数）

```
// Decimal digits; never a sign.
// @since 0.15.0 or earlier
uint16.to_string(x: UInt16) -> String

// Smallest UInt16: 0.
// @since 0.38.0
uint16.min_value() -> UInt16

// Largest UInt16: 65535.
// @since 0.38.0
uint16.max_value() -> UInt16

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
uint16.compare(a: UInt16, b: UInt16) -> Int

// Lesser of a and b.
// @since unreleased
uint16.min(a: UInt16, b: UInt16) -> UInt16

// Greater of a and b.
// @since unreleased
uint16.max(a: UInt16, b: UInt16) -> UInt16

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16

// Quotient; the same as a / b for an unsigned type; b == 0 aborts.
// @since unreleased
uint16.floor_div(a: UInt16, b: UInt16) -> UInt16

// Remainder; the same as a % b for an unsigned type; b == 0 aborts.
// @since unreleased
uint16.modulo(a: UInt16, b: UInt16) -> UInt16

// base raised to exp, wrapping to 16 bits like *; exp < 0 aborts.
// @since unreleased
uint16.pow(base: UInt16, exp: Int) -> UInt16

// True when x is a multiple of 2.
// @since unreleased
uint16.is_even(x: UInt16) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
uint16.is_odd(x: UInt16) -> Bool

// Bitwise AND.
// @since unreleased
uint16.band(a: UInt16, b: UInt16) -> UInt16

// Bitwise OR.
// @since unreleased
uint16.bor(a: UInt16, b: UInt16) -> UInt16

// Bitwise XOR.
// @since unreleased
uint16.bxor(a: UInt16, b: UInt16) -> UInt16

// Bitwise complement of all 16 bits.
// @since unreleased
uint16.bnot(x: UInt16) -> UInt16

// a shifted left by n bits; n is taken mod 16.
// @since unreleased
uint16.bshl(a: UInt16, n: Int) -> UInt16

// Zero-filling right shift; n is taken mod 16.
// @since unreleased
uint16.bshr(a: UInt16, n: Int) -> UInt16

// Zero bits above the highest set bit, counted in 16 bits; 16 for 0.
// @since unreleased
uint16.count_leading_zeros(x: UInt16) -> Int

// Zero bits below the lowest set bit; 16 for 0.
// @since unreleased
uint16.count_trailing_zeros(x: UInt16) -> Int

// Number of set bits among the 16.
// @since unreleased
uint16.count_ones(x: UInt16) -> Int

// The 2 bytes in reverse order (endian flip).
// @since unreleased
uint16.byte_swap(x: UInt16) -> UInt16

// a + b wrapped to 16 bits.
// @since unreleased
uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16

// a - b wrapped to 16 bits.
// @since unreleased
uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16

// a * b wrapped to 16 bits.
// @since unreleased
uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16

// The 16 bits of a rotated left by n; n is taken mod 16.
// @since unreleased
uint16.rotate_left(a: UInt16, n: Int) -> UInt16

// The 16 bits of a rotated right by n; n is taken mod 16.
// @since unreleased
uint16.rotate_right(a: UInt16, n: Int) -> UInt16

// Bits needed to hold x; 0 for 0.
// @since unreleased
uint16.bit_length(x: UInt16) -> Int

// Decimal UInt16 from ASCII-trimmed s; err on junk, a sign of -, or above 65535.
// @since unreleased
uint16.parse(s: String) -> Result[UInt16, String]

// Digits of x in radix, lowercase, no sign; radix outside 2..36 aborts.
// @since unreleased
uint16.to_string_radix(x: UInt16, radix: Int) -> String

// UInt16 from trimmed s in radix, the grammar of int.parse_radix; err on junk, a sign of -, or above 65535.
// @since unreleased
uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]

// x as UInt16, toward zero; NaN, an infinity or a value outside UInt16 aborts.
// @since unreleased
uint16.from(x: S) -> UInt16

// some(x as UInt16) when UInt16 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
uint16.from_checked(x: S) -> Option[UInt16]

// x as UInt16, toward zero, clamped to UInt16's range; NaN is 0.
// @since unreleased
uint16.from_saturating(x: S) -> UInt16

// The low 16 bits of integer x as UInt16 (two's complement).
// @since unreleased
uint16.from_wrapping(x: S) -> UInt16
```

## API コメントの説明訳

- `uint16.to_string(x: UInt16) -> String` — 10 進数字。符号は付かない。
- `uint16.min_value() -> UInt16` — 最小の UInt16：0。
- `uint16.max_value() -> UInt16` — 最大の UInt16：65535。
- `uint16.compare(a: UInt16, b: UInt16) -> Int` — a が b より小さい、等しい、大きい順に -1、0、1。
- `uint16.min(a: UInt16, b: UInt16) -> UInt16` — a と b の小さい方。
- `uint16.max(a: UInt16, b: UInt16) -> UInt16` — a と b の大きい方。
- `uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` — x を lo..hi に収める。lo > hi なら abort。
- `uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` — 商。符号なし型の a / b と同じ。b == 0 なら abort。
- `uint16.modulo(a: UInt16, b: UInt16) -> UInt16` — 剰余。符号なし型の a % b と同じ。b == 0 なら abort。
- `uint16.pow(base: UInt16, exp: Int) -> UInt16` — base の exp 乗。* と同様に 16 ビットへ折り返す。exp < 0 なら abort。
- `uint16.is_even(x: UInt16) -> Bool` — x が 2 の倍数なら true。
- `uint16.is_odd(x: UInt16) -> Bool` — x が 2 の倍数でなければ true。
- `uint16.band(a: UInt16, b: UInt16) -> UInt16` — ビット単位 AND。
- `uint16.bor(a: UInt16, b: UInt16) -> UInt16` — ビット単位 OR。
- `uint16.bxor(a: UInt16, b: UInt16) -> UInt16` — ビット単位 XOR。
- `uint16.bnot(x: UInt16) -> UInt16` — 16 ビットすべてをビット反転。
- `uint16.bshl(a: UInt16, n: Int) -> UInt16` — a を n ビット左シフト。n は 16 を法とする。
- `uint16.bshr(a: UInt16, n: Int) -> UInt16` — ゼロ埋め右シフト。n は 16 を法とする。
- `uint16.count_leading_zeros(x: UInt16) -> Int` — 16 ビット中の最上位の立ったビットより上のゼロ数。0 なら 16。
- `uint16.count_trailing_zeros(x: UInt16) -> Int` — 最下位の立ったビットより下のゼロ数。0 なら 16。
- `uint16.count_ones(x: UInt16) -> Int` — 16 ビット中の立っているビット数。
- `uint16.byte_swap(x: UInt16) -> UInt16` — 2 バイトを逆順にする（エンディアン反転）。
- `uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` — a + b を 16 ビットへ折り返す。
- `uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` — a - b を 16 ビットへ折り返す。
- `uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` — a * b を 16 ビットへ折り返す。
- `uint16.rotate_left(a: UInt16, n: Int) -> UInt16` — a の 16 ビットを n だけ左回転。n は 16 を法とする。
- `uint16.rotate_right(a: UInt16, n: Int) -> UInt16` — a の 16 ビットを n だけ右回転。n は 16 を法とする。
- `uint16.bit_length(x: UInt16) -> Int` — x を保持するのに必要なビット数。0 なら 0。
- `uint16.parse(s: String) -> Result[UInt16, String]` — 前後の ASCII 空白を除いた s から 10 進 UInt16 を解析。不正内容、- 符号、65535 超なら err。
- `uint16.to_string_radix(x: UInt16, radix: Int) -> String` — x を radix 進の小文字数字で表す。符号なし。radix が 2..36 外なら abort。
- `uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` — 前後の空白を除いた s を radix 進の UInt16 として解析。文法は int.parse_radix。不正内容、- 符号、65535 超なら err。
- `uint16.from(x: S) -> UInt16` — x をゼロ方向に丸め UInt16 にする。NaN、無限大、UInt16 範囲外なら abort。
- `uint16.from_checked(x: S) -> Option[UInt16]` — UInt16 が x を正確に保持できれば some(x as UInt16)、それ以外（小数部分、NaN、範囲外）は none。
- `uint16.from_saturating(x: S) -> UInt16` — x をゼロ方向に丸め UInt16 にし、UInt16 の範囲に収める。NaN は 0。
- `uint16.from_wrapping(x: S) -> UInt16` — 整数 x の下位 16 ビットを UInt16 とする（2 の補数）。
