<!-- done: 2026-03-16 -->
# Let-Polymorphism (Algorithm W)

## Current State

```almide
let f = (x) => x        // f : fn(?0) -> ?0 (monomorphic)
let a = f(1)            // ?0 fixed to Int
let b = f("hello")      // error: ?0 is already Int
```

Almide has no type scheme (∀α. τ). let bindings are monomorphic.

## Goal

```almide
let f = (x) => x        // f : ∀T. T -> T (polymorphic)
let a = f(1)            // instantiate T=Int → a: Int
let b = f("hello")      // instantiate T=String → b: String
```

## Foundation Verified in Today's Fix

| Foundation | Status |
|-----------|--------|
| `InferTy::Var` + `TyVarId` | ✅ Working |
| `unify_infer` (structural unification) | ✅ Including Named arguments |
| `resolve_inference_vars` | ✅ With Named recursive processing |
| TypeVar → Var conversion in `from_ty` | ✅ |
| eager constraint solving | ✅ |

## Three Features to Add

### 1. TypeScheme Type

```rust
pub enum TypeScheme {
    Mono(Ty),
    Poly { bound_vars: Vec<TyVarId>, body: Ty },
}
```

### 2. Generalize (after let binding)

```rust
fn generalize(ty: &Ty, env_vars: &HashSet<TyVarId>) -> TypeScheme {
    let free = find_free_vars(ty);
    let bound: Vec<TyVarId> = free.difference(&env_vars).collect();
    if bound.is_empty() { TypeScheme::Mono(ty) }
    else { TypeScheme::Poly { bound_vars: bound, body: ty } }
}
```

In the `Let` case of `check_stmt`, call generalize after inferring the value.

### 3. Instantiate (at variable reference)

```rust
fn instantiate(scheme: &TypeScheme) -> InferTy {
    match scheme {
        TypeScheme::Mono(ty) => InferTy::from_ty(ty),
        TypeScheme::Poly { bound_vars, body } => {
            let fresh: HashMap<TyVarId, InferTy> = bound_vars.iter()
                .map(|v| (*v, self.fresh_var()))
                .collect();
            substitute_infer(body, &fresh)
        }
    }
}
```

In the `Ident` case of `infer_expr`, call instantiate after looking up the variable.

## Changed Files

- `src/check/types.rs` — add `TypeScheme` enum
- `src/check/mod.rs` — `generalize`, `instantiate`, `find_free_vars`
- `src/check/infer.rs` — instantiate at `Ident`, generalize at `Let`
- `src/types/env.rs` — change `TypeEnv` scopes to `HashMap<String, TypeScheme>`

## Impact Scope

Existing explicit generics (`fn id[T](x: T) -> T`) continue to work as-is.
Let-polymorphism is an **additive feature**. Existing tests will not break.

## Estimate

3-5 days. Direct implementation of Algorithm W from TAPL Ch.22.
