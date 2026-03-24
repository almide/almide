# Remove `do` Block [DONE]

**完了**: 2026-03-24

`do` ブロックを言語から完全に撤廃した。

## 実施内容

### Phase 1: .almd ファイル移行 (66箇所)
- `effect fn ... = do { }` → `= { }` (16箇所)
- `do { guard COND else break }` → `while COND { }` (35箇所)
- `do { guard ... else ok(val)/err(val) }` → `while` + tail expression (15箇所)

### Phase 2: コンパイラ除去
- `Expr::DoBlock` を AST から削除
- `IrExprKind::DoBlock` を IR から削除
- Parser: `do` キーワードを reject + migration hint
- 46ファイル変更、-349行

### Phase 3: ドキュメント + リネーム
- `do_guard_test.almd` → `guard_test.almd`
- `do_block_pure_test.almd` → `while_loop_test.almd`
- `codegen_do_block_test.almd` → `codegen_loop_guard_test.almd`

### バグ修正
- StreamFusion の `inline_single_use_collection_lets` が Guard stmt 内の変数参照を substitute していなかったバグを発見・修正

## 結果

- ループ構文: `for` + `while` の2つに統一
- `guard` 文は `while` / `for` 内で引き続き有効
- `try` ブロックは不導入（`effect fn` の auto-? で十分）
- 159/159 テスト通過
