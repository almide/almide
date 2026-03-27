<!-- description: Ground-up rewrite of the 890-line source formatter -->
<!-- done: 2026-03-15 -->
# Formatter Rewrite

## Summary
`src/fmt.rs` (890行) を 0 から書き直し。旧コードは lambda 構文更新済みだが、全体設計が古い。

## Current State
- 890 行の単一ファイル
- 旧コードのまま（checker/lower/codegen は全て書き直し済み）
- 動作するが、新機能（paren lambda, structural bounds）のフォーマットが不完全

## Goal
- < 500 行
- AST → String の純粋な変換
- 新構文対応: `(x) => expr`, `[T: { .. }]`, union types

## Design
- `src/fmt.rs` — 書き直し
- 各 AST ノード → フォーマット文字列の pure function
- インデント管理のシンプルな depth パラメータ

## Files
```
src/fmt.rs
```
