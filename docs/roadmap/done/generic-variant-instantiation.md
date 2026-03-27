<!-- description: Fix type instantiation for generic variant constructors like Nothing -->
<!-- done: 2026-03-15 -->
# Generic Variant Type Instantiation

**Test:** `spec/lang/type_system_test.almd`
**Status:** ✅ Resolved (commit af22305)

## Problem

`type Maybe[T] = | Just(T) | Nothing` + `let y: Maybe[Int] = Nothing()` generates the following:

```rust
let y = Maybe::Nothing;  // error: type annotations needed for Maybe<_>
```

## Root Cause (3-Layer Problem)

### Layer 1: Constructor was returning empty type arguments

The variant constructor handling in `check_named_call` was returning `Ty::Named(type_name, vec![])`. A generic type with empty type arguments.

**Fix:** Added `instantiate_type_generics()`. Counts the TypeVars in the type definition, generates fresh inference variables for each. Changed to return `Ty::Named("Maybe", [TypeVar("?5")])`.

### Layer 2: Named type arguments were not being unified

In `unify_infer`'s `Concrete ↔ Concrete` path: `(Named(na, _), Named(nb, _)) if na == nb => true` — if the names matched, arguments were **ignored**. HM unification recursively unifies type constructor arguments.

**Fix:** Changed unification of `Named` types to convert arguments to `InferTy` via `from_ty` and unify recursively.

### Layer 3: resolve_inference_vars didn't reach Named arguments (**the true root cause**)

`resolve_inner` in `resolve_inference_vars` was handling `Ty::Named(name, args)` with the catch-all `_ => ty.clone()`. As a result, `?5` inside `Named("Maybe", [TypeVar("?5")])` was never resolved to `Int`, and `TypeVar("?5")` persisted in the IR.

**Fix (1 line):**

```rust
// Added to src/check/types.rs resolve_inner()
Ty::Named(name, args) if !args.is_empty() => {
    Ty::Named(name.clone(), args.iter().map(|a| Self::resolve_inner(a, solutions, seen)).collect())
}
```

## Correspondence to Type Theory

Two principles of HM (Hindley-Milner):

1. **Structural unification of type constructors**: unifying `Maybe(?5)` and `Maybe(Int)` derives `?5 = Int`
2. **Recursive resolution of type variables**: after constraint solving, substitute type variables within all type structures (including Named arguments)

Almide was missing the second one. This is standard in HM, but the `Named` case was omitted during the implementation of `resolve_inference_vars`.

## Changed Files

- `src/check/types.rs` — added recursive processing of `Ty::Named` in `resolve_inner` (+3 lines)
- `src/check/mod.rs` — made `unify_infer` structurally unify Named types (+4 lines)
- `src/check/calls.rs` — added `instantiate_type_generics`, constructors now generate fresh vars (+15 lines)
