# int16

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int16.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](int16.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](int16.zh-CN.md)



> 原文：[int16.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int16.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

`Int16`（16ビット符号付き, -32,768 … 32,767）：境界、算術演算、ビット演算、テキスト、この型への変換。自動インポートされるため `import` は不要です。

## 変換

変換は生成する型にちなんで命名します（R-L0-40）。`int16.from(x)` と同系列の関数は任意の数値入力、つまり `Int`、ビット幅を持つ整数、`Float`、ビット幅を持つ浮動小数点数を受け取ります。

```almd
let x: Int16 = int16.from(12)
let wide = int.from(x)                 // leaving Int16: the destination's `from`
let low = int16.from_wrapping(32812)   // -32724: the low 16 bits, two's complement
let fits = int16.from_checked(32768)   // none: 32768 is not an Int16
let top = int16.from_saturating(float.infinity()) // 32767: clamped to the range
let text = x.to_string()
```

| 関数 | 動作 |
|---|---|
| `int16.from(x)` | x をゼロ方向に丸めます。NaN、無限大、Int16 の範囲外の値は中断 |
| `int16.from_checked(x)` | Int16 が x を正確に保持できれば `some`、それ以外（小数、NaN、範囲外）は `none` |
| `int16.from_saturating(x)` | x をゼロ方向に丸め、Int16 の範囲に制限。NaN は0 |
| `int16.from_wrapping(x)` | 整数入力のみ。下位16ビット、2の補数 |

`Int16` から別の型にするには、変換先の関数群、たとえば `int.from(x)`、`float.from(x)`、`int16.from_wrapping(x)` などを呼びます。変換先が変換元のすべての値を保持できる場合は `from` が適切です。狭い範囲への変換では中断、`none`、範囲制限、折り返しを名前で選び、暗黙には行いません。

> **`Int` を受け取る関数へ渡す前に `Int` に変換してください。** ビット幅を持つ値は暗黙には拡張されず、検査器は型の不一致を拒否します。両ターゲットで `int.from(x)` を使い、明示的に変換を要求します。範囲外の値は中断します（意図する方針に応じて checked、saturating、wrapping 版を使ってください）。

ビット幅を持つ型のリテラルはコンパイル時に範囲検査されます。`let a: Int16 = 40000` は暗黙の折り畳みではなく E024 エラーになります。

`min_value()` / `max_value()` がこの型の境界を返します。正確な API 一覧は、以下に保持した原文の機械管理署名索引です（この訳は固定スナップショットであり、機械管理の対象ではありません）。

> 原文は Int16 から別の型への変換例にも int16.from_wrapping を挙げています。同じ型を指すこの例は原文のまま残し、別の変換先に置き換えていません。

## 原文固定スナップショットの API 索引（37関数）

```
// Decimal digits, with - when negative.
// @since 0.15.0 or earlier
int16.to_string(x: Int16) -> String

// Smallest Int16: -32768.
// @since 0.38.0
int16.min_value() -> Int16

// Largest Int16: 32767.
// @since 0.38.0
int16.max_value() -> Int16

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int16.compare(a: Int16, b: Int16) -> Int

// Lesser of a and b.
// @since unreleased
int16.min(a: Int16, b: Int16) -> Int16

// Greater of a and b.
// @since unreleased
int16.max(a: Int16, b: Int16) -> Int16

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int16.floor_div(a: Int16, b: Int16) -> Int16

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int16.modulo(a: Int16, b: Int16) -> Int16

// base raised to exp, wrapping to 16 bits like *; exp < 0 aborts.
// @since unreleased
int16.pow(base: Int16, exp: Int) -> Int16

// True when x is a multiple of 2.
// @since unreleased
int16.is_even(x: Int16) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
int16.is_odd(x: Int16) -> Bool

// Magnitude of x; min_value wraps to itself.
// @since unreleased
int16.abs(x: Int16) -> Int16

// -1, 0 or 1 by the sign of x.
// @since unreleased
int16.sign(x: Int16) -> Int

// Bitwise AND.
// @since unreleased
int16.band(a: Int16, b: Int16) -> Int16

// Bitwise OR.
// @since unreleased
int16.bor(a: Int16, b: Int16) -> Int16

// Bitwise XOR.
// @since unreleased
int16.bxor(a: Int16, b: Int16) -> Int16

// Bitwise complement of all 16 bits.
// @since unreleased
int16.bnot(x: Int16) -> Int16

// a shifted left by n bits; n is taken mod 16.
// @since unreleased
int16.bshl(a: Int16, n: Int) -> Int16

// Sign-filling right shift; n is taken mod 16.
// @since unreleased
int16.bshr(a: Int16, n: Int) -> Int16

// Zero bits above the highest set bit, counted in 16 bits; 16 for 0.
// @since unreleased
int16.count_leading_zeros(x: Int16) -> Int

// Zero bits below the lowest set bit; 16 for 0.
// @since unreleased
int16.count_trailing_zeros(x: Int16) -> Int

// Number of set bits among the 16.
// @since unreleased
int16.count_ones(x: Int16) -> Int

// The 2 bytes in reverse order (endian flip).
// @since unreleased
int16.byte_swap(x: Int16) -> Int16

// a + b wrapped to 16 bits.
// @since unreleased
int16.wrap_add(a: Int16, b: Int16) -> Int16

// a - b wrapped to 16 bits.
// @since unreleased
int16.wrap_sub(a: Int16, b: Int16) -> Int16

// a * b wrapped to 16 bits.
// @since unreleased
int16.wrap_mul(a: Int16, b: Int16) -> Int16

// The 16 bits of a rotated left by n; n is taken mod 16.
// @since unreleased
int16.rotate_left(a: Int16, n: Int) -> Int16

// The 16 bits of a rotated right by n; n is taken mod 16.
// @since unreleased
int16.rotate_right(a: Int16, n: Int) -> Int16

// Bits needed to hold x; 0 for 0, 16 if negative.
// @since unreleased
int16.bit_length(x: Int16) -> Int

// Decimal Int16 from ASCII-trimmed s; err on junk or outside -32768..32767.
// @since unreleased
int16.parse(s: String) -> Result[Int16, String]

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts.
// @since unreleased
int16.to_string_radix(x: Int16, radix: Int) -> String

// Int16 from trimmed s in radix, the grammar of int.parse_radix; err on junk or outside -32768..32767.
// @since unreleased
int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]

// x as Int16, toward zero; NaN, an infinity or a value outside Int16 aborts.
// @since unreleased
int16.from(x: S) -> Int16

// some(x as Int16) when Int16 holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int16.from_checked(x: S) -> Option[Int16]

// x as Int16, toward zero, clamped to Int16's range; NaN is 0.
// @since unreleased
int16.from_saturating(x: S) -> Int16

// The low 16 bits of integer x as Int16 (two's complement).
// @since unreleased
int16.from_wrapping(x: S) -> Int16
```

## API 説明の訳（上記署名順）

- `int16.to_string(x: Int16) -> String`：10進数字。負の場合は - を付けます。
- `int16.min_value() -> Int16`：最小の Int16：-32768。
- `int16.max_value() -> Int16`：最大の Int16：32767。
- `int16.compare(a: Int16, b: Int16) -> Int`：a が b より小さい、等しい、大きい順に -1、0、1。
- `int16.min(a: Int16, b: Int16) -> Int16`：a と b の小さい方。
- `int16.max(a: Int16, b: Int16) -> Int16`：a と b の大きい方。
- `int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16`：x を lo..hi に制限します。lo > hi のとき中断します。
- `int16.floor_div(a: Int16, b: Int16) -> Int16`：商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int16.modulo(a: Int16, b: Int16) -> Int16`：b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int16.pow(base: Int16, exp: Int) -> Int16`：base の exp 乗。* と同様に16ビットに折り返します。exp < 0 は中断します。
- `int16.is_even(x: Int16) -> Bool`：x が2の倍数なら true。
- `int16.is_odd(x: Int16) -> Bool`：x が2の倍数でなければ true。
- `int16.abs(x: Int16) -> Int16`：x の大きさ。min_value は折り返して自身になります。
- `int16.sign(x: Int16) -> Int`：x の符号に応じて -1、0、1。
- `int16.band(a: Int16, b: Int16) -> Int16`：ビット単位 AND。
- `int16.bor(a: Int16, b: Int16) -> Int16`：ビット単位 OR。
- `int16.bxor(a: Int16, b: Int16) -> Int16`：ビット単位 XOR。
- `int16.bnot(x: Int16) -> Int16`：16ビットすべてをビット反転します。
- `int16.bshl(a: Int16, n: Int) -> Int16`：a を n ビット左シフトします。n は16を法とします。
- `int16.bshr(a: Int16, n: Int) -> Int16`：符号埋めの右シフト。n は16を法とします。
- `int16.count_leading_zeros(x: Int16) -> Int`：16ビット内で、最上位の立っているビットより上のゼロビット数。0 の場合は16。
- `int16.count_trailing_zeros(x: Int16) -> Int`：最下位の立っているビットより下のゼロビット数。0 の場合は16。
- `int16.count_ones(x: Int16) -> Int`：16ビット中の立っているビット数。
- `int16.byte_swap(x: Int16) -> Int16`：2バイトの順序を反転します（エンディアン反転）。
- `int16.wrap_add(a: Int16, b: Int16) -> Int16`：a + b を16ビットに折り返します。
- `int16.wrap_sub(a: Int16, b: Int16) -> Int16`：a - b を16ビットに折り返します。
- `int16.wrap_mul(a: Int16, b: Int16) -> Int16`：a * b を16ビットに折り返します。
- `int16.rotate_left(a: Int16, n: Int) -> Int16`：a の16ビットを左に n 回転します。n は16を法とします。
- `int16.rotate_right(a: Int16, n: Int) -> Int16`：a の16ビットを右に n 回転します。n は16を法とします。
- `int16.bit_length(x: Int16) -> Int`：x を格納するのに必要なビット数。0 は0、負数は16。
- `int16.parse(s: String) -> Result[Int16, String]`：s の前後の ASCII 空白を除去して10進 Int16 にします。不正な文字や -32768..32767 の範囲外は err。
- `int16.to_string_radix(x: Int16, radix: Int) -> String`：x を radix 進の小文字数字で表し、負数には - を付けます。radix が2..36の範囲外なら中断します。
- `int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]`：前後の空白を除去した s を radix 進の Int16 にします。構文は int.parse_radix と同じです。不正な文字や -32768..32767 の範囲外は err。
- `int16.from(x: S) -> Int16`：x をゼロ方向に丸めて Int16 にします。NaN、無限大、Int16 の範囲外の値は中断します。
- `int16.from_checked(x: S) -> Option[Int16]`：Int16 が x を正確に保持できれば some(x as Int16)、それ以外（小数、NaN、範囲外）は none。
- `int16.from_saturating(x: S) -> Int16`：x をゼロ方向に丸めて Int16 にし、Int16 の範囲に制限します。NaN は0。
- `int16.from_wrapping(x: S) -> Int16`：整数 x の下位16ビットを Int16 として扱います（2の補数）。
