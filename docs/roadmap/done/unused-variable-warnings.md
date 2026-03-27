<!-- description: Warn on unused variables and imports, suppressible with _ prefix -->
<!-- done: 2026-03-15 -->
# Unused Variable Warnings

## Summary
Warn on unused variables and imports. Suppressible with `_` prefix.

## Current State
Checker v3 only reports type errors. Unused variable warnings are not implemented.

## Goal
```
warning: unused variable 'x'
  --> app.almd:3:7
  hint: Prefix with '_' to suppress: _x
```

## Design

### Detection
Uses IR use-count (`compute_use_counts` in `ir.rs`). Warns on variables with `use_count == 0` that don't have a `_` prefix.

### Scope
- `let` / `var` bindings
- Function parameters excluded (for API compatibility)
- Pattern bindings (e.g., `b` unused in `let (a, b) = ...`)
- `_` prefix suppresses the warning

### Implementation
- `src/ir.rs` — revive `collect_unused_var_warnings()` (previously existed)
- `src/main.rs` / `src/cli.rs` — add warning output
- Tests: verify that unused variables produce warnings

## Files
```
src/ir.rs
src/main.rs
src/cli.rs
```
