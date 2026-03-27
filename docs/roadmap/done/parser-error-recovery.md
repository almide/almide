<!-- description: Continue parsing after syntax errors to report multiple diagnostics -->
<!-- done: 2026-03-15 -->
# Parser Error Recovery

## Summary
Improve the current behavior where parsing stops at a single syntax error. Report multiple errors and continue parsing after errors.

## Current Problem
```
error: Expected expression at line 5
  --> app.almd:5:10
```
Stops at 1 error. Subsequent correct code is not checked either.

## Goal
```
error: Expected expression at line 5
error: undefined variable 'x' at line 8
error: type mismatch at line 12

3 error(s) found
```

## Design

### Statement-level recovery
On parse failure, skip ahead to the next statement boundary (`;`, newline + keyword, `}`) and continue.

### Declaration-level recovery
On function/type declaration parse failure, skip ahead to the next `fn`/`type`/`test` and continue.

### Error node
Insert `Expr::Error` / `Stmt::Error` at parse failure locations. The checker skips these.

## Implementation
- `src/parser/mod.rs` — add `recover_to_sync_point()` method
- `src/parser/declarations.rs` — declaration-level recovery
- `src/parser/statements.rs` — statement-level recovery
- Tests: verify that multiple errors are reported

## Files
```
src/parser/mod.rs
src/parser/declarations.rs
src/parser/statements.rs
spec/lang/core_test.almd (error recovery tests)
```
