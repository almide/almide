<!-- description: Decouple hint generation from parser into a dedicated system -->
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

## Completed Phases

### Phase 3: テスト基盤 — DONE (v0.5.12)

テーブル駆動テスト 43件 → 全5モジュールをカバー。正常系・異常系・スコープ検証。

### Phase 4: 拡張 — DONE

- ✅ `HintContext` に `next: Option<&Token>` を追加
- ✅ `|x|` closure ヒントを `primary.rs` インラインから `operator.rs` に移行（lookahead使用）
- ✅ セミコロンヒント追加（`operator.rs`）
- ✅ LLMエラーパターン11件追加（`syntax_guide.rs`）: `self`/`this`, `new`, `void`, `undefined`, `switch`, `elif`/`elsif`/`elseif`, `extends`/`implements`, `lambda`
- ✅ ヒントカタログ (`catalog.rs`) — 全ヒント一覧を `all_hints()` で取得可能
- ✅ テスト 61件（+18件追加）

## Status

**All phases complete.** This roadmap item can be moved to Done.

## Priority

This item is complete. Consider moving to `done/`.

## Reference

| Language | Hint system |
|----------|-------------|
| **Rust (rustc)** | `rustc_errors` crate, `Diagnostic` + `Subdiagnostic` derive macros, lint registry |
| **Swift** | `DiagnosticEngine` + `DiagnosticVerifier`, diagnostic IDs for each hint |
| **Elm** | 各エラーが独立モジュール、`Error.xxx.toReport()` パターン |
| **TypeScript** | `Diagnostics.generated.ts` — コード生成でエラーカタログ管理 |
