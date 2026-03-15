# Generic Variant Type Instantiation

**Test:** `spec/lang/type_system_test.almd`
**Status:** 1 rustc error, 0 checker errors

## Current State

`type Maybe[T] = | Just(T) | Nothing` + `let y: Maybe[Int] = Nothing()` generates:

```rust
let y = Maybe::Nothing;  // error: type annotations needed for Maybe<_>
```

## What's Broken

The checker resolves `Nothing()` to `Maybe(TypeVar("T"))` — the generic param is NOT instantiated to `Int` even though the let binding has an explicit `Maybe[Int]` annotation. The lower pass stores the unresolved type in the IR var table. The codegen sees `Maybe<T>` and `contains_typevar` blocks the type annotation.

## Why It Happens

- Checker's `check_named_call` for variant constructors returns `Ty::Named(type_name, vec![])` — empty generic args
- The let binding's annotation `Maybe[Int]` is resolved correctly, but the constraint between annotation and value doesn't propagate the `Int` into the constructor's return type
- `InferTy::from_ty(&Ty::Named("Maybe", []))` → `InferTy::Concrete(Ty::Named("Maybe", []))` — no inference variables to solve

## Expected Result

```rust
let y: Maybe<i64> = Maybe::Nothing;
```

## Proposed Fix

In `check_named_call` for variant constructors (`src/check/calls.rs`), when the constructor belongs to a generic type, create fresh inference variables for the type params and return `Ty::Named(type_name, [Var(?N), ...])` instead of `Ty::Named(type_name, [])`. This allows the constraint solver to unify `Maybe(?N)` with `Maybe(Int)` from the annotation.

**Effort:** ~20 lines in `src/check/calls.rs`
