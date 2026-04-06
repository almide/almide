<!-- description: WebAssembly 3.0 target: tail calls, exception handling, multi-memory, threads -->
# WebAssembly 3.0 Target

Almide targets WebAssembly 3.0 — the first language to fully leverage the new spec for linear-memory compilation.

## Why

- **Tail calls** eliminate stack overflow in recursive code — critical for a language that prefers recursion over mutation
- **Native exception handling** makes effect fn error propagation zero-cost instead of manual Result chaining
- **Multi-memory** isolates heap regions (strings, lists, closures) for safety and performance
- **No GC dependency** — Almide compiles to linear memory with deterministic allocation, running on any 3.0 runtime including lightweight embeddings where GC proposals aren't available

## Phases

### Phase 1: Tail Calls (v0.12)

Highest ROI. `return_call` / `return_call_indirect` for proper TCO in WASM.

- Extend existing `pass_tco.rs` (currently Rust-only) to emit WASM tail call instructions
- All recursive stdlib patterns (scan_while, skip_ws, etc.) become stack-safe
- **Runtime support**: Chrome 112, Firefox 121, Safari 18.2, Wasmtime 22, Wasmer 7.1 — universal

### Phase 2: Exception Handling (v0.13)

`try_table` / `throw` / `throw_ref` with first-class `exnref`.

- effect fn `?` propagation → WASM native exception flow instead of Result wrapping + branch
- Significant binary size reduction (no Result construction/destruction overhead per call)
- **Runtime support**: Chrome 137, Firefox 131, Safari 18.4, Wasmtime (flag), Wasmer 6.0
- Note: Wasmtime still behind flag — CLI target may need fallback path or wait for default-on

### Phase 3: Multi-Memory (v0.14)

Separate memories for different allocation pools.

- Memory 0: general heap (records, variants)
- Memory 1: string pool (immutable, compactable)
- Memory 2: closure environments
- Reduces fragmentation, enables per-pool growth strategies
- **Runtime support**: Chrome 120, Firefox 125, Wasmtime 15 — Safari missing, so emit as opt-in feature
- Emit flag: `--wasm-features multi-memory`

### Phase 4: Threads (with fan concurrency)

Shared memory + atomics for fan expression compilation.

- `fan { a, b, c }` → parallel WASM threads with shared linear memory
- Requires SharedArrayBuffer (browser) or WASI threads (CLI)
- **Runtime support**: Chrome 74, Firefox 79, Safari 14.1, Wasmtime 15, Wasmer 4.0 — universal
- Blocked on fan concurrency language design (separate roadmap item)

## Baseline Compatibility

Default output (`almide build --target wasm`) remains **WASM 2.0 compatible**. New features are enabled via:

```
almide build app.almd --target wasm                           # 2.0 baseline
almide build app.almd --target wasm --wasm-features tail-call # + TCO
almide build app.almd --target wasm --wasm-features 3.0       # all available 3.0 features
```

## Messaging

> Almide is the first language designed for WebAssembly 3.0 — zero-cost error handling, stack-safe recursion, and isolated memory regions, all without GC overhead.
