<!-- description: Track end column in diagnostics for precise error underlines -->
# Diagnostic end_col — Precise Error Underlines

**Priority:** Low-Medium — A polish pass for diagnostic quality. Major improvements already completed with secondary span activation
**Prerequisites:** Secondary spans activated (declaration site display for E006/E005/E009)

---

## Current State

- `Diagnostic` struct already has an `end_col: Option<usize>` field
- All call sites leave it as `None` — never populated
- Result: error highlights show only a single `^` instead of `^^^`

## Required Changes

### AST Layer
- Add `end_col: usize` to the `Span` struct in `src/ast.rs`
- Lexer needs to record the end column of each token

### Parser Layer
- Compute end_col during token generation in `src/lexer.rs`

### Type Checker Layer
- Set `span.end_col` → `diag.end_col` in `emit()` in `src/check/mod.rs`

## Impact Scope

- **Propagates across the entire AST** — every location that uses Span is affected
- Many parts of the compiler construct Span, so the change volume is large
- No functional breaking changes (end_col is Optional)

## Recommendation

Implement in a separate PR. Since secondary span activation already enables "displaying multiple locations simultaneously," urgency is low.
