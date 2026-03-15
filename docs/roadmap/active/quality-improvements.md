# C. 品質向上

## 1. エラーメッセージの行番号

**状態:** checker のエラーに span 情報がない場合がある（`in method call` としか出ない）
**原因:** `check_call` 等で `Diagnostic` を生成する時に line/col を含めてない
**修正:** `Diagnostic::error()` 生成時に `expr.span()` から line/col を取得して設定
**見積り:** 半日（全エラーサイトを走査して span を追加）

## 2. Heredoc の行番号追跡

**状態:** `lex_heredoc` 内で `\n` を消費するが lexer の `line` カウンタを更新しない
**原因:** `lex_heredoc` は `(Token, usize)` を返すが `line` は `lex_string` のスコープ内の変数で、heredoc 内の改行を反映しない
**修正:** `lex_heredoc` が消費した改行数を返すか、lexer の main loop で `pos` の変化から行番号を再計算
**見積り:** 1-2時間

## 3. LSP (Language Server Protocol)

**状態:** なし
**修正方針:** `tower-lsp` crate で最小限の LSP を実装
- hover: 式の型を表示
- diagnostics: checker のエラーをリアルタイム表示
- go-to-definition: import 先のファイルへジャンプ
**見積り:** 1-2週間
