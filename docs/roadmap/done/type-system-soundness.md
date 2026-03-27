<!-- description: Type system soundness fixes (Unknown propagation, unification, occurs) -->
# Type System Soundness

## Summary
型システムの健全性を B+ → A+ に引き上げる。Critical 3 + High 4 + Medium 3 + P1 4 = 14 修正完了。

## Goal
- Unknown の伝播を最小化
- unification が全型コンストラクタを正しく処理
- occurs check が無限型を完全に防止
- TypeVar が解決不能時に情報を失わない

## Critical ✅ / High ✅ / Medium ✅

C-1〜C-3, H-1〜H-4, M-1〜M-3: 全10項目修正済み。

## P1: Unknown 伝播の修正 ✅

### P1-a: `unwrap_or(Ty::Unknown)` の段階的エラー化 ✅
- 17箇所を分類: 意図的 wildcard (12) / ICE (3) / 推論失敗 (2)
- lower.rs `expr_ty()`, `resolve_type_expr` に ICE ログ追加

### P1-b: Result の Unknown 半分 ✅
- `expressions.rs`: ok/err で expected → current_ret → Unknown の3段フォールバック
- `infer.rs`: ok/err のデフォルトを fresh_var() に変更

### P1-c: ラムダ引数の TypeVar → Unknown 降格 ✅
- **根本原因**: `check_named_call` が stdlib 呼び出しで InferTy constraint を生成せず、lambda の fresh_var が永遠に解決されなかった
- **修正1** (calls.rs): `check_named_call` で引数の InferTy に対して constraint を追加
- **修正2** (types.rs): `InferTy::to_ty` の出力に含まれる `Ty::TypeVar("?N")` を `resolve_inference_vars` で事後解決。`seen` set で循環検出
- **修正3** (mod.rs): `check_program` / `check_module_bodies` で `resolve_inference_vars` を適用
- **修正4** (lower.rs): lambda param の型を checker 推論結果 (`Fn` 型の params) から取得

### P1-d: パターンマッチの Unknown 伝播 ✅
- match subject が Unknown のとき warning 出力

## Files
```
src/check/types.rs        — resolve_inference_vars (post-solve TypeVar resolution)
src/check/calls.rs        — constraint propagation for stdlib calls
src/check/mod.rs          — apply resolve_inference_vars in check_program
src/check/expressions.rs  — ok/err bidirectional, match Unknown warning
src/check/infer.rs        — ok/err fresh_var
src/lower.rs              — lambda param type from checker, ICE logging
```
