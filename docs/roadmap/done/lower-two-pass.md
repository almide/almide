# Lower 2パス分離

**優先度:** post-1.0
**見積:** ±500行, 大

## 現状

`lower/` が AST→IR 変換と use-count analysis を同時実行。責務が混在。

## 理想

- Pass 1: AST→IR (純粋な構造変換)
- Pass 2: use-count / codegen分析 (UseCountPass)

## タスク

- [ ] lower をAST→IR純粋変換に限定
- [ ] use-count analysis を独立 Nanopass に分離
- [ ] codegen判断ロジックを lower から排除

## 判断

壊れてない。保守性改善だが 1.0 前にリスクを取る理由がない。
