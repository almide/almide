# Hint System Architecture [ACTIVE] [P0]

## Why This Is Critical

Almideの差別化は「LLMが全エラーを見て一発で直せる」こと。そのためにはエラーメッセージが**原因**を指す必要がある。現状、親切処理（ヒント、typo検出、missing comma等）がパーサー本体に直接埋め込まれており：

1. **追加コストが高い** — 新しいヒントを追加するたびにパーサーの複数箇所を修正
2. **テストが分散** — ヒントのテストがパーサーテストに混在
3. **見通しが悪い** — どんなヒントが存在するか一覧できない
4. **パーサーが肥大化** — パース本来のロジックとヒント生成が混在

LLM向け言語として、ヒントの品質と量は競争優位の源泉。追加を簡単にする仕組みが必要。

## Current State — Phase 1 & 2 DONE

### Implemented Architecture

```
src/parser/
├── hints/
│   ├── mod.rs              # HintContext, HintScope, HintResult, check_hint() dispatcher
│   ├── missing_comma.rs    # リスト/マップ/引数/パラメータのカンマ抜け
│   ├── keyword_typo.rs     # function→fn, class→type, struct→type, enum→type, etc.
│   ├── delimiter.rs        # 括弧閉じ忘れ、= 抜け
│   ├── operator.rs         # = vs ==, || vs or, && vs and, ! vs not, -> vs =
│   └── syntax_guide.rs     # return不要, null→none, let mut→var, throw→Result, etc.
```

### Migrated Call Sites

| 元の場所 | 移行先モジュール | 状態 |
|----------|-----------------|------|
| `helpers.rs` `hint_for_expected()` | operator.rs, delimiter.rs | ✅ DONE — delegates to `check_hint()` |
| `declarations.rs` `parse_top_decl()` | keyword_typo.rs | ✅ DONE |
| `primary.rs` `parse_primary()` (Bang, PipePipe, AmpAmp) | operator.rs | ✅ DONE |
| `primary.rs` `parse_primary()` (rejected idents) | syntax_guide.rs | ✅ DONE |
| `primary.rs` `parse_primary()` (final fallback) | syntax_guide.rs | ✅ DONE |
| `expressions.rs` `parse_or()` (PipePipe) | operator.rs | ✅ DONE |
| `expressions.rs` `parse_and()` (AmpAmp) | operator.rs | ✅ DONE |
| `statements.rs` `parse_let_stmt()` (let mut) | syntax_guide.rs | ✅ DONE |
| `compounds.rs` `parse_list_expr()` (missing comma) | missing_comma.rs | ✅ DONE |
| `compounds.rs` map literal (missing comma) | missing_comma.rs | ✅ DONE |
| `expressions.rs` `parse_call_args()` (missing comma) | missing_comma.rs | ✅ DONE |

### Remaining Inline (kept intentionally)

| 場所 | 理由 |
|------|------|
| `primary.rs` `\|x\|` closure syntax | lookahead が必要（HintContext に next token がない） |
| `helpers.rs` `expect_closing()` | セカンダリスパン生成はヒントと別のメカニズム |
| `declarations.rs` import `{` detection | パース構造依存のチェック |

## Remaining Work

### Phase 3: ヒントのテスト基盤

各ヒントモジュールに対応するテーブル駆動テスト:

```rust
#[test]
fn missing_comma_in_list() {
    assert_hint(
        "[1 2 3]",
        HintScope::ListLiteral,
        "Missing ',' between list elements",
    );
}
```

### Phase 4: 拡張

- HintContext に `next: Option<&Token>` を追加して `|x|` closure チェックも移行可能に
- ヒントカタログ（どんなヒントがあるか一覧）の自動生成
- LLMエラーパターン分析に基づく新しいヒントモジュール追加

## Priority

**P0** — Phase 1-2 完了。Phase 3 (テスト基盤) は次のプライオリティ。

## Reference

| Language | Hint system |
|----------|-------------|
| **Rust (rustc)** | `rustc_errors` crate, `Diagnostic` + `Subdiagnostic` derive macros, lint registry |
| **Swift** | `DiagnosticEngine` + `DiagnosticVerifier`, diagnostic IDs for each hint |
| **Elm** | 各エラーが独立モジュール、`Error.xxx.toReport()` パターン |
| **TypeScript** | `Diagnostics.generated.ts` — コード生成でエラーカタログ管理 |
