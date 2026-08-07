# ADR-0010: T? is general Option shorthand — every type position, atom-tight binding, fmt-normalized

- **Status**: Accepted
- **Date**: 2026-08-06
- **決定範囲**: 型表記 `T?` の採否・有効位置・結合規則・正準形(fmt の正規化方向)。
  式演算子の `?`(Result→Option)/`??`/`?.` は対象外(ADR-0005 で確定済み)。
- **関連**: [ADR-0002](./0002-fallibility-effect-orthogonal.md)(`-> T!` の先例。
  同 ADR の Alternatives 5 で「`T?` はスコープ外・採否は別議論」として分離されたのが本件)、
  [ADR-0009](./0009-fn-type-quadrant-transparency.md)(fn 型 slot の `!` — 結合の向きを揃える)
- **経緯**: 2026-08-06、v0.54.0 リリース直後に○×裁定 3 問(Q1 スコープ / Q2 結合 /
  Q3 正規化)で確定。

## Context

`-> T!`(ADR-0002)で可謬性は 1 文字になったが、不在(Option)は 19 文字
`Option[T]` のまま。コーパス実測(2026-08-06):

| Option 注釈の位置 | 件数 |
|---|---|
| 戻り位置 `-> Option[T]` | 387 |
| 引数・フィールド位置 `: Option[T]` | 331 |
| ジェネリック引数位置 `[Option[T]]` | 147 |
| 入れ子 `Option[Option[T]]` | 45 |
| fn 型 slot の戻り `(A) -> Option[B]` | 52 |
| fn 型そのものを包む `Option[(A) -> B]` | **0** |

`!` は「関数を呼ぶことの可謬性」= 矢印の属性だから戻り位置限定が正しい。
`?` は「値の不在」= 型の属性で、実測でも過半(478/865)が戻り位置の外にいる。

## Decision

**D1(Q1 ○): `T?` ≡ `Option[T]` を全型位置で有効な一般型糖衣として採用する。**
戻り位置限定にしない — 位置依存の表記規則は v0.51.0 が殺した surface-rule
違反(writer が一般化した規則が場所で死ぬ)の再演になる。デノテーション規則は
1 行固定: `T?` と `Option[T]` は同一デノテーション(`16` と `0x10` の関係)。

**D2(Q2 ○): `?` は直前の型アトムに最結合し、`->` をまたがない。**
型アトム = 名前(+ジェネリクス、モジュール修飾可)または括弧で閉じた型。
fn 型全体・入れ子 Option は括弧で明示する:

```almide
f: (Int) -> Int?            // = (Int) -> Option[Int]   実在 52 件の形
on_tick: ((Int) -> Unit)?   // = Option[(Int) -> Unit]  実在 0 件の形に括弧税
pair: (String, Int)?        // = Option[(String, Int)]  タプルは括弧済みアトム
nested: (Int?)?             // = Option[Option[Int]]    `Int??` は ?? にレクスされるため不可
fn parse_opt(s: String) -> Int?!   // = Result[Option[Int], String]  ? が先、! は戻りマーカー
```

**D3(Q3 ○): fmt は `Option[T]` → `T?` へ正規化する(全型位置)。**
正準形は短形。混在は LLM writability(コーパスの一貫性 = 生成の一貫性)を直撃
するため、v0.53.5 の one-name-one-meaning 路線で 1 形に潰す。入れ子・fn 型・
レコード型の inner は再パース可能になるよう括弧を付けて出力する。

**D4(スコープ外の明示)**: `Result[T, String] → T!` の fmt 正規化は本 ADR に
**含めない**。`!` 付き署名は本体の解釈(ok 自動包み・`!` 伝搬)と連動するため、
綴りのみの置換であることを A/B で証明してから別途 1 問で裁定する。

## Rationale

- 短形が実在分布の全域(戻り 387 / 引数 331 / ジェネリック 147)をカバーし、
  括弧税は実在 0 件の形にだけかかる。
- `!`(ADR-0009: 可謬性は矢印の戻りに属す)と `?` で結合の向きが揃う。
- 実装は ADR-0002 Phase 1a の機構の完全な相似形: pseudo-generic
  `Generic { name: "?" }` → resolver で `Ty::option(T)`、fmt が表面綴りを再印字。
  下流(checker/IR/codegen/mir)は Option しか見ない — クロスターゲット挙動変更ゼロ。

## Alternatives — 検討して却下した案

1. **戻り位置限定で開始**(`T!` と揃える): 位置依存規則の新設。実測 478 件が
   対象外になり、「`->` の後では書けるが `:` の後では書けない」を教えることになる。**却下**。
2. **`?` が fn 型全体に付く**(`(A) -> B?` = Option[fn]): 実在 0 件の形を無括弧に
   して実在 52 件に括弧税をかける逆配分。**却下**。
3. **fmt 正規化なし**(両綴り併存): 移行 diff ゼロだが混在が永続し、モデルが読む
   コーパスが2形に割れる。**却下**。
4. **`T??` の特別レクス**(型文脈で `??` を分割): lexer に文脈依存を持ち込む。
   入れ子 Option は実在 45 件で、括弧 `(T?)?` で足りる。**却下**。

## Consequences

**得るもの**: Option 注釈 1044 箇所(224 ファイル)が機械的に縮む。`?`(不在)/
`!`(可謬)/`?.`/`??` の記号族が型と式で同じ意味軸に揃う。

**払うもの**:
- 文法表面の追加(型アトム後置 `?`)とその教育コスト。
- 一回きりの正規化 diff(spec/ + examples/ = fmt gate の対象範囲)。
- **stdlib も移行済み(2026-08-07 追記)**: `almide fmt --no-import-edit` が
  据え置きの前提だった splice-context ハザード(import 自動挿入)を解いたため、
  stdlib 258 ファイルを batch 正規化(import 行の増減ゼロを機械確認)。
  旧・据え置き根拠だった「長綴りの境界証人」役は spec の単一化テスト
  (fallible_marker / option_marker)へ引き継ぎ。

## Falsifier

- fn 型 slot・入れ子で `?` の結合を読み違える実測エラーが Dojo の MSR 計測で
  有意に出たら、D2 の括弧規則を再裁定する。
- 正規化後に `Option[` 綴りへの回帰(手書き)が支配的なら、D3 の方向を再考する。

## References

- 裁定ログ: 2026-08-06 セッション(Q1/Q2/Q3 いずれも ○)
- 実測: 本文の表(rg によるコーパス計数、2026-08-06)
