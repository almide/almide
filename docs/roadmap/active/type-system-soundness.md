# Type System Soundness [ACTIVE]

## Summary
型システムの健全性を B+ → A+ に引き上げる。Critical 3 + High 4 + Medium 3 + P1 3 = 13 修正完了。残り P1-c のみ。

## Goal
- Unknown の伝播を最小化
- unification が全型コンストラクタを正しく処理
- occurs check が無限型を完全に防止
- TypeVar が解決不能時に情報を失わない

## Critical (型システムが壊れる) ✅

### C-1: DoBlock が常に Unknown を返す ✅
### C-2: Record 同士の unify_infer がない ✅
### C-3: Unknown が全てと unify 成功する ✅

## High (型推論の精度に直結) ✅

### H-1: occurs check が浅い (Tuple, Record, Fn 未チェック) ✅
### H-2: 未解決 TypeVar が Unknown に落ちる ✅
### H-3: TypeVar binding 時に structural bounds 未検証 ✅
### H-4: Union unification が非決定的 ✅

## Medium ✅

### M-1: Fn type の Result auto-unwrap が ad-hoc ✅
### M-2: effect fn の return type 判定が body 依存 ✅
### M-3: Unknown の refinement が不十分 (Record, Union) ✅

## P1: Unknown 伝播の修正

### P1-a: `unwrap_or(Ty::Unknown)` の段階的エラー化 ✅
- 17箇所を分類: 意図的 wildcard (12) / 推論失敗 (2) / ICE (3)
- lower.rs `expr_ty()` に ICE ログ追加（checker が型を付けなかった式を検出）
- lower.rs `resolve_type_expr` の `List[]`/`Option[]` に ICE ログ追加
- check/mod.rs の `List[]`/`Option[]` は `resolve_type_expr` が `&self` のため warning 追加は保留（`&mut self` 化の影響範囲が大きい）

### P1-b: Result の Unknown 半分 ✅
- `expressions.rs`: `ok()`/`err()` で expected → current_ret → Unknown の3段フォールバック
- `infer.rs`: `ok()`/`err()` のデフォルトを `Ty::String`/`Ty::Unit` から `fresh_var()` に変更（制約ソルバーが正しい型を推論）

### P1-d: パターンマッチの Unknown 伝播 ✅
- match subject が `Ty::Unknown` のとき warning を出力
- パターン変数への Unknown 伝播自体は設計上正しい（error recovery）

### P1-c: ラムダ引数の TypeVar → Unknown 降格 [REMAINING]
- **問題**: ラムダの引数型推論に2つの独立パスがある
  - `infer.rs` (Pass 1): `fresh_var()` で TypeVar を生成 — 制約ベース推論に参加
  - `expressions.rs` (Pass 2): expected type or `Ty::Unknown` — 文脈駆動チェック
- **根本原因**: `check_decl` は `infer_expr` (Pass 1) のみを呼ぶ。`check_expr_with` (Pass 2) は呼ばれない。そのため `expressions.rs` の lambda チェックパスは関数宣言の body からは到達しない
- **影響**: lambda が `list.map((x) => x + 1)` のように stdlib 呼び出しの引数として使われる場合、`infer.rs` が fresh_var を生成し、stdlib の型シグネチャから制約が伝播して正しく推論される。問題が出るのは expected type が伝播しないネストしたケースのみ
- **修正案**: `check_decl` で `infer_expr` と `check_expr_with` の両方を走らせるか、`infer.rs` の lambda パスに expected type 伝播を追加する（後者がリスク低い）
- **リスク**: checker アーキテクチャ（2パス構成）に触るため、意図しない型チェック変更が広範に影響する可能性

## Files
```
src/types.rs          — unify, occurs_in, substitute
src/check/mod.rs      — unify_infer
src/check/expressions.rs — ok/err bidirectional, match Unknown warning
src/check/infer.rs    — ok/err fresh_var
src/lower.rs          — ICE logging for missing types
```
