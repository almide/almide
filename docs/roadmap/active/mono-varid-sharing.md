<!-- description: Monomorphization shares VarIds across specializations, causing stale types in WASM/codegen -->
# Mono VarId Sharing

## Problem

`specialize_function` clones the generic function body but reuses the original VarIds. When multiple specializations exist (e.g., `safe_get__Int` and `safe_get__String`), the shared VarTable entries contain whichever type the LAST specialization wrote.

This causes downstream passes and the WASM emitter to read stale types from VarTable, producing wrong-sized loads/stores, incorrect clone decisions, and compilation failures.

## Known Manifestations

### Fixed (workarounds in place)

- **list.get element type** — `resolve_list_elem` falls back to expr `.ty` instead of VarTable
- **capture clone type** — `pass_capture_clone` collects actual Var types from lambda body instead of VarTable
- **LICM guard hoist** — LICM skips guard else to avoid hoisting control-flow values

### Remaining

1. **Generic variant `unbox_all[T]` WASM** — closure conversion reads VarTable for lifted lambda params, gets wrong type for the second specialization. `substitute_expr_types` was fixed (12 variants added) but closure conversion in WASM has its own VarTable reference path.

2. **Protocol generic `show_it[T: Show]`** — mono rewrites `T.show` → concrete method name, but the call target `T_show` isn't found in scope. Protocol method dispatch in mono doesn't fully substitute the convention method name.

## Ideal Fix

Make `specialize_function` allocate **fresh VarIds** for each specialization (alpha-renaming). The cloned body's Var references and VarTable entries would be independent, eliminating all stale-type bugs at once.

### Scope

- `crates/almide-optimize/src/mono/specialization.rs` — `specialize_function`: clone body with fresh VarIds
- `crates/almide-ir/src/substitute.rs` — add VarId remapping support
- All passes that read VarTable after mono — would "just work" with fresh VarIds

### Risk

Medium — VarId remapping must be consistent across the entire cloned body (patterns, statements, expressions). Missing a reference causes silent miscompilation.
