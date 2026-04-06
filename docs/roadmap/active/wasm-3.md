<!-- description: WebAssembly 3.0 target: tail calls, multi-memory, exception handling (v2), Component Model async -->
# WebAssembly 3.0 Target

Almide targets WebAssembly 3.0 — the first language to fully leverage the new spec for linear-memory compilation.

## Why

- **Tail calls** eliminate stack overflow in recursive code — critical for a language that prefers recursion over mutation
- **Multi-memory** isolates heap regions for safety and performance
- **Component Model async** maps `fan` to `future<T>` / `stream<T>` — cooperative concurrency without threads
- **No GC dependency** — Almide compiles to linear memory with deterministic allocation, running on any 3.0 runtime including lightweight embeddings where GC proposals aren't available

## v1: Tail Calls + Multi-Memory (v0.12) ✅

### Tail Calls — Done

`return_call` / `return_call_indirect` for all tail-position calls.

- `TailCallMarkPass` marks tail-position calls in the IR
- WASM emitter emits `return_call` instead of `call` for marked nodes
- Applies to ALL tail calls (not just self-recursive) — mutual recursion included
- Replaces loop-based `TailCallOptPass` in WASM pipeline
- **Runtime support**: Chrome 112, Firefox 121, Safari 18.2, Wasmtime 22, Wasmer 7.1 — universal

### Multi-Memory — Done

Memory 0 (main) + Memory 1 (string builder scratch).

- Memory 0: 64 pages (4MB) — scratch + data segment + heap (unchanged)
- Memory 1: 1 page (64KB) — string builder scratch, grows on demand
- All load/store macros support `memory_index` parameter (default 0, backward compatible)
- `memory_copy`, `memory_fill`, `memory_size`, `memory_grow` parameterized per memory
- **Runtime support**: Chrome 120, Firefox 125, Wasmtime 15 (default ON), WasmEdge (default ON)

## v2: Exception Handling (deferred)

`try_table` / `throw` / `throw_ref` with first-class `exnref`.

- effect fn `?` propagation → WASM native exception flow instead of Result wrapping + branch
- Eliminates Result heap allocation and per-`?` branch overhead
- **Blocked on**: Wasmtime default OFF, WasmEdge requires `--enable-exception-handling`. All major WASI container runtimes (Spin, wasmCloud, Docker+WASM) require runtime config changes
- **wasm-encoder 0.225**: Already supports TryTable, Throw, ThrowRef, exnref — ready when Wasmtime flips the default
- **Trigger**: Implement when Wasmtime enables EH by default

## v3: Fan → Component Model Async (WASI 0.3)

`fan` compiles to Component Model async primitives, not wasm threads.

```
fan { fetch(url1), fetch(url2), fetch(url3) }
  → 3x future<T> + waitable-set multiplex
```

- **Why not threads**: WASI container runtimes are single-threaded by design. `wasi-threads` was withdrawn in 2023. `shared-everything-threads` is still draft with zero implementations
- **Why Component Model async**: WASI 0.3 adds `future<T>` and `stream<T>` as WIT-level types. Host runtime drives the executor. Wasmtime 37+ and Spin v3.5+ ship it
- **Almide advantage**: `fan` maps directly to async ABI without developer ceremony
- **Browser target**: fan compiles to SharedArrayBuffer + Web Workers for true parallelism
- **Runtime support**: Wasmtime 37+ (WASI 0.3 RC), Spin v3.5+, wasmCloud (tracking)

## No 2.0 Fallback

WASM output is 3.0 only. Tail calls and multi-memory are default-on in every major runtime.

## Messaging

> Almide is the first language designed for WebAssembly 3.0 — stack-safe recursion, isolated memory regions, and native async concurrency, all without GC overhead.
