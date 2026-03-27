<!-- description: Quality improvements (error line numbers, heredoc tracking) -->
# Quality Improvements

## 1. エラーメッセージの行番号 ✅

`emit()` メソッドで全 checker diagnostic に自動 span 付与。22 箇所を修正。

## 2. Heredoc の行番号追跡

**状態:** `lex_heredoc` 内で `\n` を消費するが lexer の `line` カウンタを更新しない
**修正:** `lex_heredoc` が消費した改行数を返すか、lexer の main loop で `pos` の変化から行番号を再計算
**見積り:** 1-2時間
