# Diagnostic end_col — エラー波線の精度向上

**優先度:** 低〜中 — 診断品質の仕上げ。セカンダリスパン活性化で主要な改善は完了済み
**前提:** セカンダリスパン活性化済み（E006/E005/E009 で宣言地点表示）

---

## 現状

- `Diagnostic` 構造体に `end_col: Option<usize>` フィールドは存在
- 全箇所で `None` のまま — populate されていない
- 結果: エラー箇所のハイライトが `^^^` ではなく `^` 1文字のみ

## 変更が必要な箇所

### AST 層
- `src/ast.rs` の `Span` 構造体に `end_col: usize` を追加
- lexer がトークンの終了列を記録する必要あり

### Parser 層
- `src/lexer.rs` でトークン生成時に end_col を計算

### Type Checker 層
- `src/check/mod.rs` の `emit()` で `span.end_col` → `diag.end_col` に設定

## 影響範囲

- **AST 全体に波及** — Span を使う全ての箇所が影響を受ける
- コンパイラの多くの部分で Span を構築しているため、変更量が大きい
- 機能的には破壊的変更なし（end_col は Optional）

## 推奨

別 PR で実施。セカンダリスパンの活性化で「複数地点の同時表示」は既に動いているため、緊急度は低い。
