<!-- description: Fix WASM validation errors from union-find generic instantiation -->
<!-- done: 2026-03-23 -->
# WASM Validation Fixes

## Status: Rust 153/153, WASM 1 compile failure (type_system_test), 8 skipped (Codec)

## Root of All Evil: Checker's Union-Find Generic Instantiation Contamination

### Symptom

For generic function `fn either_map_right[A, B, C](e: Either[A, B], f: (B) -> C) -> Either[A, C]`:

```
Where the checker should infer A=String, B=Int, C=Int,
Union-Find places A's fresh var into the same equivalence class as B/C's fresh vars,
contaminating A=Int.

Result: match subject.ty, arm body.ty, pattern.ty are all contaminated.
Left(a)'s a is typed as Int instead of String.
codegen emits i64_load (Int), but the actual payload is String (i32) → validation error.
```

### Root Cause

When calling a generic function via `check_call_with_type_args`:

1. `fresh_var()` assigns inference vars to each generic param (?N, ?M, ?O)
2. `constrain(param_ty_substituted, arg_ty)` unifies with argument types
3. Union-Find's `bind/union` binds ?N, ?M, ?O to concrete types

**Problem**: in step 3, `bind` overwrites existing bindings. After `?N = String` is set,
another constraint overwrites it with `?N = Int`. Or `union(?N, ?M)` places
different generic params into the same equivalence class.

### Why Codegen Patches Don't Fix This

The checker stores contaminated types in `expr_types` → lowering sets contaminated types in IR
→ mono replaces TypeVars but doesn't change concrete types → codegen emits instructions with contaminated types

**Contamination propagates throughout the entire IR.** scan, emit, pattern, match result — everything is affected.
Even if fixed individually in codegen, the same problem reoccurs at the next expression.

### Correct Fix

**Manage each generic param's fresh var independently** in the checker's generic instantiation.

Specific options:

**A. Scoped fresh vars**: create an independent fresh var set per generic function call,
isolated from other constraints until the call's constraint resolution completes.

**B. Bidirectional inference**: propagate expected types top-down, return inferred types bottom-up.
Generic params are resolved first from top-down expected types. The Union-Find overwrite problem doesn't occur.

**C. Constraint isolation**: resolve generic function call constraints in a separate solver context,
merge only results into the main context. Same concept as HM inference's let-polymorphism.

**Recommended**: A is minimal change. B is ideal but requires rewriting the entire checker. C is in between.

### What This Branch Did (Workaround)

- `IrPattern::Bind { var, ty }` — pattern carries its own type (VarTable-independent)
- mono `substitute_pattern_types` — replace pattern.ty with concrete types in mono
- Checker match result var isolation — directly return `first` arm type (no shared fresh var)
- Fix match.ty with func.ret_ty in propagate
- Use arm body WASM type consensus in emit_match

**All workarounds.** They become unnecessary once the checker is fixed.
