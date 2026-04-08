<!-- description: WASM closure mutable capture via heap cell for var mutation in lambdas -->
<!-- done: 2026-04-08 -->
# WASM Closure Mutable Capture

## Problem

Mutable variable capture in closures didn't work in WASM target. The variable mutation inside the lambda body was silently skipped, producing incorrect results.

```almide
effect fn running_sum(xs: List[Int]) -> List[Int] = {
  var acc = 0
  list.map(xs, (x) => { acc = acc + x; acc })
}
// Expected: [1, 3, 6, 10]
// Actual: WASM trap (acc mutation had no effect)
```

## Root Cause

The `ClosureConversion` nanopass (WASM-only) converts all Lambda nodes to ClosureCreate + lifted top-level functions. This broke mutable captures in two ways:

1. **Value-copy semantics**: ClosureCreate stores the captured value into the env at creation time. Each lambda invocation loads a local copy. Mutations to the local aren't visible to subsequent calls — no shared heap cell.

2. **Assign VarId not rewritten**: `rewrite_var_ids_stmt` rewrote Var references inside expressions but not the target VarId of `Assign` statements. The lifted function's Assign targeted the original (now-unmapped) VarId, so the WASM emitter skipped the assignment entirely.

A secondary issue: `demote_unused_mut` in `use_count.rs` correctly visits Lambda bodies when collecting assigned vars, but by ClosureConversion time the VarTable showed `Mutability::Let` for mutable captures, making mutability-based detection unreliable.

## Fix

**ClosureConversion now skips lambdas that assign to captured variables** (`pass_closure_conversion.rs`). These lambdas remain as `Lambda` nodes in the IR, and the WASM emitter's existing Lambda-based mutable capture path handles them correctly via heap cells:

1. **Outer function**: Allocates a heap cell for the `var`, stores the initial value. The local holds the cell pointer (i32).
2. **Closure env**: Stores the cell pointer (not the value).
3. **Lambda body**: Loads the cell pointer from env. All reads/writes go through cell indirection — mutations are immediately visible to subsequent calls.

Additionally fixed: `rewrite_var_ids_stmt` now rewrites target VarIds for `Assign`, `IndexAssign`, `MapInsert`, `FieldAssign`, and `ListSwap` statements.

## Tests

- `spec/integration/codegen_functional_test.almd` — "closure mutation (FnMut)" passes
- `spec/lang/escape_analysis_test.almd` — 6/6 pass
- Full WASM test suite: 176/176 pass (lang + stdlib + integration)
