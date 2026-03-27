<!-- description: Multi-layer platform vision with pluggable renderer and host bindings -->
# Almide Platform Architecture Vision

**Priority:** post-1.0 (2.x)
**Research:** Lessons from Flutter/RN New Architecture

## Vision

Design Almide as a general-purpose language that **can also serve as** an app runtime.
Maintain CLI/server/scripting practicality while adding an app runtime layer on top.
Language first, platform second. The same trajectory as Kotlin (JVM → Android → Multiplatform → Server).

```
[Almide Language + DSL]
        ↓
[Typed IR / Nanopass Compiler]       ← complete
        ↓
[Reactive Runtime + Effects]         ← effect fn + fan (foundation exists)
        ↓
[Domain Core / Sync / Persistence]   ← not started
        ↓
[Host Bindings via IDL + Codegen]    ← stdlib TOML (prototype exists)
        ↓
[Pluggable Renderer]                 ← not started
        ↓
[iOS / Android / Web / Desktop]
```

## 5-Layer Architecture

### Layer 1: Language Kernel ✅
- Memory management (borrow/clone analysis)
- Concurrent execution (fan)
- Effect isolation (effect fn)
- Capability permission (effect isolation Layer 1)

### Layer 2: Typed Host Boundary (1.x)
- Current: stdlib TOML + build.rs codegen
- Goal: IDL/schema-first, codegen-first host bindings
- Zero-copy boundary, explicit sync/async

### Layer 3: Core Domain Runtime (2.x)
- State machine
- Offline-first data graph
- Sync engine (CRDT/merge)
- Cache/persistence abstraction

### Layer 4: Pluggable Renderer (2.x)
- Native widget renderer (OS-native)
- Custom scene renderer (Flutter-style consistency)
- Hybrid renderer (per-screen switching)

### Layer 5: Evolution Layer (1.x-)
- Edition system ✅
- ABI versioning
- Module-level migration
- Schema versioning

## Advantages Almide Already Has

| Element | Status |
|---|---|
| Typed IR + Nanopass compiler | ✅ Complete |
| Multi-target codegen (Rust/TS) | ✅ Complete |
| Effect isolation | ✅ Layer 1 |
| Fan concurrency | ✅ thread + Promise |
| Package management | ✅ almide.lock |
| Edition system | ✅ 2026 |
| Template-driven target extension | ✅ TOML + pass |

## Next Steps (Can Start in 1.x)

1. **Capability declarations** — Extend effect fn permission model
2. **IDL for host bindings** — Evolve stdlib TOML → general-purpose IDL
3. **Hot module replacement** — Signed + ABI checked module swap
4. **Devtools** — Reactive graph inspector, host call profiler

## Design Principles

1. **UI is not the core** — Execution model + typed boundary + data sync are the core
2. **Declarative FFI boundary, not bridges** — schema-first / codegen-first
3. **Separate rendering from host integration** — Renderer and Host API are different layers
4. **"OS for apps", not a framework** — runtime + package + permission + diagnostics + devtools
