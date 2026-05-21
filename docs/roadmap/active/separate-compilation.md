<!-- description: Separate compilation with unified IR linker for all targets -->
# Separate Compilation

> **Target: v0.21**
> **Status: Design → Implementation**

## Problem

Almide emits all packages into a single flat Rust file. This causes:

- Symbol name collisions → manual `almide_rt_{mod}_{name}` prefixing
- `use` import deduplication → `net` + `http` collide
- Type alias shadowing → `type TcpStream = i64` shadows `std::net::TcpStream`
- Runtime inclusion by text search → fragile, breaks on transitive deps
- top_let prefix mismatches → cross-package constants not found

Every one of these was patched in v0.20.0. They'll keep recurring as the ecosystem grows. The root cause is single-file output without proper linking.

## Design

### Principle

**One linker for all targets.** The IR linker is the only linking strategy. Each target's emitter receives a merged IrProgram. No target-specific linking.

### Architecture

```
Package A → parse → check → lower → IrProgram ─┐
Package B → parse → check → lower → IrProgram ─┤→ IR linker → merged IrProgram
Package C → parse → check → lower → IrProgram ─┘         │
                                                           ├→ emit_rust → .rs → rustc
                                                           ├→ emit_wasm → .wasm
                                                           └→ (future) emit_llvm → .o
```

**No Cargo workspace. No wasm-ld.** The IR linker resolves everything. Emitters receive a single complete program.

### IR Linker

```rust
fn ir_link(packages: Vec<(PkgId, IrProgram)>) -> IrProgram
```

Responsibilities:
1. **Merge function tables** — each package's functions are namespaced by package
2. **Merge type declarations** — deduplicate shared types
3. **Merge top_lets** — namespaced constants
4. **Resolve cross-package references** — DefTable already tracks these
5. **Merge VarTables** — reindex VarIds to avoid collisions
6. **Collect used_stdlib_modules** — union across all packages

What it does NOT do:
- Type checking (already done per-package)
- Optimization (done per-package or on merged IR)
- Target-specific decisions (emitters handle that)

### IrProgram: imports and exports

Each package's IrProgram gains explicit boundaries:

```rust
struct IrProgram {
    // ... existing fields ...
    exports: Vec<Export>,   // public functions, types, constants
    imports: Vec<Import>,   // what this package needs from dependencies
}

enum Export {
    Function { name: Sym, sig: FnSig },
    Type { name: Sym, kind: IrTypeDeclKind },
    Constant { name: Sym, ty: Ty },
}

enum Import {
    Function { name: Sym, from_package: PkgId },
    Type { name: Sym, from_package: PkgId },
    Constant { name: Sym, from_package: PkgId },
}
```

The IR linker matches imports against exports and produces a flat merged program.

### Runtime

The runtime (`runtime/rs/src/*.rs`) is a special "package" that the linker always includes. For Rust, its source is inlined into the merged output (as today). For WASM, it's compiled inline (as today).

Future: the runtime becomes a real crate for Rust, and a precompiled WASM module for WASM. But that's an optimization, not a correctness requirement.

### What the Rust emitter receives

A merged IrProgram where:
- All cross-package references are resolved to concrete function/type/constant definitions
- No package boundaries remain — it's one flat program
- The emitter generates one .rs file and calls rustc (same as today)

The difference from today: the merge is done correctly by the IR linker instead of ad-hoc concatenation with prefix hacks.

### What the WASM emitter receives

Same merged IrProgram. No change from today's emit_wasm input.

## Migration Plan

### Phase 1: IR linker + formalize `program.modules`

What exists today: dependencies are loaded into `program.modules` during resolution. The codegen iterates over modules and prefixes their symbols.

What Phase 1 does:
1. Extract the implicit merge logic into an explicit `ir_link()` function
2. `ir_link()` takes the root IrProgram + dependency IrPrograms
3. Produces a merged IrProgram with all symbols properly namespaced
4. Codegen receives the merged result — no more per-module iteration with ad-hoc prefixing

### Phase 2: Remove prefix machinery

Once ir_link handles namespacing:
1. Remove `almide_rt_{mod}_{name}` prefixing from walker
2. Remove runtime text-search inclusion (IR linker provides used_stdlib_modules)
3. Remove `use` deduplication logic
4. Remove type alias shadow workarounds

### Phase 3: Import/export model

Add explicit imports/exports to IrProgram. This enables:
- Better error messages for missing exports
- Unused import detection at the package level
- Foundation for incremental compilation (only recompile changed packages)

### Phase 4: Incremental compilation (future)

Cache per-package IrPrograms. Only re-lower changed packages. Re-link all. This gives Cargo-level incrementality without Cargo.

## Exit Criteria

- `mc-bot-cli → mc-bot → mc_protocol` 3-level dep chain builds correctly
- No `almide_rt_` prefix in generated Rust code
- No text-search runtime inclusion
- Same IR linker used for Rust and WASM targets
- 235+ tests pass
