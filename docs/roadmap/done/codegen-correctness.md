<!-- description: Fix correctness issues in generated code (auto-unwrap, range, guard) -->
# Codegen Correctness Fixes

生成コードの正確性に関わる問題の修正。

## P1 (全7項完了)

1. **auto-`?` の二重ロジック統一** ✅ — `should_auto_unwrap_user/stdlib` に統一
2. **Range 型のハードコード** ✅ — IR の `expr.ty` から要素型を取得
3. **Box パターンデストラクトの未バインド変数** ✅ — 非 Bind パターンに `box` / skip 追加
4. **Guard の break/continue ハンドリング** ✅ — IR ノード種別を検査して適切なコード生成
5. **Do-block + guard の unreachable** ✅ — `loop { ... break; }` で wrap
6. **Module/Method 呼び出しの auto-`?`** ✅ — Named 以外の CallTarget でも effect context + Result 返却時に `?` 挿入
7. **effect fn for-loop の Result ラップ** ✅ — 上記修正 + `in_effect` が `LowerCtx` フィールドで全体伝播

## P2

1. **文字列パターンの borrowed subject** ✅ — String 型 subject に `.as_str()` を自動挿入
2. **パターンデストラクトの clone 最適化** → Clone Reduction Phase 4 に統合（Member access は既に `is_copy` で判定済み）
