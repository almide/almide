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

## ALS-E6 括弧式(`ExprKind::Paren`)

**受理形**: `( expr )`。

**値の規範**: 括弧式の値・型・効果は内側の式と同一 — 括弧は結合の優先順位を
上書きする以外に意味を持たない。`(1 + 2) * 3` は `9`、`1 + 2 * 3` は `7`
(両ターゲット実測)。fmt は冗長な括弧を**保存**する(勝手に剥がさない)。

**1-tuple との弁別の裁定**: `(e)` は括弧式、`(e,)` は **1 要素タプル** —
末尾カンマが意味を担う。fmt は 1 要素タプルの末尾カンマを往復保存する
(#1265 で修正; 回帰テスト `fmt_one_tuple_keeps_trailing_comma`)。既知の
制限: native backend は文字列補間と 1-tuple の同居で `.0` を落とせない
(#1267、loud 拒否であり誤値は出ない)。

テスト: `spec/wasm_cross/grouping_unary.almd`、`spec/wasm_cross/tuple_single.almd`
(いずれも契約 C-234)。

## ALS-E7 単項演算子(`ExprKind::Unary`)

**受理形**: 論理否定はキーワード `not`(`not b`、重ねがけ `not not b` 可)。
算術否定は前置 `-`(リテラル・変数・括弧式・浮動小数点に適用可)。

**`!` の裁定**: 前置 `!` は**受理しない** — コンパイルエラーで
`Use 'not' for boolean negation` へ誘導する(postfix `!` は unwrap、
ADR-0008)。式文位置と補間 `${...}` 内は字句経路が異なるが同じ誘導を返す
(両位置を `tests/unary_not_diag_test.rs` が pin)。

**値の規範**: `not true` → `false`、`not not b` ≡ `b`。リテラル直前の `-` は
符号込みで範囲検査に折り込まれる(ALS-E1 の裁定)。`-x`(変数)、
`-(3 + 4)`(括弧式)→ `-7`、`-(1.5)` → `-1.5` — 全て両ターゲット同一。

テスト: `spec/wasm_cross/grouping_unary.almd`(契約 C-235)、
`tests/unary_not_diag_test.rs`(負例)。

## ALS-E8 タプル(`ExprKind::Tuple` / `ExprKind::TupleIndex`)

**受理形**: リテラル `(e1, e2, …)`(要素型は異種可)。1 要素タプルは末尾
カンマ必須 `(e,)`(ALS-E6 の弁別)。位置読み出しは `.k`(k は 0 起点の
10 進桁列)。分解束縛 `let (x, y) = t`。関数のパラメータ型・返り値型として
`(A, B)` を取れる。

**連鎖 index の裁定**: `n.0.1` は**受理しない** — 字句解析が `0.1` を浮動
小数点リテラルとして読むため `Expected a name` になる。入れ子の読み出しは
`(n.0).1` と書く(実測 pin)。

**値の規範**: タプルは位置ごとに型を持つ固定長の値。`==` は要素ごとの
構造的等値。分解束縛・`.k`・パラメータ渡し・返り値のいずれの経路でも
観測値は両ターゲット同一。

**既知の制限(loud、誤値なし)**: `.k` の範囲・対象型は現状 check 時に
検査されず、範囲外や非タプル対象は build 段の拒否になる(#1266、正しくは
check 時型エラーであるべき)。native backend は文字列補間との同居で
1-tuple の `.0` を落とせない(#1267)。

テスト: `spec/wasm_cross/tuple_ops.almd`(契約 C-236)。

## ALS-E9 Option/Result コンストラクタ(`ExprKind::Some` / `ExprKind::None` / `ExprKind::Ok` / `ExprKind::Err`)

**受理形**: `some(e)`、`none`(**裸の値** — `none()` と呼ぶのは検査時
E001)、`ok(e)`、`err(e)`。すべて小文字綴り。

**値の規範**: `some(e)` は `Option[T]` を構築し、要素型は `e` から推論
される。`none` の型は注釈(`let n: Int? = none`)または消費点
(`n ?? 7`)から流れる。`ok(e)` / `err(e)` は Result 型の期待がある文脈
(注釈付き束縛・返り値位置・直接の消費)で Result を構築する。

**ok/err の文脈の裁定**: effect fn 内で `ok(e)` / `err(e)` を**無注釈の
`let` に束縛するのは検査時拒否**(E041、ADR-0008 の明示伝搬則 — ヒントは
`!`・`??`・`?`・match へ誘導する)。暗黙に Result が素通りする経路は
存在しない。負例は `tests/ctor_diag_test.rs` が両裁定を pin する。

**消費**: `??` は Ok/Some 側の値、Err/None 側でフォールバックを返す
(`ok(3) ?? 0` → `3`、`err("boom") ?? -9` → `-9`、両ターゲット実測)。
伝搬・分岐の全規範は R 系列(エラー面)を参照。

**既知の制限(loud、誤値なし)**: 入れ子 `some(none)` は検査を通り native
で正しく動くが、Option 型のデフォルトを持つ `??` は v1 renderer が wall
する(#1270)。

テスト: `spec/wasm_cross/option_result_ctors.almd`(契約 C-237)、
`tests/ctor_diag_test.rs`(負例)。

## ALS-E10 レンジ式(`ExprKind::Range`)

**受理形**: 終端排他 `start ..< end`、終端包含 `start ... end`。境界は Int の
スカラー式(リテラル・変数・呼び出し)。

**引退綴りの裁定**: `..` と `..=` は**引退済み** — 検査時 E031 で現行綴りへ
誘導し、`almide fix` が機械的に移行する。歴史的に `..` は排他・`..=` は包含
だったため、黙った読み替えはどちら向きでも off-by-one を生む — 拒否のみが
安全(負例は `tests/range_spelling_diag_test.rs` が両綴りを pin)。

**値の規範**: レンジは第一級の値 — `let r = 0..<3` と束縛してから
`for i in r` で駆動でき、インライン頭 `for i in 0..<3` と同じ列を生む。
束縛は実リスト(`list.range`)を実体化する(#1272 の回帰 pin: 遅延空値の
束縛は wasm で 0 回反復だった)。排他 `0..<3` → 0,1,2、包含 `0...5` →
0〜5、非零開始・呼び出し境界も両ターゲット同一(fixture は総和
3 / 15 / 9 / 6 を pin)。

テスト: `spec/wasm_cross/range_first_class.almd`(契約 C-238)、
`tests/range_spelling_diag_test.rs`(負例)。

## ALS-E11 リストリテラルと索引(`ExprKind::List` / `ExprKind::IndexAccess`)

**受理形**: リテラル `[e1, e2, …]`、空リスト `[]`(要素型は注釈または文脈から)、
入れ子可。読み出しは `xs[i]`(0 起点)。連結は `+`(ALS 全域の演算子多重定義:
文字列とリストの `+` は連結)。

**値の規範**: `xs[i]` は要素型の値を直接返す(Option ではない)。**範囲外は
実行時中断** — 統一メッセージ `Error: index out of bounds` + exit 1 を
両ターゲットが 3 点(stdout・stderr・終了コード)一致で出す(書き込み側の
既存契約 C-067 と同じ裁定; 黙った 0 埋めや無視は不適合)。`+` は左右の
要素を順に並べた新リストを返す。

テスト: `spec/wasm_cross/collection_literals.almd`(契約 C-239)、
範囲外中断は `spec/wasm_cross/index_bounds_write_heap.almd`(C-067)。

## ALS-E12 マップリテラル(`ExprKind::MapLiteral` / `ExprKind::EmptyMap`)

**受理形**: `["k1": v1, "k2": v2]` — **角括弧+コロン**。空マップは `[:]`
(型は注釈から)。波括弧 `{"k": v}` は**受理しない**(parse エラー —
JSON/他言語からの転記ミスは検査時に止まる)。

**値の規範**: `m[k]` は `Option[V]` を返し(リストの直接読みと対照的)、
`??` で既定値に落とす。欠損キーは `none`(実測: `m["zz"] ?? -1` → `-1`)。
挿入順は決定的に保存される(AlmideMap の規範; 反復順は挿入順)。

テスト: `spec/wasm_cross/collection_literals.almd`(契約 C-239)。

## ALS-S1 束縛文(`Stmt::Let` / `Stmt::Var` / `Stmt::Assign`)

**受理形**: 不変束縛 `let x = e`(型注釈 `let x: T = e` 任意)、可変束縛
`var x = e`、再代入 `x = e`(`var` のみ)。

**シャドーイングの裁定**: `let` の同名再束縛は**受理** — 新しい束縛の
初期化子は古い束縛を見る(`let x = 1` の後の `let x = x + 10` で `x` は
11)。これはエラーでも警告でもない(段階的な値の精製が idiom)。

**不変性の裁定**: `let` への代入は**検査時 E009** — ヒントは `var` 宣言へ
誘導する。`var` の再代入は宣言時の型に固定され、別の型での代入は
**E001**(黙った拡大なし)。負例は `tests/binding_diag_test.rs` が pin。

**値の規範**: `var` への代入は制御フロー(ループ本体・分岐)を貫いて
観測可能に反映される(fixture: 逐次 2 代入 → 10、ループ内累積 → 6)。
`let` の初期化子には条件式も取れる(`let y = if … then … else …`)。

テスト: `spec/wasm_cross/binding_stmts.almd`(契約 C-241)、
`tests/binding_diag_test.rs`(負例)。
