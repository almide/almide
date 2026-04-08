<!-- description: WASM closure mutable capture via heap cell for var mutation in lambdas -->
# WASM Closure Mutable Capture

## Problem

Mutable variable capture in closures doesn't work in WASM target. The variable mutation inside the lambda body is silently skipped, producing incorrect results.

```almide
effect fn running_sum(xs: List[Int]) -> List[Int] = {
  var acc = 0
  list.map(xs, (x) => { acc = acc + x; acc })
}
// Expected: [1, 3, 6, 10]
// Actual: WASM trap (acc mutation has no effect)
```

**Test files:**
- `spec/lang/escape_analysis_test.almd` — now passes (6/6) after Assign skip fix
- `spec/integration/codegen_functional_test.almd` — 1 test fails: "closure mutation (FnMut)"

## Root Cause

When a `var` declared in an outer scope is mutated inside a lambda, WASM codegen needs to:

1. **Allocate a heap cell** for the variable (not a stack local)
2. **Pass the cell pointer** through the closure environment
3. **Read/write via indirection** in both the outer function and the lambda

Currently:
- `mutable_captures` set is populated correctly (closures.rs:80)
- Cell locals are allocated for captured vars in the lambda (closures.rs:200-205)
- But the **outer function** doesn't convert the `var` to a cell — it stays as a stack local
- The `Assign` in the lambda body can't find the var in `var_map` (different scope), so it's skipped

## Fix Required

1. **Outer function**: When a `var` is in `mutable_captures`, allocate it as a heap cell (`__alloc(8)`) instead of a stack local. All reads/writes in the outer function go through the cell pointer.
2. **Closure creation**: Store the cell pointer (not the value) in the closure environment.
3. **Lambda body**: Read/write through the cell pointer loaded from the environment.

This is the same pattern Rust uses for `FnMut` captures (boxing the captured variable).

## Scope

- Affects: `var` mutation in lambdas (rare in practice — most Almide code uses immutable patterns)
- Does NOT affect: `var` read-only in lambdas (works), `var` mutation without lambdas (works), `let` captures (works)
