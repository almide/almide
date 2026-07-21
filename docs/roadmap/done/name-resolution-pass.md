<!-- description: Unified cross-package symbol resolution via DefId/DefTable, replacing ad-hoc name lookup -->
<!-- done: 2026-05-14 -->
# Name Resolution Pass — Unified Cross-Package Symbol Resolution

## Status: Done (Phase 1-3 shipped in v0.18.1, Phase 4 cleanup deferred to cross-package builds)

### Progress
- Phase 1 (DefId/DefTable infrastructure): ✅ `DefId`, `DefTable`, `DefInfo`, `DefKind` in `almide-ir`.
- Phase 2a (populate DefTable): ✅ `register_decls` registers all Function, Type, TopLet. `def_map` (qualified name → DefId) in TypeEnv.
- Phase 2b (IR propagation): ✅ `IrFunction.def_id` and `IrTopLet.def_id` populated from def_map during lowering.
- Phase 2c (checker integration): ✅ def_map-first lookup in `check_named_call_with_type_args`.
- Phase 3 (CallTarget DefId): ✅ `CallTarget::Module` gains `def_id: Option<DefId>`. Lowering populates from def_map. 31 files updated.
- v0.17.10 workaround: `module_versioned_names` still in place (to be removed in Phase 4).
- Phase 4 (remove ad-hoc resolution): not started. Best done while building ceangal cross-package.

## Problem

Module identity flows through the compiler as **string names**, with each pass independently re-resolving import aliases, qualified names, and cross-module references. This causes systematic failures in cross-package compilation:

| Pass | Issue |
|------|-------|
| `check/infer.rs` | `gpu.STORAGE` looked up as `"gpu.STORAGE"` instead of `"snaidhm.web.gpu.STORAGE"` |
| `check/calls.rs` | Same — module-qualified function calls not resolved |
| `lower/expressions.rs` | Same — Member access, function refs, and synthetic var names all use raw aliases |
| `emit_wasm/mod.rs` | Synthetic name `ALMIDE_RT_SNAIDHM_WEB_GPU_STORAGE` vs VarTable name `ALMIDE_RT_SNAIDHM_V0_WEB_GPU_STORAGE` |
| `emit_wasm/expressions.rs` | VarId-based global lookup collides with unrelated globals after UnifyVarTablesPass |
| `emit_wasm/collections.rs` | Record field access emits `unreachable` for cross-package types |

All are the same structural bug: **ad-hoc string-based name resolution scattered across 6+ files**.

## Root Cause

No single pass resolves all references to their canonical definitions. Each pass (infer, calls, lower, codegen) independently calls `import_table.resolve()` — or forgets to — and constructs qualified names using different conventions (dots, underscores, `_V0_` suffixes, `ALMIDE_RT_` prefixes).

## Solution: DefId-based Resolution

### Phase 1: Introduce `DefId`

A unique identifier for every named definition (function, type, top-level let, module) across all packages:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct DefId(u32);

pub struct DefTable {
    entries: Vec<DefInfo>,
}

pub struct DefInfo {
    pub package: Sym,      // "snaidhm"
    pub module: Sym,       // "snaidhm.web.gpu"
    pub name: Sym,         // "STORAGE"
    pub kind: DefKind,     // Function, Type, TopLet, Module
    pub ty: Ty,
}
```

### Phase 2: Name Resolution Pass (after parsing, before type checking)

A single pass that walks all AST expressions and resolves every `Ident`, `Member`, and qualified call to its `DefId`:

```
AST → NameResolution → ResolvedAST → TypeCheck → IR → Codegen
```

The resolved AST carries `DefId` instead of string names. All downstream passes use `DefId` exclusively.

### Phase 3: Propagate DefId through IR

- `IrExprKind::Var` carries `DefId` for cross-module references (not synthetic `ALMIDE_RT_` names)
- `CallTarget::Module` carries `DefId` instead of `(Sym, Sym)`
- `IrTopLet` carries `DefId` for global registration
- Codegen maps `DefId → global_idx` directly (no name-based fallback)

### Phase 4: Remove ad-hoc resolution

Delete all `import_table.resolve()` calls from:
- `check/infer.rs` (Member access)
- `check/calls.rs` (qualified calls)
- `lower/expressions.rs` (Member access, synthetic vars)
- `emit_wasm/expressions.rs` (name-based global lookup fallback)
- `emit_wasm/mod.rs` (ALMIDE_RT_ synthetic name registration)

## How Other Languages Do This

**Rust**: `DefId` in `rustc_hir`. Name resolution (`rustc_resolve`) runs before type checking. All subsequent passes use `DefId`.

**Go**: Package path is canonical. After name resolution, all references are fully qualified. No aliases in the IR.

**Swift**: Module names are first-class in SIL. Mangled names encode the full module path.

## Scope

- **Affected crates**: `almide-frontend` (resolve, check, lower), `almide-ir`, `almide-codegen`
- **Estimated complexity**: Medium-large (touches core data structures)
- **Testing**: Must pass all 227 existing tests + new cross-package tests
- **Validation**: ceangal → snaidhm single WASM with full GPU rendering + DOM overlay

## Current Workarounds (to be removed)

- `infer.rs`: import_table.resolve() before env.functions lookup
- `calls.rs`: resolved_name computation for module-qualified calls
- `lower/expressions.rs`: resolved_mod for qual_let_key and synthetic names
- `emit_wasm/mod.rs`: multi-form ALMIDE_RT_ name registration + _V0_ stripping
- `emit_wasm/expressions.rs`: name-based lookup prioritized over VarId-based
