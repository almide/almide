<!-- description: Continue parsing after syntax errors to report multiple diagnostics -->
# Parser Error Recovery

## Summary
1 つのシンタックスエラーでパースが停止する現状を改善。複数エラーを報告し、エラー後もパースを継続する。

## Current Problem
```
error: Expected expression at line 5
  --> app.almd:5:10
```
→ 1 個のエラーで停止。後続の正しいコードもチェックされない。

## Goal
```
error: Expected expression at line 5
error: undefined variable 'x' at line 8
error: type mismatch at line 12

3 error(s) found
```

## Design

### Statement-level recovery
パース失敗時に次のステートメント境界（`;`, newline + keyword, `}`) まで読み飛ばして継続。

### Declaration-level recovery
関数/型宣言のパース失敗時に次の `fn`/`type`/`test` まで読み飛ばして継続。

### Error node
パース失敗した箇所に `Expr::Error` / `Stmt::Error` を挿入。checker はこれらをスキップ。

## Implementation
- `src/parser/mod.rs` — `recover_to_sync_point()` メソッド追加
- `src/parser/declarations.rs` — 宣言レベル recovery
- `src/parser/statements.rs` — 文レベル recovery
- テスト: 複数エラーが報告されることを検証

## Files
```
src/parser/mod.rs
src/parser/declarations.rs
src/parser/statements.rs
spec/lang/core_test.almd (error recovery tests)
```
