<!-- description: Unify module-var / local-COW variable classification into a single VarStorage model, removing duplicated if-else mutation chains -->
<!-- done: 2026-05-15 -->
# VarStorage Refactor — Unified Variable Classification

## Status: Complete

## Problem

Module-level `var` and local COW (`RcCow<T>`) support was added incrementally across v0.16.6–v0.17.5. The result works (229/229 tests pass) but had structural issues:

1. **3 separate sets** in `CodegenAnnotations`:
   - `mutable_top_let_copy: HashSet<String>` — module var, Copy
   - `mutable_top_let_names: HashSet<String>` — module var, non-Copy
   - `rc_wrapped_vars: HashSet<VarId>` — local var, non-Copy COW

2. **if-else chains duplicated** in every mutation statement (Assign, IndexAssign, FieldAssign, MapInsert, ListSwap, ListReverse, ListRotateLeft, ListCopySlice) — same 3-way check copy-pasted 10+ times.

3. **`module_var_mut_target` helper** — a function that duplicated the non-Copy module var check, called from 5+ locations.

## Solution

### `VarStorage` enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarStorage {
    Local,      // plain local var
    RcCow,      // local var, non-Copy → RcCow<T>
    ModuleCell,  // module var, Copy → thread_local! { Cell<T> }
    ModuleRc,    // module var, non-Copy → thread_local! { RefCell<Rc<T>> }
}
```

### Unified lookup

Replaced 3 sets with:
```rust
pub var_storage: HashMap<VarId, VarStorage>,
pub var_storage_by_name: HashMap<String, VarStorage>,
```

Helper methods:
```rust
impl CodegenAnnotations {
    pub fn get_var_storage(&self, var: &VarId, name: &str) -> VarStorage { ... }
    pub fn is_rc_cow(&self, var: &VarId) -> bool { ... }
    pub fn is_module_var(&self, var: &VarId, name: &str) -> bool { ... }
}
```

### Statement rendering pattern

Each mutation statement now uses `match`:
```rust
match ctx.ann.get_var_storage(target, &target_str) {
    VarStorage::ModuleRc => emit_module_rc_mutation(...),
    VarStorage::ModuleCell => ...,
    VarStorage::RcCow => emit_rccow_mutation(...),
    VarStorage::Local => emit_local_mutation(...),
}
```

## Files changed

1. `crates/almide-ir/src/annotations.rs` — Added VarStorage enum, replaced 3 sets
2. `crates/almide-codegen/src/walker/mod.rs` — Populates var_storage in one pass
3. `crates/almide-codegen/src/walker/statements.rs` — All mutation stmts use match
4. `crates/almide-codegen/src/walker/expressions.rs` — Var read, Clone, Borrow, RuntimeCall use VarStorage

## Exit criteria

- [x] 3 old sets removed from CodegenAnnotations
- [x] All mutation statements use `match storage` instead of if-else chains
- [x] `module_var_mut_target` helper removed (replaced by VarStorage)
- [x] 229/229 tests pass
- [x] No behavioral change (pure refactor)
