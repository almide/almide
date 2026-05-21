<!-- description: Separate compilation: each package → independent compilation unit, linked by Cargo (Rust) or IR linker (WASM) -->
# Separate Compilation

> **Target: v0.21**
> **Status: Design**

## Problem

Almide emits all packages into a single flat Rust file. This causes:

- Symbol name collisions → manual `almide_rt_{mod}_{name}` prefixing
- `use` import deduplication → `net` + `http` collide
- Type alias shadowing → `type TcpStream = i64` shadows `std::net::TcpStream`
- Runtime inclusion by text search → fragile, breaks on transitive deps
- top_let prefix mismatches → cross-package constants not found

Every one of these was patched in v0.20.0. They'll keep recurring as the ecosystem grows. The root cause is single-file output.

## Design

### Principle

**Each Almide package is an independent compilation unit.** Type checking and lowering are already per-package. Only codegen merges everything — that's what changes.

### Architecture

```
                      ┌─ Rust:  each IrProgram → crate → Cargo links
Package A → IR ───────┤
Package B → IR ───────┤
Package C → IR ───────┤─ WASM:  IR linker merges → emit_wasm → .wasm
                      └─ (future) LLVM: each IR → .o → lld links
```

Common pipeline up to IR. Divergence is only in the linking strategy.

### Rust Target

Each package becomes a Rust crate in a Cargo workspace.

```
.almide-build/
├── Cargo.toml              (workspace)
├── almide_runtime/         (shared runtime crate)
│   ├── Cargo.toml
│   └── src/lib.rs          (runtime/rs/src/*.rs consolidated)
├── mc_protocol/            (dependency package)
│   ├── Cargo.toml          (depends on almide_runtime)
│   └── src/lib.rs          (generated from mc_protocol IR)
├── mc_bot/                 (dependency package)
│   ├── Cargo.toml          (depends on almide_runtime, mc_protocol)
│   └── src/lib.rs
└── mc_bot_cli/             (root package)
    ├── Cargo.toml          (depends on all above)
    └── src/main.rs
```

**What this eliminates:**
- `almide_rt_{mod}_{name}` prefix → `pub fn name()` in its own crate
- `use` deduplication → each crate has its own imports
- Type alias shadowing → separate namespaces
- Runtime text search → `almide_runtime` crate always available
- top_let prefix → `pub const NAME` in the crate, imported via `use`
- Transitive dep resolution → Cargo handles it

**Generated Cargo.toml for a dependency crate:**
```toml
[package]
name = "almide-gen-mc_protocol"
version = "0.1.0"
edition = "2021"

[dependencies]
almide-runtime = { path = "../almide_runtime" }
```

**Generated lib.rs:**
```rust
use almide_runtime::*;

pub const DEFAULT_THRESHOLD: i64 = 256;

pub fn connect(host: &str, port: i64) -> Result<Connection, String> {
    let s = almide_rt_net_tcp_connect(host, port)?;
    Ok(Connection { stream: s, threshold: DEFAULT_THRESHOLD })
}
```

No prefix. No deduplication. No shadow. Cargo does the rest.

### WASM Target

WASM has no linker ecosystem. Use an **IR linker** instead:

1. Each package is compiled to `IrProgram` independently (same as Rust path)
2. **IR linker** merges all `IrProgram`s into one (resolve cross-package DefIds)
3. `emit_wasm` takes the merged `IrProgram` and emits a single `.wasm`

This is what we do today, but formalized:
- Currently: packages are merged implicitly in `program.modules`
- After: explicit `ir_link(programs: Vec<IrProgram>) -> IrProgram`

The IR linker is simple because cross-package references are already tracked via `DefTable`.

### Runtime Crate

The `almide_runtime` crate contains all `runtime/rs/src/*.rs` modules. It's always available to generated crates.

```rust
// almide_runtime/src/lib.rs
pub mod string;
pub mod list;
pub mod bytes;
pub mod net;
pub mod zlib;
pub mod fs;
// ...
```

Each generated crate does `use almide_runtime::net::*;` only for the modules it needs. No text search, no inclusion heuristics.

For WASM: the runtime stays inline (compiled into the single .wasm).

## Migration Plan

### Phase 1: Runtime crate extraction

- Move `runtime/rs/src/*.rs` into a real `almide-runtime` crate
- Generated code does `use almide_runtime::*;`
- Still single-file output, but runtime is external
- This alone fixes: `use` deduplication, type alias shadowing

### Phase 2: Dependency crates

- Each dependency package → generated lib crate
- Workspace Cargo.toml with dependency graph
- Eliminates: prefix naming, top_let prefix, transitive dep resolution

### Phase 3: Prefix removal

- Remove `almide_rt_{mod}_{name}` from codegen
- Function names become `pub fn {name}()`
- StdlibLowering simplified

### Phase 4: IR linker for WASM

- `ir_link()` function that merges IrPrograms
- `emit_wasm` takes merged IR
- Formalize what `program.modules` does today

## What Stays The Same

- Parser, checker, lowering — unchanged
- IR structure — unchanged
- Most codegen passes — unchanged
- WASM emit_wasm — unchanged (takes merged IR)
- `almide run` / `almide test` UX — unchanged (build system is internal)

## Exit Criteria

- `mc-bot-cli → mc-bot → mc_protocol` 3-level dep chain builds without any of today's workarounds
- No `almide_rt_` prefix in generated Rust code (crate boundaries handle namespacing)
- Runtime inclusion is a Cargo.toml dependency, not text search
- 235+ tests pass
