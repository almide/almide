<!-- description: Unify InferTy and Ty representations in the type checker -->
<!-- done: 2026-03-18 -->
# Checker InferTy/Ty Unification

**Priority:** post-1.0 (1.x)
**Estimate:** ~1000 lines, large. Core type system change.

## Current State

During type inference, `InferTy` (with unification variables) is used; after resolution, `Ty`. Conversion cost on every transition.

## Ideal

Express inference and resolution with a unified type. Simplify solutions table management.

## Tasks

- [ ] Design unified type combining `InferTy` and `Ty`
- [ ] Reduce conversion cost
- [ ] Simplify solutions table management

## Decision

Core type system change. Design carefully before implementing. Recommended for post-1.0.
