# Type System Soundness [ACTIVE]

## Summary
型システムの健全性を B+ → A+ に引き上げる。Critical 3 + High 4 + Medium 3 = 10 修正完了。残り P1 項目あり。

## Goal
- Unknown の伝播を最小化
- unification が全型コンストラクタを正しく処理
- occurs check が無限型を完全に防止
- TypeVar が解決不能時に情報を失わない

## Critical (型システムが壊れる) ✅

### C-1: DoBlock が常に Unknown を返す ✅
- `src/check/expressions.rs` — `_ty` をそのまま返す。guard 付き do block は `Ty::Unit`

### C-2: Record 同士の unify_infer がない ✅
- `src/check/mod.rs` — Record のフィールド同士を再帰的に unify するケース追加

### C-3: Unknown が全てと unify 成功する ✅
- `src/types.rs` — Unknown 同士 → true。片方 Unknown → binding を試行して情報伝播

## High (型推論の精度に直結) ✅

### H-1: occurs check が浅い (Tuple, Record, Fn 未チェック) ✅
- `src/types.rs` — Tuple, Record, OpenRecord, Fn の再帰チェック追加

### H-2: 未解決 TypeVar が Unknown に落ちる ✅
- `src/types.rs`, `src/check/types.rs` — `Ty::TypeVar(name)` のまま保持

### H-3: TypeVar binding 時に structural bounds 未検証 ✅
- `src/types.rs` — binding 前に bound の compatible チェック

### H-4: Union unification が非決定的 ✅
- `src/types.rs` — binding のスナップショット → 成功時に commit、失敗時に rollback

## Medium ✅

### M-1: Fn type の Result auto-unwrap が ad-hoc ✅
### M-2: effect fn の return type 判定が body 依存 ✅
### M-3: Unknown の refinement が不十分 (Record, Union) ✅

## Remaining P1: Unknown 伝播の根本修正

### P1-a: `unwrap_or(Ty::Unknown)` の段階的エラー化
- **問題**: 型が見つからない場合 `Ty::Unknown` で続行し、以降の型チェックをすり抜ける（15箇所以上）
- **修正**: Unknown の発生源を分類 (意図的ワイルドカード / 推論失敗 / 内部エラー) し、(b)(c) をエラーまたは warning として報告
- codegen が Unknown を含む IR を受け取った場合に ICE を出す

### P1-b: Result の Unknown 半分
- **問題**: `ok(42)` → `Result[Int, Unknown]`、`err("fail")` → `Result[Unknown, String]`
- **修正**: 双方向型推論で呼び出し文脈から期待される Result 型を取得し、Unknown 半分を埋める

### P1-c: ラムダ引数の TypeVar → Unknown 降格
- **問題**: ラムダの引数型推論で TypeVar が Unknown に降格される
- **修正**: TypeVar を保持し、ラムダの body 型チェック中に制約収集して解決

### P1-d: パターンマッチの Unknown 伝播
- **問題**: `match` の subject が Unknown 型のとき、パターン変数が Unknown でバインドされる
- **修正**: subject が Unknown の場合にエラーにするか、制約を収集して遅延推論

## Files
```
src/types.rs          — unify, occurs_in, substitute
src/check/mod.rs      — unify_infer
src/check/expressions.rs — DoBlock, pattern binding
src/check/types.rs    — InferTy::to_ty
```
