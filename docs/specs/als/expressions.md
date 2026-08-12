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

## ALS-E4 ユニットリテラル(`ExprKind::Unit`)

**受理形**: `()`(空の括弧対)。

**値の規範**: 型は `Unit`。`Unit` の値はちょうど一つであり、`()` はその唯一の
リテラル表記。式位置・`let` 束縛位置・`-> Unit` 関数の本体として受理され、
ブロックの末尾式として関数の返り値になる。

**等値性の裁定**: `Unit` 上の `==` は**反射的に真**(`() == ()` → `true`、
`!=` → `false`)。住人が一つの型に実行時の読み取りは存在しないため、v1
lowering は **call-free なオペランド対に限り**この比較を定数 Bool に畳む。
呼び出しを含むオペランド(`f() == ()`)は畳まない — 呼び出しの効果が消える
のは miscompile であり、既存の wall に落ちて拒否される(正直な未対応 >
黙った誤答)。この畳み込み以前は Eq-over-Unit が v1 で全位置 wall であり、
等値性の証拠は native 単独だった — 現在は両ターゲットの実行 pin。

テスト: `spec/wasm_cross/literal_unit.almd`(契約 C-233)、
`spec/lang/unit_literal_test.almd`(assert 形、wasm 脚で実行)。

> **ALS-E3(浮動小数点リテラル)は未執筆**: fixture が `almide fmt` の
> spec ゲートを通過できない(fmt が指数・下線綴りを f64 値経由で正規化する
> — #1261)。裁定が下り fmt が綴りを保存するまで、
> proofs/als-element-coverage.toml の `ExprKind::Float` 行は UNWRITTEN の
> まま据え置く(F1: fixture なき規範化はしない)。
