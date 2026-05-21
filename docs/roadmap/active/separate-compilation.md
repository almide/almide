<!-- description: Separate compilation with unified IR linker for all targets -->
# Separate Compilation

> **Target: v0.21**
> **Status: Phase 1 in progress**

## Problem

Almide emits all packages into a single flat Rust file. This causes:

- Symbol name collisions → manual `almide_rt_{mod}_{name}` prefixing
- `use` import deduplication → `net` + `http` collide
- Type alias shadowing → `type TcpStream = i64` shadows `std::net::TcpStream`
- Runtime inclusion by text search → fragile, breaks on transitive deps
- top_let prefix mismatches → cross-package constants not found

These were patched in v0.20. The root cause is single-file output without proper linking.

## Design Decision: Unified IR Linker

**One linker for all targets.** No Cargo workspace, no wasm-ld. The IR linker merges dependency IrPrograms. Each target's emitter receives a single merged program.

```
Package A → parse → check → lower → IrProgram ─┐
Package B → parse → check → lower → IrProgram ─┤→ IR linker → merged IrProgram
Package C → parse → check → lower → IrProgram ─┘         │
                                                           ├→ emit_rust → .rs → rustc
                                                           ├→ emit_wasm → .wasm
                                                           └→ (future) emit_llvm → .o
```

### Why not Cargo workspace?

Cargo workspace = target-specific linking. The IR linker is target-agnostic. Same merge for Rust and WASM. Simpler, fewer concepts.

### Why WASM-style?

WASM has no linker ecosystem, so IR-level merging is the only option. By making this the universal strategy, Rust benefits from the same clean architecture instead of having a separate Cargo-based path.

## Current State (v0.20 → v0.21)

### Done in v0.20

| Fix | Approach | Status |
|---|---|---|
| Runtime inclusion | `IrProgram.used_stdlib_modules` (IR scan) + `RUNTIME_DEPS` (auto-extracted from source by build.rs) | ✅ Shipped |
| top_let prefix | `ALMIDE_RT_{MOD}_{NAME}` prefix on module top_lets | ✅ Shipped |
| Hyphen in package names | Go convention: disallow hyphens, error with hint | ✅ Shipped |

### Done in v0.21 (Phase 1)

| Component | Description | Status |
|---|---|---|
| `ir_link.rs` | Explicit merge point (`almide-frontend/src/ir_link.rs`) | ✅ |
| CLI integration | `ir_link()` called before codegen at all 6 entry points | ✅ |
| Module stdlib scan | `ir_link` extends `used_stdlib_modules` with deps' references | ✅ |
| 235 tests | All pass | ✅ |

### Phase 1 does NOT flatten

`ir_link` currently extends metadata (stdlib modules) but does NOT flatten `program.modules`. The walker still iterates modules and applies prefixes. Flattening requires updating:

1. All `CallTarget` references (internal calls renamed)
2. Walker's per-module rendering (prefix insertion, type dedup)
3. VarTable merge with reindexing

These must change simultaneously. Doing one without the other breaks the 235 tests.

## Remaining Phases

### Phase 2: Flatten + walker simplification

1. `ir_link` merges module functions/types/top_lets into root
2. Update all `CallTarget::Named` references to prefixed names
3. Walker renders flat `program.functions` — no per-module loop
4. Remove: per-module prefix insertion, type dedup, separate top_let rendering
5. Remove: `program.modules` field entirely

**Exit criteria**: walker has no module-iteration code. All symbols in one flat namespace.

### Phase 3: Import/export model

Add explicit imports/exports to IrProgram:

```rust
enum Export {
    Function { name: Sym, sig: FnSig },
    Type { name: Sym, kind: IrTypeDeclKind },
    Constant { name: Sym, ty: Ty },
}
```

This enables:
- Better error messages for missing exports
- Unused import detection at package level
- Foundation for incremental compilation

### Phase 4: Prefix removal

Once flatten is solid:
- Remove `almide_rt_{mod}_{name}` from codegen entirely
- Functions become `pub fn {name}()` — linker handles namespacing
- StdlibLowering simplified

### Phase 5: Incremental compilation (future)

Cache per-package IrPrograms on disk. Only re-lower changed packages. Re-link all. Gives Cargo-level incrementality without Cargo.

## Architecture After Completion

```
checker  → per-package type checking (unchanged)
lowering → per-package IR generation (unchanged)
ir_link  → merge all IrPrograms into one flat program
optimize → monomorphize, DCE, etc. on merged IR
codegen  → emit_rust or emit_wasm on merged IR (no module awareness)
```

The linker is the single point where packages meet. Everything before it is per-package. Everything after it is whole-program.
