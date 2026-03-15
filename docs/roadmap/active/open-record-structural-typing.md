# Open Record / Structural Typing

**Test:** `spec/lang/open_record_test.almd`
**Status:** 16 rustc errors, 0 checker errors

## Current State

```almide
fn greet(who: { name: String, .. }) -> String = "hello ${who.name}"
```

Generates:

```rust
pub fn greet_named(who: Named) -> String {  // error: cannot find type Named
```

## What's Broken

Open records (`{ name: String, .. }`) use structural subtyping — any record with at least the specified fields is accepted. The codegen emits `Named` as the Rust type, which doesn't exist. Rust has no structural typing.

## Why It Happens

- The IR function param type is `Ty::TypeVar("Named")` (from generic structural bound) or `Ty::OpenRecord { fields: [...] }`
- `lower_ty` converts `TypeVar("Named")` → `Type::Named("Named")` which doesn't exist as a Rust struct
- Open record structural typing has no direct Rust equivalent
- The monomorphizer (`src/mono.rs`) already handles row-polymorphic functions but doesn't fully specialize open record params

## Expected Result

Two possible approaches:

### A. Monomorphization (simpler)

For each call to a function with open record params, create a specialized version:

```rust
// greet({ name: "Alice", age: 30 }) → specialized version
pub fn greet_AlmdRec0(who: AlmdRec0) -> String {
    format!("hello {}", who.name)
}
```

### B. Trait-based (idiomatic Rust)

```rust
trait HasName { fn name(&self) -> String; }
impl HasName for Person { fn name(&self) -> String { self.name.clone() } }
pub fn greet(who: &impl HasName) -> String {
    format!("hello {}", who.name())
}
```

## Proposed Fix

**Approach A** — fix the existing monomorphizer:

The monomorphizer (`src/mono.rs`) already exists and handles row-polymorphic functions. The issue is that:
1. It doesn't generate specialized function copies for all call sites with open record args
2. The call sites aren't rewritten to call the specialized versions
3. Structural bounds from generics (`T: { name: String, .. }`) aren't resolved to concrete types

Steps:
1. In `mono.rs`, when a function has `OpenRecord` or structural-bound params, collect all call sites
2. For each unique concrete record type passed, generate a specialized function copy
3. Rewrite call sites to use the specialized version
4. Remove the original generic function

**Effort:** ~100 lines to fix the monomorphizer, or ~200 lines for the trait approach
