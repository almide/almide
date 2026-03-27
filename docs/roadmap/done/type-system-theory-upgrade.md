<!-- description: Hindley-Milner integration plan (type schemes, let-polymorphism) -->
<!-- done: 2026-03-16 -->
# Type System Theory Upgrade — HM Integration Plan

## Current State: What Almide Has

Almide's checker is **constraint-based without type schemes** — a pragmatic simplification of Hindley-Milner.

| Feature | HM (Algorithm W) | Almide | Gap |
|---------|------------------|--------|-----|
| Occurs check | Yes | **Yes** | None |
| Unification | Eager | **Hybrid** (eager + batch) | None (pragmatic) |
| Type schemes (∀α. τ) | Essential | **Missing** | CRITICAL |
| Instantiation (fresh vars per use) | Yes | **Partial** (direct unify) | HIGH |
| Generalization (abstract unbound vars) | Yes | **Missing** | HIGH |
| Let-polymorphism | Yes | **No** (monomorphic lets) | Design choice |
| Row variables (ρ) | Optional | **Missing** | MEDIUM |
| µ-types (recursive) | Optional | **Implicit** (naming) | LOW |

## What Almide Gets Right

- **Occurs check** — prevents infinite types, follows solution chains
- **Structural record unification** — `OpenRecord` matching works
- **Eager unification in `constrain()`** — propagates info into lambda bodies
- **Explicit generics on function defs** — `fn id[T](x: T) -> T` avoids the need for inference
- **Monomorphization** for structural bounds — practical substitute for row polymorphism

## The Core Problem

Almide stores **monomorphic types** in the environment. When `fn id[T](x: T) -> T` is registered, `T` is a `TypeVar("T")` string, not a quantified variable. When `Just(42)` is called, the constructor returns `Maybe(TypeVar("T"))` with empty generic args — no fresh variables are created.

**Result:** Type information doesn't flow from annotations/call sites back to generic constructors.

## How Each Issue Maps to Theory

### Issues solved by proper Instantiation (~20 lines each)

**1. Generic Variant (`type_system_test`)**
- `Nothing()` returns `Maybe(TypeVar("T"))` → should return `Maybe(Var(?N))` with fresh var
- Fix: in `check_named_call` for constructors, instantiate parent type's generics with fresh vars

**2. Higher-Order Function (`function_test`)**
- `adder(3)` returns `fn(Int) -> Int`, then `adder(3)(10)` fails
- Actually a **checker call dispatch** issue, not HM — the `_` catch-all in `check_call` should handle callable variable types
- Fix: check if callee variable has `Fn` type, constrain against args

### Issue solved by cycle detection (~50 lines)

**3. Recursive Type Box (`eq_protocol_test`)**
- `Tree(Tree, Tree)` → Rust needs `Box<Tree>`
- Not a type theory issue — just codegen graph analysis (SCC detection)
- Fix: in `lower_rust.rs`, check if variant fields reference the parent enum name

### Issue solved by row variable tracking (~100 lines)

**4. Open Record (`open_record_test`)**
- `{ name: String, .. }` → Rust has no structural typing
- Monomorphizer exists but doesn't cover all call sites
- Fix: improve `src/mono.rs` to specialize all structurally-bounded function params

### Issue solved by IR investigation (~10 lines)

**5. Guard Value Loss (`error_test`)**
- `guard i < len else ok(count)` → `return Ok(())` instead of `return Ok(count)`
- Not a type theory issue — IR lowering bug
- Fix: trace guard else lowering to find where `count` is replaced by `()`

## Minimum Viable Upgrade: Instantiation Only

**Don't rewrite the checker.** Add one mechanism: **fresh variable instantiation for generic type constructors.**

```rust
// In check_named_call for variant constructors (src/check/calls.rs):
if let Some((type_name, case)) = self.env.constructors.get(name).cloned() {
    // CURRENT: returns Named(type_name, vec![])
    // FIX: instantiate generics with fresh vars
    let type_def_generics = self.env.get_type_generics(&type_name);
    let fresh_args: Vec<InferTy> = type_def_generics.iter()
        .map(|_| self.fresh_var())
        .collect();
    let result_ty = Ty::Named(type_name, fresh_args.iter()
        .map(|v| v.to_ty(&self.solutions))
        .collect());
    // Constrain constructor args against instantiated field types
    // ...
    InferTy::from_ty(&result_ty)
}
```

This single change fixes **#1 (type_system_test)** and lays groundwork for **#2 (function_test)**.

## Full HM Upgrade (Future, ~300 lines)

If Almide ever needs full let-polymorphism:

1. **Add `TypeScheme`:** `Poly { bound_vars: Vec<String>, body: Ty }`
2. **Generalize after let:** abstract over unresolved type vars not in enclosing scope
3. **Instantiate on lookup:** each use of a polymorphic binding gets fresh vars
4. **Store schemes in `TypeEnv`:** instead of `Ty`

This enables:
```almide
let f = (x) => x        // f : ∀T. T → T
let a = f(1)            // a : Int (T instantiated to Int)
let b = f("hello")      // b : String (T instantiated to String)
```

## References

| Priority | Resource | What You Get |
|----------|----------|-------------|
| 1 | TAPL Ch.22 (Type Reconstruction) | Algorithm W complete understanding |
| 2 | github.com/wh5a/Algorithm-W-Step-By-Step | Haskell impl with walkthrough |
| 3 | TAPL Ch.20 (Recursive Types) | µ-type theory, iso vs equi |
| 4 | Rémy 1989 "Type checking records and variants" | Row polymorphism original paper |
| 5 | PureScript type checker source | HM + row polymorphism in practice |
