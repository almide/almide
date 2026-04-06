<!-- description: Monomorphization shares VarIds across specializations, causing stale types in WASM/codegen -->
<!-- done: 2026-04-06 -->
# Mono VarId Sharing

## Problem

`specialize_function` cloned the generic function body but reused the original VarIds. When multiple specializations existed (e.g., `safe_get__Int` and `safe_get__String`), the shared VarTable entries contained whichever type the LAST specialization wrote.

This caused downstream passes and the WASM emitter to read stale types from VarTable, producing wrong-sized loads/stores, incorrect clone decisions, and compilation failures.

## Fix: Per-Specialization Alpha-Renaming

`specialize_function` now allocates **fresh VarIds** for each specialization. The cloned body's Var references and VarTable entries are fully independent.

### Changes

- `specialize_function` — collects all VarIds in the original function, allocates fresh IDs with substituted types, remaps all references in the cloned body
- `update_var_table_types` — **deleted**. No longer needed; fresh VarIds carry correct types at allocation time.
- `pass_capture_clone` — removed `collect_var_types` workaround; now trusts VarTable directly
- `resolve_list_elem` — multi-source fallback retained but VarTable source is now reliable

### Workaround status

| Workaround | Status |
|---|---|
| `resolve_list_elem` multi-source fallback | Retained (defensive, VarTable now correct) |
| `pass_capture_clone` collect_var_types | **Removed** (VarTable now reliable) |
| LICM guard else skip | Retained (semantic correctness, not VarTable) |

## Remaining issues (now unblocked by this fix)

1. **Protocol generic `show_it[T: Show]`** — mono rewrites `T.show` → concrete method name, but the call target `T_show` isn't found in scope. Protocol method dispatch in mono doesn't fully substitute the convention method name. (This is a mono rewrite issue, not a VarTable issue.)
