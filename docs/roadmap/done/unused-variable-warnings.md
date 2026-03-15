# Unused Variable Warnings [ACTIVE]

## Summary
未使用の変数・インポートに対して warning を出す。`_` プレフィックスで抑制可能。

## Current State
checker v3 は型エラーのみ報告。未使用変数の warning は実装されていない。

## Goal
```
warning: unused variable 'x'
  --> app.almd:3:7
  hint: Prefix with '_' to suppress: _x
```

## Design

### Detection
IR の use-count（`ir.rs` の `compute_use_counts`）を利用。`use_count == 0` かつ `_` プレフィックスなしの変数を警告。

### Scope
- `let` / `var` バインディング
- 関数パラメータは除外（API 互換性のため）
- パターンバインディング（`let (a, b) = ...` の `b` が未使用）
- `_` プレフィックスは抑制

### Implementation
- `src/ir.rs` — `collect_unused_var_warnings()` を復活（以前存在した）
- `src/main.rs` / `src/cli.rs` — warning 出力を追加
- テスト: 未使用変数で warning が出ることを検証

## Files
```
src/ir.rs
src/main.rs
src/cli.rs
```
