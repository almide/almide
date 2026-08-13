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

**入れ子の規範**: `some(none)` / `some(some(v))` は通常の Option 値であり、
Option 型のデフォルトを持つ `??` で一段ずつ剥がせる(`sn ?? none` →
内側 Option、実測: none 側 → 9、some 側 → 5、両ターゲット同一 —
#1270 で v1 の wall を閉鎖)。

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

## ALS-E13 条件式(`ExprKind::If`)

**受理形**: `if cond then arm`(else 省略可、文位置)、`if cond then arm
else arm`、アームは単一式または brace ブロック `then { … } else { … }`、
連鎖 `else if`。条件は `Bool`。

**`then` の裁定**: `then` は**必須** — brace 言語からの転記形
`if cond { … }` は parse エラーで `if requires 'then'` と誘導する
(負例は `tests/if_then_diag_test.rs` が pin)。

**値の規範**: 条件式は値を持つ(`let g = if … then "low" else if … then
"mid" else "high"`)。else 省略形は文位置でのみ許され、値は `Unit`。取られた
アームだけが効果を発火する(取られないアームの効果が観測されたら不適合 —
v1 lowering の both-arms 線形化 wall はこの規範の防衛)。

テスト: `spec/wasm_cross/if_block_forms.almd`(契約 C-242)、
`tests/if_then_diag_test.rs`(負例)。

## ALS-E14 ブロック式(`ExprKind::Block`)

**受理形**: `{ stmt* tail? }` — 文の列に任意の末尾式。

**値の規範**: ブロックの値は**末尾式の値**(なければ `Unit`)。ブロックは
式であり、`let` の初期化子・`if` のアーム・関数本体のいずれにも置ける。
ブロック内の束縛はブロックスコープ(fixture: `let b = { let t = a * 2
t + 1 }` → 11、if アーム内ブロック → 9、両ターゲット同一)。

テスト: `spec/wasm_cross/if_block_forms.almd`(契約 C-243)。

## ALS-E15 while 文(`ExprKind::While`)

**受理形**: `while cond { body }` — 条件は `Bool`、本体は brace ブロック。
前判定(条件が最初から偽なら本体は 0 回)。

**値の規範**: 文であり値は `Unit`。可変状態(`var`)への代入が反復を跨いで
観測可能に累積する(fixture: カウンタ和 10 / 終端値 5、データ依存の反復数
— Collatz(16) → 4 ステップ — 両ターゲット同一)。条件は各反復の先頭で
再評価される。

**break/continue**: 文位置のガード形が両ターゲットで実行される
(ALS-E24、#1306 で brick 着地)。ヒープフレーム跨ぎは wall 維持。

テスト: `spec/wasm_cross/while_loops.almd`(契約 C-244)。

## ALS-E16 文字列補間(`ExprKind::InterpolatedString`)

**受理形**: `"text ${expr} text"` — セグメントは任意個、`expr` は式
(変数参照・算術式を実測 pin)。リテラルの `${` は `\${` でエスケープする
(実測: `"escaped \${not_interp}"` → `escaped ${not_interp}`)。

**値の規範**: 各セグメントの値をその型の**正準表示形**で埋め込む — String
はそのまま、Int は 10 進(ALS-E1)、Bool は小文字(ALS-E2)、Float は正準
形(`1.5` を実測 pin; 網羅的な浮動小数点表示規範は ALS-E3/T2 の裁定後)。
補間全体の型は `String`。

**`!` の裁定**: 補間内の前置 `!` は式文位置と同じ `not` 誘導で拒否される
(ALS-E7、`tests/unary_not_diag_test.rs` の補間側ケース)。

テスト: `spec/wasm_cross/string_interpolation.almd`(契約 C-245)。

## ALS-E17 識別子(`ExprKind::Ident`)

**受理形**: 束縛済みの名前。解決は**最も近い束縛**(シャドーイングの規範は
ALS-S1)。

**裁定**: 未解決の名前は**検査時 E003** — 診断は解決に失敗した名前そのもの
を含む(どの識別子が誤りかを診断単体で特定できる;
`tests/ident_diag_test.rs` が pin)。実行時の名前解決失敗は存在しない
(検査を通った名前は必ず束縛に解決される — v1 は NameTotality の証明対象)。

テスト: `spec/wasm_cross/string_interpolation.almd`(契約 C-246)、
`tests/ident_diag_test.rs`(負例)。

## ALS-E18 match 式(`ExprKind::Match`)

**受理形**: `match subject { pattern [if guard] => expr, … }` — パターンは
リテラル(文字列・整数)、ワイルドカード `_`、束縛子 `x`、ガード付き束縛子
`x if cond`、Option の `some(x)` / `none`、Result の `ok(x)` / `err(e)`。
アーム本体は式(補間を含む)。

**選択の裁定**: アームは**先頭から順に最初に一致したもの**が取られる
(fixture: `n = 7` は `x if x < 0` と `x if x % 2 == 0` を通過して `_` に
落ちる — first-match-wins を実測 pin)。取られたアームだけが評価される。

**網羅性の裁定**: 非網羅の match は**検査時 E010** — 診断は欠落ケースを
**名指し**し(`missing none`)、arm 追加のヒントを添える
(`tests/match_diag_test.rs` が pin)。実行時の「どのアームにも一致しない」
は型検査を通った match には存在しない。

テスト: `spec/wasm_cross/match_forms.almd`(契約 C-247)、
`tests/match_diag_test.rs`(負例)。

## ALS-E19 for-in 文(`ExprKind::ForIn`)

**受理形**: `for pat in iterable { body }` — 反復対象はリスト・レンジ
(ALS-E10)・マップ(ALS-E12 の `(k, v)` 反復)。`pat` は束縛子、または
タプル要素リストに対する分解 `(i, v)`。

**値の規範**: 文であり値は `Unit`。要素は先頭から順に一回ずつ束縛され、
本体の可変状態への代入は反復を跨いで累積する(fixture: List[Int] 累積 →
8、List[String] 要素出力、List[(Int, Int)] の分解ヘッド → 53)。反復対象は
ループ開始前に一度だけ評価される。

**既知の制限(loud、誤値なし)**: `break` / `continue` は while と同じく
v1 の wall(#1277)。

テスト: `spec/wasm_cross/for_in_forms.almd`(契約 C-248)、レンジ head は
`range_first_class.almd`(C-238)、マップ反復は
`collection_literals.almd`(C-240)。

## ALS-S2 分解束縛(`Stmt::LetDestructure`)

**受理形**: `let (a, b, …) = e` — 右辺はタプル。各成分が対応位置の値に
不変束縛される。

**値の規範**: 成分ごとの型は右辺タプルの位置型(fixture: `let (p, q) =
(1, 2)` の後 `p + q` が加算に参加 — 実測 53 の内訳)。関数パラメータ位置の
タプル分解(`((idx, case)) =>`)・for-in ヘッドの分解(ALS-E19)と同じ
規範を共有する。

テスト: `spec/wasm_cross/for_in_forms.almd`(契約 C-249)、
`spec/wasm_cross/tuple_ops.almd`(C-236 の `let (x, y) = t`)。

## ALS-E20 パイプと合成(`ExprKind::Pipe` / `ExprKind::Compose`)

**受理形**: `x |> f`(単段・連鎖 `x |> f |> g`・HOF への流し込み
`xs |> list.map((x) => …) |> list.len`)。合成は `f >> g` — 呼び出し可能な
値を返し、`(f >> g)(x)` は `g(f(x))`。

**値の規範**: `x |> f` ≡ `f(x)`(観測等価; fixture: `5 |> double` → 10、
`5 |> double |> inc` → 11、HOF 連鎖 → 3)。`>>` は左から右への適用順
(`double >> inc` に 4 → `inc(double(4))` = 9、両ターゲット同一)。

テスト: `spec/wasm_cross/pipe_compose_forms.almd`(契約 C-250)。

## ALS-E21 if let(`ExprKind::IfLet`)

**受理形**: `if let x = o { … } else { … }` — 束縛子は**裸の識別子**。
`if let some(x) = o` は parse エラー(束縛子位置にパターンは書けない —
Swift 型の暗黙 unwrap が設計)。

**値の規範**: 審査対象は Option。some なら内側の値が `x` に束縛され
then アームが、none なら else アームが走る(fixture: some(5) → 10、
none → "empty"、両ターゲット同一)。

テスト: `spec/wasm_cross/if_let_forms.almd`(契約 C-251)。

## ALS-S3 式文(`Stmt::Expr`)

**受理形**: 文位置の式。`Unit` を返す呼び出し(`println(…)` 等)はそのまま
文になる。値を持つ純粋式の意図的破棄は `let _ = e`。

**must-use の裁定**: Result を返す呼び出しを文位置で裸のまま置くのは
**検査時 E042**(ADR-0008)— エラーの黙った握り潰しは存在しない。逃げ道は
二つだけで、ヒントが両方を綴る: `expr!`(伝搬)と `let _ = expr`(明示
破棄; err は伝搬しない — C-217)。負例は `tests/expr_stmt_diag_test.rs` が
pin。

テスト: `spec/wasm_cross/expr_stmt_comment.almd`(契約 C-252)、
`tests/expr_stmt_diag_test.rs`(負例)。

## ALS-S4 コメント(`Stmt::Comment`)

**受理形**: 行コメント `// …`(行頭・行末尾)、ブロックコメント
`/* … */`(複数行可、宣言前・文間)。

**値の規範**: コメントは**意味論的に不可視** — どの位置のどの形も観測可能
挙動に一切寄与しない(fixture: 全形を挟んだ出力が両ターゲットで
コメント無しと同一)。fmt はコメントを保存する(comment_map / doc_map が
宣言単位で担持)。

テスト: `spec/wasm_cross/expr_stmt_comment.almd`(契約 C-252)。

## ALS-S5 場所代入(`Stmt::IndexAssign` / `Stmt::FieldAssign`)

**受理形**: リスト要素 `xs[i] = v`、レコードフィールド `p.x = v`、マップ
キー `m[k] = v`(存在キーは上書き、不在キーは挿入 — upsert)。対象は
`var` 束縛(`let` への場所代入は不変性違反、ALS-S1 の E009 系)。

**値の規範**: 代入後の読みは新値を観測する(fixture: 21 / 12 / 6 / 100、
両ターゲット同一)。値意味論 — 代入は共有を通じて漏れない(COW)。**範囲外
インデックス書きは実行時中断**(統一メッセージ + exit 1、C-067 の裁定)。

テスト: `spec/wasm_cross/place_assign_ascription.almd`(契約 C-253)、
範囲外は `spec/wasm_cross/index_bounds_write_heap.almd`(C-067)。

## ALS-E22 型注釈式(`ExprKind::TypeAscription`)

**受理形**: `(e: T)` — 式位置の型注釈。

**値の規範**: 値は `e` のまま、型検査の期待型として `T` を供給する
(fixture: `(7: Int)` → 7)。実行時表現に影響しない。

テスト: `spec/wasm_cross/place_assign_ascription.almd`(契約 C-253)。

## ALS-E23 レコード(`ExprKind::Record` / `ExprKind::SpreadRecord` / `ExprKind::Member`)

**受理形**: リテラル `{ x: 1, y: 2, tag: "a" }`(型は宣言 `type Pt = { … }`
または文脈から)、フィールド読み `p.x`、スプレッド更新 `{ ...p, y: 20 }`
(列挙外フィールドは継承)。

**値の規範**: フィールド読みは宣言型の値を返す。スプレッドは**新しい値**を
作り、**元の値は不変**(fixture: `q = { ...p, y: 20 }` 後も `p.y` は 2 —
値意味論、共有を通じた変異は観測されない)。フィールドへの代入は `var`
束縛上でのみ(ALS-S5)。

テスト: `spec/wasm_cross/record_forms.almd`(契約 C-255)。

## ALS-E24 break と continue(`ExprKind::Break` / `ExprKind::Continue`)

**受理形**: ループ本体(`while` / `for-in`)内の文位置 `break` /
`continue` — 裸、または `if cond then break` / `if cond then continue` の
ガード形。

**値の規範**: `continue` は現在の反復の残りを飛ばして次反復へ
(**for-range では step が必ず実行される** — 飛ばすと無限ループになる罠は
lowering が構造的に排除)。`break` はループを即座に抜ける。**mid-body の
break はその位置で即時**に効く(fixture: `if k > 3 then break; last = k`
→ 3 — 遅延フラグ化で 4 になる誤りは #1306 で修正、v0・interp・v1 native・
v1 wasm の 4 者一致)。

**3-way の裁定**: この fixture は意図的に spec/wasm_cross に置く — #1306 の
発見(両 v1 レグが同じ誤値で一致し 2-way ゲートに不可視)は interp が投票
する 3-way でのみ検出できるクラス。ヒープフレームを跨ぐ break は per-
iteration Drop を飛ばすため引き続き wall(loud)。

テスト: `spec/wasm_cross/loop_break_continue.almd`(契約 C-256)、
`spec/lang/loop_break_continue_test.almd`(wasm レグ)。

## ALS-E25 エラー演算子(`ExprKind::Unwrap` / `ExprKind::ToOption` / `ExprKind::UnwrapOr` / `ExprKind::Try`)

**受理形**: 後置 `e!`(unwrap-伝搬 — effect fn 内のみ、ADR-0008)、後置
`e?`(Result → Option 変換)、`e ?? d`(Ok/Some の値、Err/None で `d`)。
改行を跨ぐ後置 `?` / `?.` は受理しない(行頭の `?` は別の式)。

**値の規範**: `half(10)!` → 5(Ok 経路; Err なら呼び出し元へ伝搬 —
ALS-R1 の abort 形へ)。`err(_)?` → `none`、`ok(4)?` → `some(4)`(fixture:
-1 / 4)。`??` の消費は ALS-E9 と共通。詳細な伝搬規範は ADR-0006/0008 の
契約群(C-216・C-217・C-222)が pin する。

**Try の裁定**: `ExprKind::Try` は**パーサから生成されない死語彙** —
表層の後置 `?` は `ToOption` を構築する。ノードは AST に残るが構文に
対応物を持たない(削除は語彙掃除の followup、#1300 の CallArg::Label と
同類)。

**`?.` は未節化**: OptionalChain の record 主体は wasm の untracked-subject
match wall(loud)に当たるため、行は UNWRITTEN 維持。

テスト: `spec/wasm_cross/error_operators.almd`(契約 C-257)。

## ALS-E26 呼び出しとラムダ(`ExprKind::Call` / `ExprKind::Lambda`)

**受理形**: 名前呼び出し `f(a, b)`(位置引数)、ラムダ `(x: Int) => e`
(型注釈任意)・`(x) => e`・零引数 `() => e`、HOF 引数位置のインライン
ラムダ。effect 呼び出しの消費規範は ALS-S3/E25(E041/E042)。

**値の規範**: 引数は先頭から順に一度ずつ評価され、値渡し(値意味論)。
ラムダは関数値 — 束縛して呼ぶ・引数に渡すの両経路が同一観測
(fixture: 5 / 10 / 42 / 12、両ターゲット同一)。アリティ不一致は検査時
E004。

テスト: `spec/wasm_cross/call_lambda_ctor.almd`(契約 C-258)。

## ALS-E27 コンストラクタ参照(`ExprKind::TypeName`)

**受理形**: 型宣言 `type Color = Red | Green | Rgb(Int, Int, Int)` の
ケース名 — ペイロード付きは呼び出し形 `Rgb(1, 2, 3)`、無引数ケースは裸の
参照 `Green`。

**値の規範**: 構築した variant 値は match のケースパターンで消費される
(fixture: `Rgb(r, _, b) => r + b` → 4、`Green` → 20、両ターゲット同一)。
match の網羅性は ALS-E18(E010)。

テスト: `spec/wasm_cross/call_lambda_ctor.almd`(契約 C-259)。

## ALS-D1 宣言(`Decl::Module` / `Decl::Import` / `Decl::Type` / `Decl::Fn` / `Decl::TopLet` / `Decl::Protocol` / `Decl::Test` / `Decl::TestWhereDef`)

**受理形**: `module name`(先頭、任意)/ `import mod`(自動 import 外の
stdlib と外部パッケージに必須 — ALS の import 規範は module-system spec)/
`type T = Case | Case(payload)` と `type T = { fields }` / `fn f(…) -> T =
…`・`effect fn …` / トップレベル `let NAME = value`(fn 本体から参照可)/
`protocol P { fn sig }` と `fn T.method(…)` 実装 / `test "name" { … }`
ブロックと `local test where` / `mod test where` 定義。

**値の規範**: fixture が import(json の parse→stringify 往復)・variant
型・トップ let(Int/String)・fn/effect fn を一本で貫き、両ターゲット同一
(300 / hi / careful / [1,2])。`module` ヘッダの実行 evidence は
`spec/integration/modules/` 群(実 module ヘッダで CI 毎回実行; fixture に
置けないのは fmt の並べ替えバグ #1323 のため — fmt(valid)→invalid 族)。protocol の解決規範は既存契約
C-094/C-126(`protocol_ufcs_inferred_lambda.almd`)が pin。test ブロックは
`almide test` の wasm レグで常時実行される(spec/lang 全域が evidence;
`test where` は `spec/lang/test_where_test.almd`)。

**Strict は未節化**: `strict <mode>` は受理されるが現状どの層も消費しない
(#1321 — accept-and-ignored)。裁定が下るまで UNWRITTEN 維持。

テスト: `spec/wasm_cross/declaration_forms.almd`(契約 C-260)。

## ALS-E3 浮動小数点リテラル(`ExprKind::Float`)— 部分節

**受理形(確定分)**: 10 進小数 `1.5`・`0.5`(小数点の両側に数字必須 —
`.5`/`1.` は parse エラー、実測)・負零 `-0.0`。指数形(`1e10`/`1E10`/
`1.5e-3`)と下線区切り(`1_000.25`)は**受理される**が、fmt が値経由で
綴りを再印字するため fixture で pin できない — **形の規範化は OPEN
(#1261 の裁定待ち)**。範囲外リテラル(`1e999`)の扱いも同 issue の
フォーク(現状 inf 飽和、fmt が unparseable `inf.0` を出す)。

**値の規範(確定分)**: 型は binary64(ALS-T2)。正準表示は
`float.to_string` — `1.5` / `0.5` / **`-0.0`(符号保存)** / 演算結果
`1.75` を fixture が両ターゲット同一で pin。

テスト: `spec/wasm_cross/string_float_stable.almd`(契約 C-261)。

## ALS-E5 文字列リテラル(`ExprKind::String`)— 部分節

**受理形(確定分)**: 二重引用符リテラル、空文字列 `""`、エスケープ
`\t`・`\n`・`\\`(綴りとも fmt 安定、値を fixture が pin)。
`\u{…}`(値置換される)・heredoc `"""…"""`(単一行へ潰される)・引用符
選択(自動切替)は受理されるが fmt が形を保存しないため**形の規範化は
OPEN(#1263)**。未知エスケープ(`\q`)と範囲外 `\u{…}` の黙過は
**OPEN(#1264)** — 現状は素通りであり、これを規範とは認めない。

**値の規範(確定分)**: `\t` → タブ、`\n` → 改行、`\\` → バックスラッシュ
1 字(fixture が可視括りで pin)。空文字列は長さ 0 の値。

テスト: `spec/wasm_cross/string_float_stable.almd`(契約 C-262)。
