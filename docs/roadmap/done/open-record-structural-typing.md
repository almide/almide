<!-- description: Open record / row polymorphism implementation for structural typing -->
<!-- done: 2026-03-16 -->
# Open Record / Row Polymorphism — Implementation Guide

**Tests:** `spec/lang/open_record_test.almd`
**Status:** 16 rustc errors, 0 checker errors
**Theory:** Remy 1989 Row Polymorphism

## Current Architecture

```
Checker (src/check/)        → OpenRecord passes compatible check ✅
Monomorphizer (src/mono.rs) → Only handles generic + structural bound ⚠️
Codegen (src/emit_rust/)    → Cannot convert OpenRecord / TypeVar("Named") to Rust type ❌
```

## Two Patterns Required by Tests

### Pattern A: Direct OpenRecord Parameter
```almide
fn greet(who: { name: String, .. }) -> String = "Hello, ${who.name}!"
greet(Dog { name: "Rex", breed: "Lab" })  // Dog は name を持つ
```
**Not recognized by monomorphizer** — because there are no generics.

### Pattern B: Generic + Structural Bound
```almide
fn describe[T: { name: String, .. }](x: T) -> String = "name: ${x.name}"
describe(Dog { name: "Rex", breed: "Lab" })
```
**Already handled by monomorphizer** — specialized in `src/mono.rs`.

## Files to Modify

### 1. `src/mono.rs` — Extend find_structurally_bounded_fns

```rust
// Current: only detects generic + structural bound
fn find_structurally_bounded_fns(functions: &[IrFunction]) -> HashMap<String, Vec<BoundedParam>> {
    for func in functions {
        if let Some(ref generics) = func.generics {
            // detect generic params with structural_bound
        }
    }
}

// Fix: also detect direct OpenRecord parameters
fn find_open_record_fns(functions: &[IrFunction]) -> HashMap<String, Vec<OpenRecordParam>> {
    for func in functions {
        for (i, param) in func.params.iter().enumerate() {
            if matches!(&param.ty, Ty::OpenRecord { .. }) {
                // this function is a monomorphization candidate
            }
        }
    }
}
```

### 2. `src/mono.rs` — Extend discover_instances

Collect concrete types passed at call sites:

```rust
// greet(Dog { name: "Rex", breed: "Lab" })
// → call target: "greet", args[0].ty = Named("Dog", [])
// → instance: ("greet", "Dog") → { param_0 → Dog }
```

Traverse IR Call nodes; register an instance when the target is an open record fn and the arg type is Named.

### 3. `src/mono.rs` — Extend specialize_function

Replace open record parameter types with concrete types:

```rust
// greet(who: { name: String, .. }) → greet__Dog(who: Dog)
// who.name in function body resolves to Dog::name
```

### 4. `src/mono.rs` — Extend rewrite_calls

Redirect call sites to specialized versions:

```rust
// greet(dog) → greet__Dog(dog)
```

## Algorithm Core (Row Unification)

```
unify({ name: String, .. }, Dog)
  ↓
Resolve Dog → { name: String, breed: String }
  ↓
{ name: String | ρ } vs { name: String, breed: String | RowEmpty }
  ↓
Common: name: String ✓
Remainder: breed: String → goes into ρ
  ↓
ρ = { breed: String }
```

This **already works** in the checker (compatible check). Only the codegen-side monomorphization is missing.

## Implementation Order

1. `find_open_record_fns()` — Detect functions with OpenRecord parameters
2. `discover_instances()` — Collect concrete types at each call site
3. `specialize_function()` — Generate function copies with OpenRecord replaced by concrete type
4. `rewrite_calls()` — Redirect call sites to specialized versions
5. Fallback for `TypeVar("Named")` in codegen to resolved named type

## Expected Test Results

```
spec/lang/open_record_test.almd: 16 tests pass
```

All 16 tests pass Rust compilation and satisfy runtime assertions.
