# bigint

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bigint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](bigint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bigint.zh-CN.md)



> 原文：[bigint.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bigint.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。
>
> 原文上の注意: 冒頭の「大きすぎる値はない」は、後述の変換桁数・結果サイズ・資源の制限をなくす保証ではありません。型索引は原文の固定コピーです。

任意精度整数 `BigInt`。大きすぎる値はなく、wrap もしません。`import bigint` が必要です。pure で、両ターゲットで同じ動作です。

表記は `int` と同じです。`parse` / `to_string` とその `_radix` 対、`compare`、`min`、`max`、`clamp`、`abs`、`sign`、`pow`、`floor_div`、`modulo`、`is_even`、`is_odd`、`bit_length`、`b` 接頭辞のビット操作関数があります。負の値は上位に 1 ビットが無限に続く二の補数なので、`bnot(x)` は `-x - 1`、`bshr` は -inf へ丸めます。同じ値を持つ二つの BigInt は `==` です。

制限（#4196）。二のべき乗でない基数（十進を含む）のテキストは、長さの二乗に比例する時間を要するため、Python の既定と同じ 4300 桁に制限します。`parse` / `parse_radix` はそれを超える桁数（先頭ゼロは数え、符号は数えない）に `err("integer string conversion exceeds the limit of 4300 digits")` を返し、`to_string` / `to_string_radix` は結果が長すぎる場合に同じ文言で abort します。基数 2、4、8、16、32 の変換は線形時間で制限がありません。巨大な値は十六進で出力してください。`pow` は、30 * 2^28 ビット（2^28 limbs、`bshl` が達するリスト上限）を確実に超える結果を計算する前に `pow result too large` で abort します。

BigInt は算術演算子 `+ - * / %` に対応します。`parse`、`pow`、`bigint.from(x)` で値を作ります。この変換は数値ソースを受け入れてゼロ方向へ切り捨て、NaN または無限大では abort します。`bigint.from_checked(x)` は、ソースが有限の整数でなければ `none` を返します。

```almd run
import bigint

effect fn main() -> Unit = {
  let two = bigint.parse("2")!
  let p = bigint.pow(two, 100)
  println(bigint.to_string(p))
  println(bigint.to_string_radix(p, 16))
  println("${bigint.bit_length(p)} ${bigint.compare(p, two)}")
}
```
```output
1267650600228229401496703205376
10000000000000000000000000
101 1
```



<a id="signature-index-24-functions"></a>

## 固定スナップショットの API 索引 (24 関数)

```
// BigInt from trimmed decimal s with an optional + or -; err on junk or more than 4300 digits.
// @since unreleased
bigint.parse(s: String) -> Result[BigInt, String]

// BigInt from trimmed s in radix: a + or - sign, any case, and 0x, 0o, 0b allowed for 16, 8, 2;
// err on junk, and on more than 4300 digits in a radix that is not a power of two;
// radix outside 2..36 aborts.
// @since unreleased
bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]

// Decimal digits, with - when negative; more than 4300 digits aborts.
// @since unreleased
bigint.to_string(x: BigInt) -> String

// Digits of x in radix, lowercase, - when negative; radix outside 2..36 aborts, and so does
// a radix that is not a power of two when the text would pass 4300 digits.
// @since unreleased
bigint.to_string_radix(x: BigInt, radix: Int) -> String

// -1, 0 or 1 as a is below, equal to or above b.
// @since unreleased
bigint.compare(a: BigInt, b: BigInt) -> Int

// Lesser of a and b.
// @since unreleased
bigint.min(a: BigInt, b: BigInt) -> BigInt

// Greater of a and b.
// @since unreleased
bigint.max(a: BigInt, b: BigInt) -> BigInt

// x limited to lo..hi; aborts when lo > hi.
// @since unreleased
bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt

// Magnitude of x.
// @since unreleased
bigint.abs(x: BigInt) -> BigInt

// -1, 0 or 1 by the sign of x; 0 for 0.
// @since unreleased
bigint.sign(x: BigInt) -> Int

// True when x is a multiple of 2.
// @since unreleased
bigint.is_even(x: BigInt) -> Bool

// True when x is not a multiple of 2.
// @since unreleased
bigint.is_odd(x: BigInt) -> Bool

// Quotient rounded toward -inf; b == 0 aborts.
// @since unreleased
bigint.floor_div(a: BigInt, b: BigInt) -> BigInt

// Remainder with the sign of b, so a == floor_div(a, b) * b + modulo(a, b); b == 0 aborts.
// @since unreleased
bigint.modulo(a: BigInt, b: BigInt) -> BigInt

// base raised to exp; exp < 0 aborts, and so does a result sure to pass 30 * 2^28 bits.
// @since unreleased
bigint.pow(base: BigInt, exp: Int) -> BigInt

// Bits needed to hold the magnitude of x; 0 for 0.
// @since unreleased
bigint.bit_length(x: BigInt) -> Int

// Bitwise AND.
// @since unreleased
bigint.band(a: BigInt, b: BigInt) -> BigInt

// Bitwise OR.
// @since unreleased
bigint.bor(a: BigInt, b: BigInt) -> BigInt

// Bitwise XOR.
// @since unreleased
bigint.bxor(a: BigInt, b: BigInt) -> BigInt

// Bitwise complement; equals -x - 1.
// @since unreleased
bigint.bnot(x: BigInt) -> BigInt

// a shifted left by n bits (a * 2^n); n < 0 aborts.
// @since unreleased
bigint.bshl(a: BigInt, n: Int) -> BigInt

// a shifted right by n bits, rounding toward -inf (sign-filling); n < 0 aborts.
// @since unreleased
bigint.bshr(a: BigInt, n: Int) -> BigInt

// x as BigInt, toward zero; NaN or an infinity aborts.
// @since unreleased
bigint.from(x: S) -> BigInt

// some(x as BigInt) when x is an integer, else none (a fraction, NaN, an infinity).
// @since unreleased
bigint.from_checked(x: S) -> Option[BigInt]
```

<a id="type-index-1-types"></a>

## 固定スナップショットの型索引（1 型）

```
// An arbitrary-precision integer.
// @since unreleased
type bigint.BigInt = { neg: Bool, limbs: List[Int] }
```



## API 説明の訳（上記署名順）

- `bigint.parse(s: String) -> Result[BigInt, String]` — 前後空白を除いた十進 s から BigInt。任意の + または - 符号。不正入力や 4300 桁超は err。
- `bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` — 前後空白を除いた基数 radix の s から BigInt。+/- 符号、大小文字可。16/8/2 では 0x/0o/0b 可。不正入力、および二のべき乗でない基数の 4300 桁超は err。radix が 2..36 外なら abort。
- `bigint.to_string(x: BigInt) -> String` — 十進数字。負なら - を付けます。4300 桁超は abort。
- `bigint.to_string_radix(x: BigInt, radix: Int) -> String` — x の radix 進数字、小文字、負なら -。radix が 2..36 外、または二のべき乗でない基数で 4300 桁を超えるテキストになる場合は abort。
- `bigint.compare(a: BigInt, b: BigInt) -> Int` — a が b より小・同じ・大なら -1、0、1。
- `bigint.min(a: BigInt, b: BigInt) -> BigInt` — a と b の小さい方。
- `bigint.max(a: BigInt, b: BigInt) -> BigInt` — a と b の大きい方。
- `bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt` — x を lo..hi に制限します。lo > hi は abort。
- `bigint.abs(x: BigInt) -> BigInt` — x の大きさ（絶対値）。
- `bigint.sign(x: BigInt) -> Int` — x の符号により -1、0、1。0 には 0。
- `bigint.is_even(x: BigInt) -> Bool` — x が 2 の倍数なら true。
- `bigint.is_odd(x: BigInt) -> Bool` — x が 2 の倍数でなければ true。
- `bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` — -inf 方向に丸めた商。b == 0 は abort。
- `bigint.modulo(a: BigInt, b: BigInt) -> BigInt` — b と同符号の余り。a == floor_div(a, b) * b + modulo(a, b) が成立します。b == 0 は abort。
- `bigint.pow(base: BigInt, exp: Int) -> BigInt` — base の exp 乗。exp < 0、または結果が確実に 30 * 2^28 ビット超となる場合は abort。
- `bigint.bit_length(x: BigInt) -> Int` — x の絶対値を表すためのビット数。0 には 0。
- `bigint.band(a: BigInt, b: BigInt) -> BigInt` — ビット単位 AND。
- `bigint.bor(a: BigInt, b: BigInt) -> BigInt` — ビット単位 OR。
- `bigint.bxor(a: BigInt, b: BigInt) -> BigInt` — ビット単位 XOR。
- `bigint.bnot(x: BigInt) -> BigInt` — ビット単位の補数。-x - 1 に等しい。
- `bigint.bshl(a: BigInt, n: Int) -> BigInt` — a を n ビット左シフト（a * 2^n）。n < 0 は abort。
- `bigint.bshr(a: BigInt, n: Int) -> BigInt` — a を n ビット右シフトし、-inf 方向へ丸めます（符号で充填）。n < 0 は abort。
- `bigint.from(x: S) -> BigInt` — x をゼロ方向へ丸めて BigInt にします。NaN または無限大は abort。
- `bigint.from_checked(x: S) -> Option[BigInt]` — x が整数なら some(x as BigInt)、それ以外（小数、NaN、無限大）は none。
- `type bigint.BigInt = { neg: Bool, limbs: List[Int] }` — 任意精度整数。
