<!-- description: Track end column in diagnostics for precise error underlines -->
<!-- done: 2026-03-30 -->
# Diagnostic end_col — Precise Error Underlines

## Implementation

- Added `end_col: usize` to `Token` struct (lexer) — computed at every token construction site
- Added `end_col: usize` to `Span` struct (AST) — `#[serde(default)]` for backward compatibility with existing JSON
- `current_span()` now propagates `tok.end_col` into `Span`
- `emit()` in type checker and `diag_error()` in parser set `diag.end_col` from span
- `at_span()` on Diagnostic also wires through `end_col`

## Result

Before: single `^` caret on every error
After: `^^^^^` matches the width of the offending token

```
error[E003]: undefined variable 'undefinedVariable'
  --> file.almd:1:9
1 | let x = undefinedVariable
  |         ^^^^^^^^^^^^^^^^^
```
