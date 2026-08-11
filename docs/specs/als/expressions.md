# ALS §E — Expression Semantics (normative)

> Last updated: 2026-08-12

> **Status**: normative. これらの節は実装から独立した**規範**であり、v0(native)と
> v1(MIR/wasm)の両実装がこの節に適合する義務を負う。適合の証拠は
> `spec/wasm_cross/` の対応 fixture(3点観測: stdout・stderr・exit code)。
> 本ファイルは**構文要素方向**(proofs/als-element-coverage.toml)の最初の節群 —
> 各節は「受理形の文法」「値の規範」「裁定」「fixture」の四部形式を雛形とする。
> 全主張は制定時に両ターゲットで実測済み(推測による規範化は F1 违反)。

## ALS-E1 整数リテラル(`ExprKind::Int`)

**受理形**:

```
int     := dec | hex | oct | bin
dec     := digit (digit | "_")*
hex     := "0x" hexdigit (hexdigit | "_")*
oct     := "0o" octdigit (octdigit | "_")*
bin     := "0b" bindigit (bindigit | "_")*
```

アンダースコアは桁区切りとしてどの基数でも受理され、値に影響しない。

**値の規範**: 整数リテラルの型は `Int` = **64 ビット符号付き 2 の補数整数**
(i64)。表せる範囲は −9223372036854775808 〜 9223372036854775807。

**範囲の裁定**: 範囲外のリテラルは**検査時エラー E024**(`integer literal
'…' is out of range for Int`)であり、黙った折返しや飽和は不適合。**単項マイナス
直後のリテラル**は符号込みで範囲検査する — したがって最小値
`-9223372036854775808` はリテラルとして直接書ける(マイナスと数字の間の空白は
許容)。この裁定がないと i64::MIN は `-9223372036854775807 - 1` としか書けず、
定数表の書き写しがエラーの温床になる。

**表示との往復**: `int.to_string` は最小値を含む全域で 10 進表記を返す
(i64::MIN の桁列 `-9223372036854775808` を含む — v1 wasm の `$itoa` は
この edge を実行ピンで検証済み)。

テスト: `spec/wasm_cross/literal_int_forms.almd`(契約 C-231)。

## ALS-E2 真偽リテラル(`ExprKind::Bool`)

**受理形**: キーワード `true` / `false`(予約語であり識別子に使えない)。

**値の規範**: 型は `Bool`。表示形は文字列補間・`==` の被演算子など全観測点で
`true` / `false` の小文字綴りとする。

テスト: `spec/wasm_cross/literal_int_forms.almd`(契約 C-231)。
