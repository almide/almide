<a id="als--言語意味論semantics"></a>

# ALS — 言語意味論（Semantics）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](semantics.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](semantics.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](semantics.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/semantics.md)の非規範的な読者向けコピーです。規範としての位置付けと契約参照は正本の原文に帰属し、このコピーが新たな正本になるものではありません。コミット固定のスナップショットであり、生成処理の入力でも自動同期文書でもありません。原本および機械的な参照先は変更していません。例・測定値・ターゲット間同等性・適合性の主張を独立に実行・再検証していません。

> Last updated: 2026-08-13

パターン・レコード・変種・effect-fn・ジェネリクス・ループ蓄積の観測規範。
各節は契約台帳（`spec` キー）から参照され、検証テストは各契約の evidence
（spec/wasm_cross fixture）が担う。

<a id="als-m1-パターンマッチの束縛規範"></a>

## ALS-M1 パターンマッチの束縛規範

Option/Result の構築とマッチ、ネストしたコンストラクタパターン、変種
コンストラクタを試すタプルパターン、let 分解のネスト部分パターンは、
**全ての葉を宣言通りに束縛**し、マッチは上から最初の一致アームを実行する。
heap payload（String 等）の束縛は所有を移さず借用として読める。
アーム本体が更に variant match であるネスト形も、`let` 束縛（heap のため
branch_lift がヘルパ fn へ持ち上げる位置）で全レベルの payload を束縛する。

**コンストラクタパターンは subject が持つケースを名指す（静的規範）**:
`some`/`none` は Option を、`ok`/`err` は Result を分解し、ユーザー変種は
自分のケースだけで match する。subject が持たないケースを名指すパターンは
**チェック時 E048** で拒否される — 両ターゲット同一（型検査はターゲット分岐
より前）。4 セル: 族違い 2 方向、ユーザー変種への組込みキャリア、他変種
（またはキャリア）からのユーザーケース。#1341 以前はスカラ subject だけを
拒否していたため、`match list.get(xs, 0) { ok(a) => … }`（`list.get` は `T?`）
は payload を `Ty::Unknown` で束縛したまま codegen まで生き延び、native では
`[COMPILER BUG]` バナー（ユーザーケース違いでは**不正な Rust 出力**）、wasm
では実行可能部分集合の wall になっていた。構造が確定した subject のみを対象と
し、推論中の型や opaque alias の `Named` は黙って通す。
テスト: `spec/wasm_cross/nested_variant_match_bind.almd`,
`tests/diagnostics/e048-cross-family-variant-pattern/`,
`tests/diagnostics/e048-foreign-ctor-case/`。
Contracts: C-044, C-070, C-073, C-091, C-113, C-114, C-269。

<a id="als-m2-レコード意味論"></a>

## ALS-M2 レコード意味論

レコードの spread 更新 `{ ...r, f: v }` は**新しい値**を作る（元は不変 —
ALS-C5 の値意味論に従う）。フィールド表現は推論・明示で同一。ジェネリック
フィールドは構築時のインスタンス型でサイズされる。phantom 型パラメータは
観測表現に影響しない。クロスモジュールの単相化も同一の観測。
Contracts: C-046, C-072, C-078, C-092, C-123。

<a id="als-m3-変種型adt"></a>

## ALS-M3 変種型（ADT）

ユーザー定義変種は名前空間が独立（`Box` 等の標準名と共存）。相互再帰変種・
匿名レコードを payload に持つ変種・モジュール内での構築と外部からの観測は
すべて宣言通りで、ターゲットに依存しない。
Contracts: C-043, C-076, C-079, C-093。

<a id="als-m4-effect-fn-の脱糖規範"></a>

## ALS-M4 effect fn の脱糖規範

> **整合注記（非規範、2026-10-31）**: 以下の auto-`?` の説明は旧実装の
> 記録である。採択済み [ADR-0008](../../../../../adr/0008-explicit-propagation-only.md)
> と C-217 は暗黙伝搬の廃止、C-345 は引数位置での Result 保持を記録する。
> 現行の利用説明は [effect-fn-call-semantics](../../../../../specs/effect-fn-call-semantics.md)
> と `spec/lang/explicit_propagation_test.almd` を参照。
> C-064 の契約本文には旧 auto-`?` の記述が残る。この注記はその契約を
> 書き換えず、judge 側で解消すべき不整合を明示するものである。

effect fn 内の can-err 呼び出しへの自動 `?` 付与は**束縛形に依存しない**
（let / tail / 引数位置で同一規則）。構築子位置の auto-`?` も同一。`!` の
Err は `for` ループ本体からも関数境界へ伝播する。末尾自己再帰は(明示の
`err($var)` 再包み tail — C-069 が v1 レグでループ書き換えを意図的に辞退する形
— を除き)**O(1)
スタックのループへ変換**される（深さ 10^6 でも溢れない）。
Contracts: C-064, C-068, C-069, C-119, C-135。

<a id="als-m5-ジェネリクスと推論"></a>

## ALS-M5 ジェネリクスと推論

空の `map.from_list`/`set.from_list` は使用側から要素型を解決する。推論
パラメータのラムダ内のジェネリック関数・UFCS プロトコル呼び・ネストした
ラムダの HOF パラメータは推論リンクを保つ。型パラメータ上の `+` は
実型（文字列/リスト/数値）の規範に従う。先行パラメータを参照する
デフォルト引数は呼び出しごとに評価される。`unwrap_or` の payload 型は
デフォルト側からも解決される。コンビネータは Float instantiation でも
両ターゲットで有効なコードに落ちる。
Contracts: C-080, C-081, C-082, C-089, C-094, C-097, C-126, C-127, C-142, C-145, C-151。

<a id="als-m6-蓄積ループの規範"></a>

## ALS-M6 蓄積ループの規範

`var` 蓄積（scalar・所有ハンドル・相互依存・変異混在）、リスト反復と
call 結果要素の実体化、末尾再帰蓄積形、ループ内の let-bound heap 分岐は、
逐次意味論（1反復ずつ順に評価した結果）と観測一致する。
Contracts: C-102, C-104, C-105, C-117, C-174。

<a id="als-m7-識別子の独立性"></a>

## ALS-M7 識別子の独立性

関数・変数名はターゲット言語の予約語（Rust キーワード等）と衝突しても
そのまま使える。名前がコード生成の都合で観測に漏れてはならない。
Contracts: C-088。

<a id="als-m8-演算子の全域規範"></a>

## ALS-M8 演算子の全域規範

比較・等価演算子は全オペランド形（リテラル・変数・フィールド・call 結果・
複合値）で同一の結果。`-9223372036854775808`（i64::MIN）リテラルは
表現可能な値であり 0 へ畳んではならない。
Contracts: C-083, C-099。

<a id="als-m9-静的検査unit-変異子"></a>

## ALS-M9 静的検査（Unit 変異子）

Unit を返す in-place 変異子の結果を**非 Unit 束縛へ再代入**する式
（`xs = list.push(xs, x)`）は **E001** で拒否される — 変異子は in-place で
あり、その「戻り値」を書き戻す綴りは型不一致として検出される。素の
`let x = list.push(…)` は Unit を束縛するだけで、未使用警告に留まる
（0.59.1 実測）。
Contracts: C-057。

<a id="als-m10-分岐からの-heap-束縛"></a>

## ALS-M10 分岐からの heap 束縛

if/match アームから束縛される heap 値（`let s = if c then "a" else make()`）
は選択されたアームの値そのものであり、空値・別アームの値の観測は不適合。
ブロック本体のラムダへのパイプも同様。
Contracts: C-106, C-115。

<a id="als-m11-unwrap-の脱糖"></a>

## ALS-M11 unwrap の脱糖

> **文脈の補足（非規範）**: 以下は C-108 が扱う Result の伝搬文脈の説明。
> `test` 内などの plain unwrap（C-222 が区別する失敗時停止）や Option の
> `!` まで、すべて Err 返却に読み替えるものではない。文脈別の利用説明は
> [result-option-effect](../../../../../specs/result-option-effect.md) を参照。

`!`（unwrap）と let-unwrap は全ての位置（let / tail / 引数 / ネスト）で
「Ok payload を取り出し、Err なら関数の Err として伝播」の単一規則。
Contracts: C-108。

<a id="als-m12-heap-要素リスト操作の一般性"></a>

## ALS-M12 heap 要素リスト操作の一般性

List[String]/List[Value] のパラメータ渡し・join/len/添字/反復・分類/変換
ops（zip_with の要素型ルーティングを含む）は要素の所有を壊さず、native と
観測一致する。
Contracts: C-045, C-100, C-101, C-141, C-147, C-148。

<a id="als-m13-mut-パラメータの-in-place-変異"></a>

## ALS-M13 mut パラメータの in-place 変異

`mut` パラメータへの in-place 変異は呼び出し元の `var` に反映される（ALS-C5 の
例外）。呼び出し側が渡せるのは `var` 束縛だけで、`let`・非 `mut` パラメータ・
一時値は E032。この例外を開くのは宣言された `mut` だけである: 標準ライブラリの
in-place 変異子（`list.push`/`pop`/`clear`、`map.clear`、
`string.push`/`clear`、`bytes` の書き込み族）も受け手を `mut` と宣言しており、
非 `mut` パラメータ越しに呼び出し元の値が変わる経路は無い（`Bytes` の素の
パラメータ経路は dialect epoch 5 で閉じた、C-132）。**再割り当てする**
コンテナ（List / String / Bytes の容量成長）でも成立し、値返しの callee・
入れ子式位置・レコードフィールド実引数（`f(b.items, …)`）を含む全呼び出し
位置で呼び出し元スロットに書き戻される。実引数は呼び出し時に複写され、
呼び出しの後に書き戻される: callee が実引数の根である module-level `var` を
読むと、呼び出し前の値が見える（C-132）。
Contracts: C-061, C-110, C-132, C-136。

<a id="als-m14-整数リテラルの型域静的規範"></a>

## ALS-M14 整数リテラルの型域（静的規範）

整数リテラルは**文脈の型が表現できる値**でなければならない。表現できない
リテラルは両ターゲットで **checker エラー（E024）** であり、暗黙に 0 へ
畳まれることも、片方のターゲットだけがビルドに失敗することもない。
逸脱は 3 種あり、それぞれ別の修正を要する:

- **大きさ** — 幅に対して値が大きすぎる（`Int` に `i64::MAX + 1` 等）。
  小さいリテラル、または広い型で修正する。
- **符号** — 符号なし型に負リテラル（`UInt64` に `-5` 等）。符号なし型は
  負値を一切持たないため大きさの問題ではない。`-` を外すか、同じ幅の
  符号付き型（`UInt64` → `Int64`）で修正する。`-0` は `0` なので適合。
- **キャリア（解消済み）** — かつて `18446744073709551615` 等の UInt64 上半分
  は E024 だったが、符号なしレーンが 0.37.0 で入り(C-179、C-173 の
  SUPERSEDED 節)、現在は受理され unsigned のまま演算・印字される。
  残る逸脱は上の 2 種(大きさ・符号)である。

Contracts: C-173, C-179。

<a id="als-m15-effect-fn-型スロット"></a>

## ALS-M15 effect fn 型スロット

fn 型は `effect` 前置で**効果スロット**になる: `effect (A) -> B`(または
`effect fn(A) -> B`)。効果スロットに検査される lambda は effect fn の本体
規律を得る — effect 呼び出しが許可され、`!` はその lambda 自身の失敗
チャネルへ伝搬する。スロットの実行時形状は効果キャリア
`(A) -> Result[B, String]` の**一形**であり、pure な lambda は値 tail が
`ok(...)` に包まれ、`!` を使う lambda はそのまま — 呼び出し側(HOF)は
どちらの綴りを受けても同じ形を受け取る。

- 効果スロットは pure lambda と可謬 lambda の**両方**を受理する。
- effect fn 型の**値**の呼び出しはそれ自体が effect 呼び出しであり、
  キャリア `Result[B, String]` を生む(pure 文脈からは E006)。
- `effect` を書いていない素の `(A) -> B` スロットでは、#1108 2b-iii の
  Cell 1 サブセット内の可謬 lambda は生成された `__fallible__` twin へ
  ルーティングされて**実行される**(C-221、`fallible_user_hof_test.almd`)。
  サブセット外は従来どおり E005。
- 逸脱(片ターゲットのみ ok-wrap が落ちる・err メッセージが異なる等)は
  C-221 の fixture が検出する。


## 原文の制約に関する読者向け注記

原文は ALS-M4 の自動伝搬の説明を旧実装と明示し、C-064 の未解消の不整合を指摘しています。このコピーは説明と限定注記の両方を保持し、暗黙伝搬を復活させるものではありません。ALS-M11 は指定された Result の伝搬文脈の説明であり、すべての unwrap には一般化しません。ALS-M15 の Cell 1 部分集合、生成された twin、部分集合外の E005 は原文に帰属する主張で、現行のあらゆる HOF を独立検証した記述ではありません。0.59.1 の実測、v1 の末尾再帰例外、符号なしキャリアの履歴、推論中の型・opaque alias の例外も、原文の範囲を保っています。
