<!-- description: Fix fragile compiler internals: visitor pattern, ExprId duplication, UF isolation, Ty clone cost -->
# Compiler Fragility Hotspots

**Goal**: コンパイラ内部の脆弱なポイントを構造的に解消する
**Priority**: 1 が最もクリティカル（現在の作業のブロッカー）

---

## 1. resolve walk が hand-written で fragile 🔴

`resolve_expr_types` が `ExprKind` の 47 variant を手動列挙して再帰している。`infer_expr_inner` と完全に同じパスを通る保証がない。variant 追加時に片方だけ更新して壊れるリスクが常にある。

**現状の問題**: 今まさにこれで詰まっている。

**理想**: AST に generic visitor (`walk_expr_mut`) を 1 つ定義し、infer も resolve もそれを使う。`ExprKind` に variant を追加したとき 1 箇所直せば全 walk が追従する。

**Effort**: L

---

## 2. fn_defaults の ExprId 重複

`lower_program_with_prefix` の冒頭で `p.default.clone()` してデフォルト引数を収集している。型チェック済み AST から取るので `ty` は含まれるが、`ExprId` が元の式と重複する。

**リスク**: 将来 `ExprId` ベースの lookup（source map, debug info 等）を入れると壊れる。

**対策**: clone 時に ExprId を振り直すか、デフォルト引数を別の表現（テンプレート式）で保持する。

**Effort**: S

---

## 3. check_module_bodies の snapshot/restore が constraints + uf だけ

`infer_expr` が `expr.ty` に直接書く設計に変えたことで、inference state は AST に閉じ込められた。しかし `constraints` と `uf` は共有リソースとして save/restore している。

**リスク**: モジュール間で同じ `TypeVar` 番号が発生すると、resolve walk で他モジュールの UF を使って resolve してしまう。現在は各モジュールごとに fresh UF なので問題ないが、前提が暗黙的。

**対策**: `TypeVar` に module scope を持たせるか、UF のスコープ分離を型レベルで保証する。

**Effort**: M

---

## 4. Ty の clone コスト

`expr.ty.clone().unwrap_or(Ty::Unknown)` が lowering 中に全式で呼ばれる。`Ty::Record { fields: Vec<(Sym, Ty)> }` のような複合型は深いクローンになる。

**対策**: `Rc<Ty>` か arena allocation を検討。ただし compiler-architecture-10.md で `Rc<Ty>` は cost-benefit ratio が悪いと判断済み — プロファイリングで実際にボトルネックになった時点で再検討。

**Effort**: M-L

---

## 5. Serde の flatten + internally tagged enum

`Expr` の `#[serde(flatten)]` は `--emit-ast` で動くが、serde の `flatten` + internally tagged enum の組み合わせは known perf issue。

**リスク**: 大きな AST の JSON シリアライズが遅い場合はここがボトルネック。

**対策**: パフォーマンスが問題になったら `flatten` を外して手動シリアライズに切り替える。現時点では低優先度。

**Effort**: S

---

## Priority Order

| # | Item | Impact | Urgency | Effort |
|---|------|--------|---------|--------|
| 1 | Generic visitor pattern | 全 walk の正確性保証 | 🔴 今ブロック中 | L |
| 2 | ExprId 重複 | 将来の lookup 基盤 | 🟡 予防的 | S |
| 3 | UF スコープ分離 | モジュール間の型安全性 | 🟡 暗黙的前提 | M |
| 4 | Ty clone コスト | パフォーマンス | 🟢 計測してから | M-L |
| 5 | Serde flatten perf | --emit-ast 速度 | 🟢 計測してから | S |
