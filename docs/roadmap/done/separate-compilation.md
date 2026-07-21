<!-- description: Separate compilation with unified IR linker for all targets -->
<!-- done: 2026-05-21 -->
# Separate Compilation

> **Target: v0.21 (Phase 1-4 shipped), Phase 5 future**
> **Status: Phase 1-4 complete**

## Problem (solved)

Almide emitted all packages into a single flat Rust file, causing symbol collisions, import deduplication, type alias shadowing, and fragile runtime inclusion.

## Architecture

```
Package A → parse → check → lower → IrProgram ─┐
Package B → parse → check → lower → IrProgram ─┤→ ir_link (stdlib scan)
Package C → parse → check → lower → IrProgram ─┘         │
                                                           ├→ IrLinkFlattenPass (merge modules)
                                                           ├→ emit_rust → .rs → rustc
                                                           └→ emit_wasm → .wasm
```

One IR linker for all targets. No Cargo workspace. No wasm-ld.

## Completed Phases

### Phase 1: ir_link + stdlib scan ✅

- `ir_link.rs` in almide-frontend: explicit merge point
- Collects `used_stdlib_modules` across all dependency modules
- `build.rs` auto-extracts `RUNTIME_DEPS` from source (no whitelist)
- `IrProgram.used_stdlib_modules` for IR-level runtime tracking

### Phase 2: IrLinkFlattenPass + walker simplification ✅

- `IrLinkFlattenPass` nanopass: merges `program.modules` into root
- Runs after `UnifyVarTablesPass` (VarIds already unified)
- Walker's 80-line per-module rendering loop deleted
- Functions get `almide_rt_{mod}_{name}` prefix in IR (not string replacement)

### Phase 3: Import/export model ✅

- `IrExport` enum: Function, Type, Constant
- `IrImport` struct: name + from_module
- `IrModule.exports` populated during lowering from visibility
- Foundation for unused import detection and cross-package error messages

### Phase 4: Prefix consolidation ✅

- Declaration prefix (fn + top_let): `IrLinkFlattenPass` only
- Call target prefix (→ RuntimeCall): `StdlibLowering` only
- `lower_module`: zero prefix logic
- Two owners, clear separation of concerns

## Future: Phase 5 — Incremental Compilation

Cache per-package IrPrograms on disk. Only re-lower changed packages. Re-link all.

### Design

```
.almide-cache/
├── mc_protocol.v0.ir    (serialized IrProgram)
├── mc_protocol.v0.hash  (source hash)
├── mc_bot.v0.ir
└── mc_bot.v0.hash
```

On build:
1. For each dependency, compute source hash
2. If hash matches cached `.hash`, load `.ir` from cache
3. If not, re-lower and update cache
4. `ir_link` + `IrLinkFlattenPass` on all (cached + fresh) IrPrograms
5. Codegen on merged result

### Prerequisites (all met)

- [x] Per-package IrProgram (lowering is already per-package)
- [x] Explicit merge point (ir_link)
- [x] IrProgram is Serialize/Deserialize (serde derives exist)
- [x] Stable IR format (no breaking changes expected)

### Not yet needed

Incremental compilation is a performance optimization. Current compile times are acceptable for the ecosystem size. Implement when:
- A project has 10+ dependency packages
- Clean build exceeds 10 seconds
- Users request it
