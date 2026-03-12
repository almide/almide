# Error Recovery [ACTIVE] [P0]

## Why This Is Critical

LLMはコードを1箇所ずつ直すのではなく、**全エラーを一括で見て一発で直す**のが最も効率的。現状の「最初の1エラーで止まる」挙動は、LLMとの対話ループを不必要に増やしている。

エラーリカバリはAlmideの「modification survival rate」ミッションの中核要素。

## Current State

| Component | Recovery | Error Format |
|-----------|----------|-------------|
| Parser (declarations) | `skip_to_next_decl()` — 次の `fn`/`type`/`test` キーワードまでスキップ | Plain string with line:col |
| Parser (statements) | なし — エラーが宣言レベルまでバブルアップ | Plain string |
| Parser (expressions) | なし — 最初のエラーで式全体を中断 | Plain string |
| Type checker | 全宣言を処理、全 `Diagnostic` を収集 | Structured `Diagnostic` with source, hints, secondary spans |

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
Function → sync on statement boundaries (newline + indent level)   [Phase 2]
Statement → sync on expression terminators (, ) ] } newline)       [Phase 3]
Expression → produce ErrorExpr node, continue parsing              [Phase 4]
```

## Phases

### Phase 1: Structured Parser Errors [P0]

パーサーエラーを `String` から `Diagnostic` に変換。チェッカーと同じ形式で出力。

- [ ] `parser/` のエラー型を `Diagnostic` に統一
- [ ] ソースライン表示、キャレット（`^^^`）、セカンダリスパン
- [ ] パーサーエラーにもヒント追加（例: `expected ')' to close function call started at line 5`）
- [ ] テスト: パーサーエラーのスナップショットテスト追加

### Phase 2: Statement-Level Recovery [P0]

ブロック内でパースエラーが起きたとき、次の文の境界まで同期して残りの文をパースし続ける。

- [ ] `parse_block()` にステートメント回復ロジック追加
- [ ] 回復ポイント: 同じインデントレベルの改行、`let`/`var`/`if`/`match`/`for`/`do`/`guard`/`return` キーワード
- [ ] エラー後の文は `Stmt::Error` としてASTに記録
- [ ] テスト: 1関数内に複数パースエラー → 全報告

### Phase 3: Error AST Nodes [P1]

```rust
// ast.rs に追加
Expr::Error { span: Span }
Stmt::Error { span: Span }
```

- [ ] `Expr::Error` / `Stmt::Error` をASTに追加
- [ ] パーサーがエラー時にこれらのノードを生成
- [ ] IR lowering: `Expr::Error` → `IrExpr::Error`（型は `Ty::Unknown`）
- [ ] チェッカー: `Error` ノードを無視（cascading error 抑制）
- [ ] コードジェン: `Error` ノードが残っていたらemitしない（エラー時はcodegen到達しないはずだが安全弁）
- [ ] テスト: 部分ASTから型チェック → cascading errorなし

### Phase 4: Expression-Level Recovery [P1]

式の途中でエラーが起きたとき、安全な回復ポイントまでスキップして `Expr::Error` を返す。

- [ ] 回復ポイント: 閉じデリミタ（`)` `]` `}`）、カンマ、改行
- [ ] バランシング: 開きデリミタと閉じデリミタの対応を追跡
- [ ] 不完全な式（`1 +`）→ `Expr::Error` + 次の文を正常パース
- [ ] テスト: 不完全な式の後に正常なコード → 両方パース

### Phase 5: Common Typo Detection [P2]

よくある間違いを検出して具体的な修正案を提示。

- [ ] Near-miss keywords: `funcion` → `did you mean 'fn'?`
- [ ] Missing delimiters: `if cond { ... ` → `missing '}' to close block started at line 5`
- [ ] Wrong operators: `if x = 5` → `did you mean '=='?`
- [ ] Missing comma in records: `{ a: 1 b: 2 }` → `expected ',' between fields`
- [ ] 未閉じ文字列リテラル: `"hello` → `unterminated string literal`

### Phase 6: Checker Continuation on Partial AST [P2]

チェッカーがエラーノードを含むASTを安全に処理できるようにする。

- [ ] `Ty::Unknown` が他の型と互換 → cascading error 抑制
- [ ] エラーノードを含む関数の戻り値型 → `Unknown`（型推論に影響しない）
- [ ] 部分的に正しいコードの型エラーは引き続き報告
- [ ] テスト: パースエラー + 型エラーの混在 → 両方報告、cascading なし

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
| **Almide (current)** | Declaration-level only | `skip_to_next_decl()` |
| **Almide (target)** | Yes, 全フェーズ | Statement + expression level, error AST nodes |
