# int

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](int.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](int.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](int.zh-CN.md)



> 原文：[int.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/int.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

> 本文の絶対値や範囲制限の簡略説明に加え、API 索引は min_value の折り返しや逆順の境界での中断を記載しています。限定を含め原文の説明を保持しています。

整数演算とビット演算。自動インポートされます。

### `int.to_string(x: Int) -> String`

整数を10進数の文字列表現に変換します。

```almd run
fn main() -> Unit = {
  println(int.to_string(42))
}
```
```output
42
```

### `int.to_hex(x: Int) -> String`

整数を16進数の文字列表現（小文字）に変換します。

```almd run
fn main() -> Unit = {
  println(int.to_hex(255))
}
```
```output
ff
```

### `int.parse(s: String) -> Result[Int, String]`

10進数の文字列を整数に解析します。有効な整数でない場合は err を返します。

数値の前後にある ASCII 空白（スペース、タブ、LF、CR、フォームフィード）だけを無視します。改行しないスペース（U+00A0）、全角スペース（U+3000）、垂直タブなど、それ以外の空白はエラーです。`int.parse("\u{A0}5")` は `err("invalid digit found in string")` になります。Unicode 空白を受け入れるには、先に `string.trim` を呼んでください。

```almd run
fn main() -> Unit = {
  match int.parse("42") {
    ok(n) => println("ok(${n})"),
    err(e) => println("err(${e})"),
  }
  match int.parse("4x2") {
    ok(n) => println("ok(${n})"),
    err(e) => println("err(${e})"),
  }
}
```
```output
ok(42)
err(invalid digit found in string)
```

### `int.abs(x: Int) -> Int`

整数の絶対値を返します。

```almd run
fn main() -> Unit = {
  println("${int.abs(-5)}")
}
```
```output
5
```

### `int.is_even(x: Int) -> Bool`

偶奇判定：`n` が2で割り切れるとき `true`。負数と境界値でも正確です（`%` は被除数の符号を保つため、判定は `% 2 == 0` です）。

```almd run
fn main() -> Unit = {
  println("${int.is_even(4)} ${int.is_even(-3)} ${int.is_even(0)}")
}
```
```output
true false true
```

### `int.is_odd(x: Int) -> Bool`

偶奇判定のもう一方：`n` が2で割り切れないとき `true`。スカラーの `int` のみが対象で、ビット幅を持つ値は `int.from(x)` を経由して渡します。

```almd run
fn main() -> Unit = {
  println("${int.is_odd(7)} ${int.is_odd(-3)} ${int.is_odd(4)}")
}
```
```output
true true false
```

### `int.min(a: Int, b: Int) -> Int`

2つの整数の小さい方を返します。

```almd run
fn main() -> Unit = {
  println("${int.min(3, 7)}")
}
```
```output
3
```

### `int.max(a: Int, b: Int) -> Int`

2つの整数の大きい方を返します。

```almd run
fn main() -> Unit = {
  println("${int.max(3, 7)}")
}
```
```output
7
```

### `int.band(a: Int, b: Int) -> Int`

2つの整数のビット単位 AND。

```almd run
fn main() -> Unit = {
  println("${int.band(0b1100, 0b1010)}") // 0b1000
}
```
```output
8
```

### `int.bor(a: Int, b: Int) -> Int`

2つの整数のビット単位 OR。

```almd run
fn main() -> Unit = {
  println("${int.bor(0b1100, 0b1010)}") // 0b1110
}
```
```output
14
```

### `int.bxor(a: Int, b: Int) -> Int`

2つの整数のビット単位 XOR。

```almd run
fn main() -> Unit = {
  println("${int.bxor(0b1100, 0b1010)}") // 0b0110
}
```
```output
6
```

### `int.bshl(a: Int, n: Int) -> Int`

ビット単位の左シフト。

```almd run
fn main() -> Unit = {
  println("${int.bshl(1, 3)}")
}
```
```output
8
```

### `int.bshr(a: Int, n: Int) -> Int`

ビット単位の右シフト（算術シフト）。

```almd run
fn main() -> Unit = {
  println("${int.bshr(8, 2)}")
  println("${int.bshr(-8, 2)}")
}
```
```output
2
-2
```

### `int.bnot(x: Int) -> Int`

整数のビット単位 NOT（反転）。

```almd run
fn main() -> Unit = {
  println("${int.bnot(0)}")
}
```
```output
-1
```

### `int.wrap_add(a: Int, b: Int) -> Int`

64ビット Int のラッピング加算：オーバーフローすると折り返します。Nビットレジスタの場合は `int.wrap` と組み合わせます：`int.wrap(int.wrap_add(a, b), bits)`。

```almd run
fn main() -> Unit = {
  println("${int.wrap_add(int.max_value(), 1)}")
  println("${int.wrap(int.wrap_add(255, 1), 8)}")
}
```
```output
-9223372036854775808
0
```

### `int.wrap_mul(a: Int, b: Int) -> Int`

64ビット Int のラッピング乗算：オーバーフローすると折り返します。Nビットレジスタの場合は `int.wrap` と組み合わせます。

```almd run
fn main() -> Unit = {
  println("${int.wrap(int.wrap_mul(16, 16), 8)}")
}
```
```output
0
```

### `int.rotate_right(a: Int, n: Int) -> Int`

`a` の64ビットを右に回転します。回数 `n` には64を法とするユークリッド剰余を使うため、負の回数は左回転になります：`rotate_right(1, -1) = 2`。

```almd run
fn main() -> Unit = {
  println("${int.rotate_right(4, 1)}")
}
```
```output
2
```

### `int.rotate_left(a: Int, n: Int) -> Int`

`a` の64ビットを左に回転します。回数 `n` には64を法とするユークリッド剰余を使うため、負の回数は右回転になります：`rotate_left(2, -1) = 1`。

```almd run
fn main() -> Unit = {
  println("${int.rotate_left(int.min_value(), 1)}")
}
```
```output
1
```

### `int.clamp(x: Int, lo: Int, hi: Int) -> Int`

整数を範囲 [lo, hi] に収めます。

```almd run
fn main() -> Unit = {
  println("${int.clamp(15, 0, 10)}")
  println("${int.clamp(-3, 0, 10)}")
}
```
```output
10
0
```

## 変換行列の完全性 (#956)

変換は生成する型にちなんで命名します（R-L0-40）。`int.from(x)`（範囲外では中断）、`int.from_checked(x)`（`none`）、`int.from_saturating(x)`（範囲内に制限、NaN は0）、`int.from_wrapping(x)`（整数入力、2の補数）は数値を入力に取り、上位の範囲が `Int` に収まらない `UInt64 → Int` も含みます。ビット幅を持つ各モジュールには、その型用の同じ変換群があり、固定幅の数値型はすべて `min_value()`/`max_value()` を持ちます。


## 原文固定スナップショットの API 索引（47関数）

```
// Decimal digits, with - when negative.
// @since 0.5.0 or earlier
int.to_string(x: Int) -> String

// Lowercase hex, no 0x; -1 gives 16 f digits.
// @since 0.5.0 or earlier
int.to_hex(x: Int) -> String

// Decimal Int from s trimmed of ASCII space/tab/LF/FF/CR; err on junk or overflow.
// @since 0.5.0 or earlier
int.parse(s: String) -> Result[Int, String]

// Magnitude of x; min_value wraps to itself.
// @since 0.5.0 or earlier
int.abs(x: Int) -> Int

// Lesser of a and b.
// @since 0.5.0 or earlier
int.min(a: Int, b: Int) -> Int

// Greater of a and b.
// @since 0.5.0 or earlier
int.max(a: Int, b: Int) -> Int

// x limited to lo..hi; aborts when lo > hi.
// @since 0.5.0 or earlier
int.clamp(x: Int, lo: Int, hi: Int) -> Int

// True when x % 2 == 0; exact for negatives.
// @since 0.62.0
int.is_even(x: Int) -> Bool

// True when x % 2 != 0; -3 counts as odd.
// @since 0.62.0
int.is_odd(x: Int) -> Bool

// Bitwise AND over all 64 bits.
// @since 0.5.0 or earlier
int.band(a: Int, b: Int) -> Int

// Bitwise OR over all 64 bits.
// @since 0.5.0 or earlier
int.bor(a: Int, b: Int) -> Int

// Bitwise XOR over all 64 bits.
// @since 0.5.0 or earlier
int.bxor(a: Int, b: Int) -> Int

// Bitwise complement; equals -x - 1.
// @since 0.5.0 or earlier
int.bnot(x: Int) -> Int

// a shifted left by n bits; n is taken mod 64.
// @since 0.5.0 or earlier
int.bshl(a: Int, n: Int) -> Int

// Arithmetic (sign-filling) right shift; n mod 64.
// @since 0.5.0 or earlier
int.bshr(a: Int, n: Int) -> Int

// Leading zero bits of the 64-bit word; 64 for 0.
// @since 0.16.0 or earlier
int.count_leading_zeros(x: Int) -> Int

// Trailing zero bits; 64 for 0.
// @since 0.16.0 or earlier
int.count_trailing_zeros(x: Int) -> Int

// Number of set bits; 64 for -1.
// @since 0.16.0 or earlier
int.pop_count(n: Int) -> Int   (deprecated — use int.count_ones)

// Number of set bits; 64 for -1.
// @since unreleased
int.count_ones(x: Int) -> Int

// The 64 bits in reverse order; 1 gives min_value.
// @since 0.16.0 or earlier
int.bit_reverse(x: Int) -> Int

// The 8 bytes in reverse order (endian flip).
// @since 0.16.0 or earlier
int.byte_swap(x: Int) -> Int

// Bits needed to hold n; 0 for 0, 64 if negative.
// @since 0.16.0 or earlier
int.bit_width(n: Int) -> Int   (deprecated — use int.bit_length)

// Bits needed to hold x; 0 for 0, 64 if negative.
// @since unreleased
int.bit_length(x: Int) -> Int

// Floor of log2(x); -1 when x <= 0.
// @since 0.16.0 or earlier
int.log2_floor(x: Int) -> Int

// Ceiling of log2(x); 0 when x <= 1.
// @since 0.16.0 or earlier
int.log2_ceil(x: Int) -> Int

// Least power of 2 >= x; 1 if x <= 1; min_value past 2^62.
// @since 0.16.0 or earlier
int.next_power_of_two(x: Int) -> Int

// Greatest power of 2 <= x; 0 when x <= 0.
// @since 0.16.0 or earlier
int.prev_power_of_two(x: Int) -> Int

// a + b wrapped to 64 bits.
// @since 0.5.0 or earlier
int.wrap_add(a: Int, b: Int) -> Int

// a * b wrapped to 64 bits.
// @since 0.5.0 or earlier
int.wrap_mul(a: Int, b: Int) -> Int

// The 64 bits of a rotated right by n; n is taken mod 64.
// @since 0.5.0 or earlier
int.rotate_right(a: Int, n: Int) -> Int

// The 64 bits of a rotated left by n; n is taken mod 64.
// @since 0.5.0 or earlier
int.rotate_left(a: Int, n: Int) -> Int

// Smallest Int: -2^63.
// @since 0.38.0
int.min_value() -> Int

// Largest Int: 2^63-1.
// @since 0.38.0
int.max_value() -> Int

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
int.compare(a: Int, b: Int) -> Int

// -1, 0 or 1 by the sign of x; 0 for 0.
// @since unreleased
int.sign(x: Int) -> Int

// base raised to exp, wrapping to 64 bits like *; exp < 0 aborts.
// @since unreleased
int.pow(base: Int, exp: Int) -> Int

// Quotient rounded toward -inf; b == 0 and min_value / -1 abort, like /.
// @since unreleased
int.floor_div(a: Int, b: Int) -> Int

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
int.modulo(a: Int, b: Int) -> Int

// x mod 2^bits, non-negative when bits < 64; bits <= 0 aborts, bits >= 64 keeps x.
// @since unreleased
int.wrap(x: Int, bits: Int) -> Int

// a - b wrapped to 64 bits.
// @since unreleased
int.wrap_sub(a: Int, b: Int) -> Int

// Zero-filling right shift of the 64 bits; n is taken mod 64.
// @since unreleased
int.bshr_logical(a: Int, n: Int) -> Int

// Signed digits of x in radix, lowercase; -255 in 16 is "-ff"; radix outside 2..36 aborts.
// @since unreleased
int.to_string_radix(x: Int, radix: Int) -> String

// Int from trimmed s in radix: a + or - sign, any case, and 0x, 0o, 0b allowed for 16, 8, 2;
// err on junk or overflow; radix outside 2..36 aborts.
// @since unreleased
int.parse_radix(s: String, radix: Int) -> Result[Int, String]

// x as Int, toward zero; NaN, an infinity or a value outside Int aborts.
// @since unreleased
int.from(x: S) -> Int

// some(x as Int) when Int holds x exactly, else none (a fraction, NaN, out of range).
// @since unreleased
int.from_checked(x: S) -> Option[Int]

// x as Int, toward zero, clamped to Int's range; NaN is 0.
// @since unreleased
int.from_saturating(x: S) -> Int

// The low 64 bits of integer x as Int (two's complement).
// @since unreleased
int.from_wrapping(x: S) -> Int
```


## API 説明の訳（上記署名順）

- `int.to_string(x: Int) -> String`：10進数字。負の場合は - を付けます。
- `int.to_hex(x: Int) -> String`：小文字の16進数。0x は付きません。-1 は f が16桁になります。
- `int.parse(s: String) -> Result[Int, String]`：s の前後の ASCII スペース/タブ/LF/FF/CR を除去して10進 Int にします。不正な文字やオーバーフローは err です。
- `int.abs(x: Int) -> Int`：x の大きさ。min_value は折り返して自身になります。
- `int.min(a: Int, b: Int) -> Int`：a と b の小さい方。
- `int.max(a: Int, b: Int) -> Int`：a と b の大きい方。
- `int.clamp(x: Int, lo: Int, hi: Int) -> Int`：x を lo..hi に制限します。lo > hi のとき中断します。
- `int.is_even(x: Int) -> Bool`：x % 2 == 0 のとき true。負数でも正確です。
- `int.is_odd(x: Int) -> Bool`：x % 2 != 0 のとき true。-3 は奇数です。
- `int.band(a: Int, b: Int) -> Int`：64ビットすべてのビット単位 AND。
- `int.bor(a: Int, b: Int) -> Int`：64ビットすべてのビット単位 OR。
- `int.bxor(a: Int, b: Int) -> Int`：64ビットすべてのビット単位 XOR。
- `int.bnot(x: Int) -> Int`：ビット反転。-x - 1 と等価です。
- `int.bshl(a: Int, n: Int) -> Int`：a を n ビット左シフトします。n は64を法とします。
- `int.bshr(a: Int, n: Int) -> Int`：算術（符号埋め）右シフト。n は64を法とします。
- `int.count_leading_zeros(x: Int) -> Int`：64ビット語の先頭のゼロビット数。0 の場合は64。
- `int.count_trailing_zeros(x: Int) -> Int`：末尾のゼロビット数。0 の場合は64。
- `int.pop_count(n: Int) -> Int   (deprecated — use int.count_ones)`：立っているビット数。-1 の場合は64。
- `int.count_ones(x: Int) -> Int`：立っているビット数。-1 の場合は64。
- `int.bit_reverse(x: Int) -> Int`：64ビットの順序を反転します。1 は min_value になります。
- `int.byte_swap(x: Int) -> Int`：8バイトの順序を反転します（エンディアン反転）。
- `int.bit_width(n: Int) -> Int   (deprecated — use int.bit_length)`：n を格納するのに必要なビット数。0 は0、負数は64。
- `int.bit_length(x: Int) -> Int`：x を格納するのに必要なビット数。0 は0、負数は64。
- `int.log2_floor(x: Int) -> Int`：log2(x) の床。x <= 0 の場合は -1。
- `int.log2_ceil(x: Int) -> Int`：log2(x) の天井。x <= 1 の場合は0。
- `int.next_power_of_two(x: Int) -> Int`：x 以上の最小の2の累乗。x <= 1 なら1。2^62 を超えると min_value。
- `int.prev_power_of_two(x: Int) -> Int`：x 以下の最大の2の累乗。x <= 0 なら0。
- `int.wrap_add(a: Int, b: Int) -> Int`：a + b を64ビットに折り返します。
- `int.wrap_mul(a: Int, b: Int) -> Int`：a * b を64ビットに折り返します。
- `int.rotate_right(a: Int, n: Int) -> Int`：a の64ビットを右に n 回転します。n は64を法とします。
- `int.rotate_left(a: Int, n: Int) -> Int`：a の64ビットを左に n 回転します。n は64を法とします。
- `int.min_value() -> Int`：最小の Int：-2^63。
- `int.max_value() -> Int`：最大の Int：2^63-1。
- `int.compare(a: Int, b: Int) -> Int`：a が b より小さい、等しい、大きい順に -1、0、1。
- `int.sign(x: Int) -> Int`：x の符号に応じて -1、0、1。0 に対しては0。
- `int.pow(base: Int, exp: Int) -> Int`：base の exp 乗。* と同様に64ビットに折り返します。exp < 0 は中断します。
- `int.floor_div(a: Int, b: Int) -> Int`：商を -inf 方向に丸めます。/ と同様、b == 0 と min_value / -1 は中断します。
- `int.modulo(a: Int, b: Int) -> Int`：b と同じ符号の剰余で、a == floor_div(a, b) * b + modulo(a, b) を満たします。b == 0 は中断します。
- `int.wrap(x: Int, bits: Int) -> Int`：x mod 2^bits。bits < 64 なら非負です。bits <= 0 は中断し、bits >= 64 は x を保ちます。
- `int.wrap_sub(a: Int, b: Int) -> Int`：a - b を64ビットに折り返します。
- `int.bshr_logical(a: Int, n: Int) -> Int`：64ビットをゼロ埋めして右シフトします。n は64を法とします。
- `int.to_string_radix(x: Int, radix: Int) -> String`：x を radix 進の符号付き小文字数字で表します。16進の -255 は "-ff"。radix が2..36の範囲外なら中断します。
- `int.parse_radix(s: String, radix: Int) -> Result[Int, String]`：前後の空白を除去した s を radix 進の Int にします。+ または - の符号、大文字・小文字、16・8・2進での0x・0o・0bを許可します。不正な文字やオーバーフローは err。radix が2..36の範囲外なら中断します。
- `int.from(x: S) -> Int`：x をゼロ方向に丸めて Int にします。NaN、無限大、Int の範囲外の値は中断します。
- `int.from_checked(x: S) -> Option[Int]`：Int が x を正確に保持できれば some(x as Int)、それ以外（小数、NaN、範囲外）は none。
- `int.from_saturating(x: S) -> Int`：x をゼロ方向に丸めて Int にし、Int の範囲に制限します。NaN は0。
- `int.from_wrapping(x: S) -> Int`：整数 x の下位64ビットを Int として扱います（2の補数）。
