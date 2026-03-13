# Hint System Architecture [ACTIVE] [P0]

## Why This Is Critical

Almideの差別化は「LLMが全エラーを見て一発で直せる」こと。そのためにはエラーメッセージが**原因**を指す必要がある。現状、親切処理（ヒント、typo検出、missing comma等）がパーサー本体に直接埋め込まれており：

1. **追加コストが高い** — 新しいヒントを追加するたびにパーサーの複数箇所を修正
2. **テストが分散** — ヒントのテストがパーサーテストに混在
3. **見通しが悪い** — どんなヒントが存在するか一覧できない
4. **パーサーが肥大化** — パース本来のロジックとヒント生成が混在

LLM向け言語として、ヒントの品質と量は競争優位の源泉。追加を簡単にする仕組みが必要。

## Current State

ヒントが埋め込まれている箇所：

| 場所 | 内容 |
|------|------|
| `parser/helpers.rs` `hint_for_expected()` | 閉じ括弧不足、`=` vs `==`、`->` vs `=` |
| `parser/helpers.rs` `expect_closing()` | 開き括弧の位置を示すセカンダリスパン |
| `parser/declarations.rs` `parse_top_decl()` | `function`→`fn`, `class`→`type`, `enum`→`type` 等 |
| `parser/primary.rs` `parse_primary()` | `loop`→`while true`, `return`→不要, `null`→`none` 等 |
| `parser/compounds.rs` `parse_list_expr()` | カンマ抜け検出 |
| `parser/expressions.rs` `parse_call_args()` | 引数間カンマ抜け検出 |
| `parser/statements.rs` `parse_let_stmt()` | `let mut`→`var` |
| `parser/expressions.rs` `parse_or()/parse_and()` | `\|\|`→`or`, `&&`→`and` |

計30箇所以上のインラインヒント。

## Design

### Phase 1: Hint Registry

パーサーのエラー時にヒントチェーンを呼び出す仕組み。

```
src/parser/
├── hints/
│   ├── mod.rs              # HintContext, check_hint() dispatcher
│   ├── missing_comma.rs    # リスト/マップ/引数のカンマ抜け
│   ├── keyword_typo.rs     # function→fn, class→type, struct→type
│   ├── delimiter.rs        # 括弧閉じ忘れ + 開始位置
│   ├── operator.rs         # = vs ==, || vs or, && vs and
│   └── syntax_guide.rs     # return不要, null→none, let mut→var
```

```rust
/// ヒントが判断に使うコンテキスト
struct HintContext<'a> {
    expected: Option<&'a TokenType>,  // 期待されたトークン
    got: &'a Token,                   // 実際のトークン
    prev: Option<&'a Token>,          // 直前のトークン
    scope: HintScope,                 // List, Call, Block, TopLevel, etc.
}

enum HintScope {
    TopLevel,
    FnParams,
    CallArgs,
    ListLiteral,
    MapLiteral,
    Block,
    MatchArms,
    Pattern,
}

/// ヒントの結果
struct HintResult {
    message: String,           // メイン・エラーメッセージの上書き（Optional）
    hint: String,              // ヒントテキスト
    secondary: Option<(usize, usize, String)>,  // セカンダリスパン
}

/// 各ヒントモジュールが実装
fn check(ctx: &HintContext) -> Option<HintResult>;
```

### Phase 2: パーサーからの分離

パーサー本体のヒント生成コードを `hints/` に移動。パーサーは：

```rust
// Before (現状)
if is_expr_start {
    return Err(format!(
        "Missing ',' before this element at line {}:{}...",
        tok.line, tok.col
    ));
}

// After (Phase 2)
let ctx = HintContext { expected: Some(&TokenType::Comma), got: tok, scope: HintScope::ListLiteral, .. };
if let Some(result) = self.check_hints(&ctx) {
    return Err(result.to_error_string(tok));
}
```

### Phase 3: ヒントのテスト基盤

各ヒントモジュールに対応するテストファイル:

```
tests/
└── hints/
    ├── missing_comma_test.rs
    ├── keyword_typo_test.rs
    ├── delimiter_test.rs
    └── operator_test.rs
```

ヒントの入出力をテーブル駆動テストで記述：

```rust
#[test]
fn missing_comma_in_list() {
    assert_hint(
        "[1 2 3]",
        HintScope::ListLiteral,
        "Missing ',' before this element",
    );
}
```

## Priority

**P0** — ヒントの数は今後急速に増える。LLMが書くコードのエラーパターンを分析するたびに新しいヒントが必要になる。パーサー本体に埋め込み続けると保守不能になる。

## Implementation Order

1. `hints/mod.rs` + `HintContext` + `HintScope` 定義
2. 既存の `missing_comma` ロジックを `hints/missing_comma.rs` に移動（PoC）
3. `keyword_typo` を移動
4. 残りを順次移動
5. テスト基盤整備

## Reference

| Language | Hint system |
|----------|-------------|
| **Rust (rustc)** | `rustc_errors` crate, `Diagnostic` + `Subdiagnostic` derive macros, lint registry |
| **Swift** | `DiagnosticEngine` + `DiagnosticVerifier`, diagnostic IDs for each hint |
| **Elm** | 各エラーが独立モジュール、`Error.xxx.toReport()` パターン |
| **TypeScript** | `Diagnostics.generated.ts` — コード生成でエラーカタログ管理 |
