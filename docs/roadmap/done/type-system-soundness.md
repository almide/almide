<!-- description: Type system soundness fixes (Unknown propagation, unification, occurs) -->
<!-- done: 2026-03-15 -->
# Type System Soundness

## Summary
Raise the type system soundness from B+ to A+. Critical 3 + High 4 + Medium 3 + P1 4 = 14 fixes completed.

## Goal
- Minimize Unknown propagation
- Ensure unification correctly handles all type constructors
- Ensure occurs check fully prevents infinite types
- Prevent TypeVar from losing information when unresolvable

## Critical ✅ / High ✅ / Medium ✅

C-1 through C-3, H-1 through H-4, M-1 through M-3: all 10 items fixed.

## P1: Fix Unknown Propagation ✅

### P1-a: Gradual error-ification of `unwrap_or(Ty::Unknown)` ✅
- Classified 17 occurrences: intentional wildcard (12) / ICE (3) / inference failure (2)
- Added ICE logging to lower.rs `expr_ty()`, `resolve_type_expr`

### P1-b: Unknown halves of Result ✅
- `expressions.rs`: three-stage fallback for ok/err: expected → current_ret → Unknown
- `infer.rs`: changed ok/err defaults to fresh_var()

### P1-c: Lambda argument TypeVar → Unknown demotion ✅
- **Root cause**: `check_named_call` did not generate InferTy constraints for stdlib calls, so lambda fresh_vars were never resolved
- **Fix 1** (calls.rs): add constraints for InferTy arguments in `check_named_call`
- **Fix 2** (types.rs): post-resolve `Ty::TypeVar("?N")` in `InferTy::to_ty` output via `resolve_inference_vars`. Cycle detection with `seen` set
- **Fix 3** (mod.rs): apply `resolve_inference_vars` in `check_program` / `check_module_bodies`
- **Fix 4** (lower.rs): get lambda param types from checker inference results (`Fn` type params)

### P1-d: Unknown propagation in pattern matching ✅
- Emit warning when match subject is Unknown

## Files
```
src/check/types.rs        — resolve_inference_vars (post-solve TypeVar resolution)
src/check/calls.rs        — constraint propagation for stdlib calls
src/check/mod.rs          — apply resolve_inference_vars in check_program
src/check/expressions.rs  — ok/err bidirectional, match Unknown warning
src/check/infer.rs        — ok/err fresh_var
src/lower.rs              — lambda param type from checker, ICE logging
```
