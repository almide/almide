<!-- description: Ground-up rewrite of the 890-line source formatter -->
<!-- done: 2026-03-15 -->
# Formatter Rewrite

## Summary
Rewrite `src/fmt.rs` (890 lines) from scratch. Old code has lambda syntax updated, but overall design is outdated.

## Current State
- Single 890-line file
- Original code unchanged (checker/lower/codegen have all been rewritten)
- Works, but formatting of new features (paren lambda, structural bounds) is incomplete

## Goal
- < 500 lines
- Pure AST → String transformation
- Support new syntax: `(x) => expr`, `[T: { .. }]`, union types

## Design
- `src/fmt.rs` — rewrite
- Pure function per AST node → formatted string
- Simple depth parameter for indent management

## Files
```
src/fmt.rs
```
