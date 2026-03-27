<!-- description: Decouple hint generation from parser into a dedicated system -->
<!-- done: 2026-03-13 -->
# Hint System Architecture [P0]

## Why This Is Critical

Almide's differentiator is "LLMs can see all errors and fix them in one shot." For this, error messages must point to the **cause**. Currently, helpful processing (hints, typo detection, missing comma, etc.) is directly embedded in the parser core:

1. **High cost to add** — adding a new hint requires modifying the parser in multiple places
2. **Tests are scattered** — hint tests are mixed in with parser tests
3. **Poor visibility** — cannot get an overview of what hints exist
4. **Parser bloat** — parsing logic and hint generation are interleaved

As a language designed for LLMs, hint quality and quantity are a source of competitive advantage. A system that makes adding hints easy is needed.

## Current State — Phase 1 & 2 DONE

### Implemented Architecture

```
src/parser/
├── hints/
│   ├── mod.rs              # HintContext, HintScope, HintResult, check_hint() dispatcher
│   ├── missing_comma.rs    # missing comma in list/map/args/params
│   ├── keyword_typo.rs     # function→fn, class→type, struct→type, enum→type, etc.
│   ├── delimiter.rs        # unclosed brackets, missing =
│   ├── operator.rs         # = vs ==, || vs or, && vs and, ! vs not, -> vs =
│   └── syntax_guide.rs     # return not needed, null→none, let mut→var, throw→Result, etc.
```

### Migrated Call Sites

| Original Location | Migration Target Module | Status |
|-------------------|------------------------|--------|
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

| Location | Reason |
|----------|--------|
| `primary.rs` `\|x\|` closure syntax | Requires lookahead (HintContext doesn't have next token) |
| `helpers.rs` `expect_closing()` | Secondary span generation is a separate mechanism from hints |
| `declarations.rs` import `{` detection | Check depends on parse structure |

## Completed Phases

### Phase 3: Test Infrastructure — DONE (v0.5.12)

Table-driven tests: 43 tests covering all 5 modules. Success cases, failure cases, and scope verification.

### Phase 4: Extensions — DONE

- ✅ Added `next: Option<&Token>` to `HintContext`
- ✅ Migrated `|x|` closure hint from `primary.rs` inline to `operator.rs` (using lookahead)
- ✅ Added semicolon hint (`operator.rs`)
- ✅ Added 11 LLM error patterns (`syntax_guide.rs`): `self`/`this`, `new`, `void`, `undefined`, `switch`, `elif`/`elsif`/`elseif`, `extends`/`implements`, `lambda`
- ✅ Hint catalog (`catalog.rs`) — all hints retrievable via `all_hints()`
- ✅ 61 tests (+18 added)

## Status

**All phases complete.** This roadmap item can be moved to Done.

## Priority

This item is complete. Consider moving to `done/`.

## Reference

| Language | Hint system |
|----------|-------------|
| **Rust (rustc)** | `rustc_errors` crate, `Diagnostic` + `Subdiagnostic` derive macros, lint registry |
| **Swift** | `DiagnosticEngine` + `DiagnosticVerifier`, diagnostic IDs for each hint |
| **Elm** | Each error in independent module, `Error.xxx.toReport()` pattern |
| **TypeScript** | `Diagnostics.generated.ts` — error catalog managed via code generation |
