# Recursive Type Box Insertion

**Test:** `spec/lang/eq_protocol_test.almd`
**Status:** 4 rustc errors (recursive type + AnonRecord), 0 checker errors

## Current State

`type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])` generates:

```rust
pub enum Tree<T> {
    Leaf(T),
    Node(Tree<T>, Tree<T>),  // error: recursive type has infinite size
}
```

## What's Broken

Rust requires `Box<Tree<T>>` for recursive type members. The codegen emits direct recursive references without indirection.

## Why It Happens

- The IR `IrTypeDeclKind::Variant` stores variant fields as direct `Ty` references
- The Rust codegen (`lower_rust.rs` enum emission) converts fields via `lty()` without checking for self-reference
- No cycle detection exists in the codegen pipeline

## Expected Result

```rust
pub enum Tree<T> {
    Leaf(T),
    Node(Box<Tree<T>>, Box<Tree<T>>),
}
```

## Proposed Fix

In `lower_rust.rs` when emitting enum variants, detect self-referential fields (where the field type contains the enum's own name) and wrap them in `Box<T>`. Also need to wrap construction sites in `Box::new()` and add `*` dereference at pattern match sites.

Steps:
1. `emit_enum_def`: if variant field type contains the enum name → `Type::Box(inner)`
2. `lower_expr` for Constructor with recursive args → `Expr::Call { func: "Box::new", args: [inner] }`
3. `lower_pat` for recursive patterns → add deref
4. Add `Box` variant to `rust_ir::Type` and rendering in `render.rs`

**Effort:** ~50 lines across lower_rust.rs, lower_rust_expr.rs, rust_ir.rs, render.rs
