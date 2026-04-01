<!-- description: Environment-variable-controlled IR dump for each nanopass stage -->
<!-- done: 2026-04-01 -->
# Nanopass Debug Dump

環境変数で各 nanopass の中間 IR をダンプし、コンパイラのデバッグと最適化の検証を容易にする。

## 参考

- **Roc**: `load_internal/file.rs` — デバッグフラグで各パス後の IR を出力
  ```rust
  ROC_PRINT_IR_AFTER_SPECIALIZATION
  ROC_PRINT_IR_AFTER_DROP_SPECIALIZATION
  ROC_PRINT_IR_AFTER_REFCOUNT
  ROC_PRINT_IR_AFTER_RESET_REUSE
  ROC_PRINT_IR_AFTER_TRMC
  ROC_CHECK_MONO_IR  // IR の整合性チェック
  ```
- **Roc**: `assert_sizeof_all!` マクロで構造体サイズの退行を検出

## 設計案

```bash
# 特定パスの後の IR をダンプ
ALMIDE_DUMP_IR=capture_clone,clone_insertion almide build app.almd --target wasm

# 全パスをダンプ
ALMIDE_DUMP_IR=all almide build app.almd

# IR の整合性チェック（TypeVar が残っていないか等）
ALMIDE_CHECK_IR=1 almide build app.almd
```

## ゴール

- 各 nanopass の before/after で IR をファイルまたは stderr に出力
- パス名でフィルタリング可能
- IR の整合性チェック（TypeVar 残存、未解決参照、型不一致の検出）
- コンパイラ開発者が最適化パスの効果を視覚的に確認できる
