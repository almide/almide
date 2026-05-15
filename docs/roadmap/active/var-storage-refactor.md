# VarStorage Refactor — Unified Variable Classification

## Status: Active (design complete, implementation pending)

## Problem

Module-level `var` and local COW (`RcCow<T>`) support was added incrementally across v0.16.6–v0.17.5. The result works (229/229 tests pass) but has structural issues:

1. **3 separate sets** in `CodegenAnnotations`:
   - `mutable_top_let_copy: HashSet<String>` — module var, Copy
   - `mutable_top_let_names: HashSet<String>` — module var, non-Copy
   - `rc_wrapped_vars: HashSet<VarId>` — local var, non-Copy COW

2. **if-else chains duplicated** in every mutation statement (Assign, IndexAssign, FieldAssign, MapInsert, ListSwap, ListReverse, ListRotateLeft, ListCopySlice) — same 3-way check copy-pasted 10+ times.

3. **Rust walker and WASM emit solve the same problem separately** — "is this var local or global?" is answered differently in each, causing bugs when one side is updated but the other isn't.

## Design

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

Replace 3 sets with:
```rust
pub var_storage: HashMap<VarId, VarStorage>,
pub var_storage_by_name: HashMap<String, VarStorage>,
```

Add helper:
```rust
impl CodegenAnnotations {
    pub fn get_var_storage(&self, var: &VarId, name: &str) -> VarStorage { ... }
    // Convenience:
    pub fn is_rc_cow(&self, var: &VarId) -> bool { ... }
    pub fn is_module_cell(&self, name: &str) -> bool { ... }
    pub fn is_module_rc(&self, name: &str) -> bool { ... }
}
```

### Statement rendering pattern

Each mutation statement becomes:
```rust
IrStmtKind::IndexAssign { target, index, value } => {
    let name = ctx.var_name(*target).to_string();
    let storage = ctx.ann.get_var_storage(target, &name);
    match storage {
        VarStorage::ModuleRc => emit_module_rc_mutation(...),
        VarStorage::ModuleCell => unreachable for list index,
        VarStorage::RcCow => emit_rccow_mutation(...),
        VarStorage::Local => emit_local_mutation(...),
    }
}
```

### WASM: `emit_var_get` helper (already added)

```rust
impl FuncCompiler<'_> {
    fn emit_var_get(&mut self, var: &VarId) -> bool {
        // Try local var_map, then global top_let_globals
    }
}
```

## Files to change

1. `crates/almide-ir/src/annotations.rs` — Add VarStorage enum, replace 3 sets
2. `crates/almide-codegen/src/walker/mod.rs` — Populate var_storage in one pass
3. `crates/almide-codegen/src/walker/statements.rs` — Replace if-else chains with match
4. `crates/almide-codegen/src/walker/expressions.rs` — Replace scattered checks
5. `crates/almide-codegen/src/emit_wasm/statements.rs` — Already uses emit_var_get, clean up

## Exit criteria

- [ ] 3 old sets removed from CodegenAnnotations
- [ ] All mutation statements use `match storage` instead of if-else chains
- [ ] `module_var_mut_target` helper removed (replaced by VarStorage)
- [ ] 229/229 tests pass
- [ ] No behavioral change (pure refactor)
