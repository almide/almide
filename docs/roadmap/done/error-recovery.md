<!-- description: Report all errors at once instead of stopping at the first one -->
# Error Recovery [DONE]

## Why This Is Critical

LLMはコードを1箇所ずつ直すのではなく、**全エラーを一括で見て一発で直す**のが最も効率的。現状の「最初の1エラーで止まる」挙動は、LLMとの対話ループを不必要に増やしている。

エラーリカバリはAlmideの「modification survival rate」ミッションの中核要素。

## Current State (v0.5.10)

| Component | Recovery | Error Format |
|-----------|----------|-------------|
| Parser (declarations) | `skip_to_next_decl()` — 次の `fn`/`type`/`test` キーワードまでスキップ | Structured `Diagnostic` |
| Parser (statements) | `skip_to_next_stmt()` — 次の文境界まで同期、`Stmt::Error` 挿入 | Structured `Diagnostic` |
| Parser (expressions) | エラー時に `Stmt::Error` を挿入して続行 | Structured `Diagnostic` |
| Type checker | 全宣言を処理、`Expr::Error` → `Ty::Unknown` で cascading 抑制 | Structured `Diagnostic` |

### 問題の具体例

```almd
fn foo() -> Int = {
  let x = 1 +          // ← ここでパースエラー
  let y = x * 2        // ← これ以降の全文が失われる
  let z = y + "hello"  // ← 型エラーも報告されない
  z
}

fn bar() -> String = {  // ← ここは別の宣言なのでパースされる
  42                    // ← この型エラーは報告される
}
```

**現状**: エラー1件（`let x` のパースエラー）のみ報告。`foo` の残りと `z` の型エラーは消失。
**目標**: パースエラー1件 + 型エラー2件、計3件を一括報告。

## Design Principles

1. **Sync Points at Every Scope Level** — 各スコープレベルに回復ポイントを設ける
2. **No Cascading Errors** — エラーノードから派生する二次エラーは抑制
3. **Partial AST is Better Than No AST** — パース失敗しても部分ASTを構築してチェッカーに渡す
4. **Structured Diagnostics Everywhere** — パーサーエラーもチェッカーと同じ `Diagnostic` 形式で出力

```
Program  → sync on declaration keywords (fn, type, test, impl)     [DONE]
Function → sync on statement boundaries (newline + keyword)        [DONE]
Statement → sync on expression terminators (, ) ] } newline)       [DONE]
Expression → produce ErrorExpr node, continue parsing              [DONE]
```

## Phases

### Phase 1: Structured Parser Errors [DONE]

パーサーエラーを `String` から `Diagnostic` に変換。チェッカーと同じ形式で出力。

- [x] `Parser.errors` を `Vec<String>` → `Vec<Diagnostic>` に変更
- [x] `Parser.with_file()` でファイル名をセット、`diag_error()` で位置情報付き `Diagnostic` 生成
- [x] ソースライン表示、キャレット（`^^^`）、`display_with_source()` で統一出力
- [x] `parse_file()` が `(Program, String)` を返し、ソース再読み込みを排除
- [x] パーサーエラーにもヒント追加（例: `expected ')' to close function call started at line 5`）
  - `expect_closing()` メソッド: 開き括弧の位置をセカンダリスパンとして表示
  - 全デリミタ（`()`, `[]`, `{}`）の主要呼び出し箇所に適用
- [x] テスト: パーサーエラーのスナップショットテスト追加（12テスト）

### Phase 2: Statement-Level Recovery [DONE]

ブロック内でパースエラーが起きたとき、次の文の境界まで同期して残りの文をパースし続ける。

- [x] `parse_brace_expr()` にステートメント回復ロジック追加
- [x] `skip_to_next_stmt()`: 改行後の `let`/`var`/`if`/`match`/`for`/`while`/`do`/`guard` で同期
- [x] 1関数内に複数パースエラー → 全報告
- [x] エラー後の文は `Stmt::Error` としてASTに記録

### Phase 3: Error AST Nodes [DONE]

- [x] `Expr::Error { span }` / `Stmt::Error { span }` をASTに追加
- [x] パーサーがエラー時に `Stmt::Error` ノードを生成（ブロック内リカバリ）
- [x] IR lowering: `Expr::Error` → `IrExprKind::Unit` (型は `Ty::Unknown`)
- [x] IR lowering: `Stmt::Error` → `IrStmtKind::Comment { "/* error */" }`
- [x] チェッカー: `Expr::Error` → `Ty::Unknown`（cascading error 抑制）
- [x] チェッカー: `Stmt::Error` → skip（cascading error 抑制）
- [x] フォーマッタ: `Expr::Error` → `/* error */`、`Stmt::Error` → skip

### Phase 4: Statement-Level Expression Recovery [DONE]

ブロック内でステートメントのパースが失敗したとき、エラーを収集し `Stmt::Error` を挿入して次のステートメントまでスキップ。

- [x] `parse_brace_expr` でエラー時に `Stmt::Error` を挿入して続行
- [x] `skip_to_next_stmt()` で次のステートメント境界まで同期
- [x] 部分ASTがチェッカーに渡り、型エラーも同時報告

### Phase 5: Common Typo Detection [DONE]

よくある間違いを検出して具体的な修正案を提示。

- [x] Near-miss keywords: `function`/`func`/`def`/`fun` → `fn` のヒント
- [x] Wrong type syntax: `struct`/`class`/`enum`/`data` → `type` のヒント
- [x] Wrong operators: `if x = 5` → `Did you mean '=='?`
- [x] Missing delimiters: `)`/`]`/`}` のヒント
- [x] Missing `=` before value: `let x value` → `Missing '='`
- [x] Arrow confusion: `fn f() = Int` → `Use '->' for return type`
- [x] `let mut` → `Use 'var'` (既存)
- [x] `<>` generics → `Use []` (既存)

### Phase 6: Checker Continuation on Partial AST [DONE]

パースエラーがあっても部分ASTをチェッカーに渡し、パースエラー + 型エラーを一括報告。

- [x] `parse_file` がパースエラーを返しつつ部分ASTも返す
- [x] `compile_with_options` / `cmd_check` でパースエラー + チェッカーエラーを結合して報告
- [x] `Expr::Error` → `Ty::Unknown` で cascading error を抑制
- [x] パースエラーがある場合は IR lowering / codegen をスキップ（安全弁）
- [x] テスト: パースエラー + 型エラーの混在 → 両方報告

## Success Criteria

```bash
# このコードに対して:
fn foo() -> Int = {
  let x = 1 +
  let y = "hello" * 2
  y
}

fn bar() -> String = {
  42
}

# 期待されるエラー出力:
# error[E0001]: unexpected token 'let' — expected expression
#   --> app.almd:2:15
#   |
# 2 |   let x = 1 +
#   |               ^ expected expression after '+'
#
# error[E0002]: cannot apply '*' to String and Int
#   --> app.almd:3:19
#   |
# 3 |   let y = "hello" * 2
#   |                   ^ String does not support arithmetic
#   |
#   = hint: use string.repeat("hello", 2) for repetition
#
# error[E0003]: expected String, found Int
#   --> app.almd:7:3
#   |
# 7 |   42
#   |   ^^ this is Int, but bar() declares return type String

# 3 errors emitted
```

## Reference

| Language | Multi-error | Recovery strategy |
|----------|------------|-------------------|
| **Rust (rustc)** | Yes, 全フェーズ | Statement-level sync, error propagation suppression |
| **Go** | Yes, 最大10件 | Statement-level sync, `BadExpr`/`BadStmt` nodes |
| **Swift** | Yes | Expression-level recovery, fix-it suggestions |
| **TypeScript** | Yes | Token-level recovery, partial AST |
| **Elm** | 1件ずつ（意図的） | 1エラー1修正の哲学 |
| **Almide (v0.5.10)** | Yes, 全フェーズ | Statement + expression level, error AST nodes, partial AST type checking |

## Completion

全6フェーズ + パーサーヒント・スナップショットテスト完了。
