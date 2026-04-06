<!-- description: WebAssembly 3.0 target: tail calls, exception handling, multi-memory, threads -->
# WebAssembly 3.0 Target

Almide targets WebAssembly 3.0 — the first language to fully leverage the new spec for linear-memory compilation.

## Why

- **Tail calls** eliminate stack overflow in recursive code — critical for a language that prefers recursion over mutation
- **Native exception handling** makes effect fn error propagation zero-cost instead of manual Result chaining
- **Multi-memory** isolates heap regions (strings, lists, closures) for safety and performance
- **No GC dependency** — Almide compiles to linear memory with deterministic allocation, running on any 3.0 runtime including lightweight embeddings where GC proposals aren't available

## Implementation (all v0.12)

### Tail Calls

Highest ROI. `return_call` / `return_call_indirect` for proper TCO in WASM.

- Extend existing `pass_tco.rs` (currently Rust-only) to emit WASM tail call instructions
- All recursive stdlib patterns (scan_while, skip_ws, etc.) become stack-safe
- **Runtime support**: Chrome 112, Firefox 121, Safari 18.2, Wasmtime 22, Wasmer 7.1 — universal

### Multi-Memory

Separate memories for different allocation pools.

- Memory 0: general heap (records, variants)
- Memory 1: string pool (immutable, compactable)
- Memory 2: closure environments
- Reduces fragmentation, enables per-pool growth strategies
- **Runtime support**: Chrome 120, Firefox 125, Wasmtime 15 (default ON), WasmEdge (default ON)

### Exception Handling

`try_table` / `throw` / `throw_ref` with first-class `exnref`.

- effect fn `?` propagation → WASM native exception flow instead of Result wrapping + branch
- Significant binary size reduction (no Result construction/destruction overhead per call)
- **Runtime support**: Chrome 137, Firefox 131, Safari 18.4 — all browsers ship it
- **Container gap**: Wasmtime default OFF, WasmEdge requires `--enable-exception-handling`. This means Spin, wasmCloud, Docker+WASM, and K8s shims all require runtime config to enable it
- **Strategy**: Emit EH instructions, but also keep the current Result-chain codegen as fallback. Select at build time: `--wasm-eh=native` (default for browser) / `--wasm-eh=result` (default for WASI CLI). When Wasmtime flips the default (tracked), remove the fallback

### Threads — Deferred

Shared memory + atomics for fan expression compilation.

- **Container ecosystem is single-threaded by design.** Spin, wasmCloud, Fastly, Cloudflare all disable shared memory or run one thread per request. Wasmtime has the proposal ON but disables shared memory creation by default
- **Browser support exists** (Chrome 74, Firefox 79, Safari 14.1) but fan concurrency for browser is a separate concern
- **Decision**: Do not block v0.12 on threads. Revisit when WASI 0.3 async lands or container runtimes adopt shared-everything threads

## No 2.0 Fallback

WASM output is 3.0 only. No `--compat 2.0` mode. Tail calls and multi-memory are default-on in every major runtime. Exception handling has a build-time flag for runtimes that haven't enabled it yet (Wasmtime). Threads are deferred entirely.

## Messaging

> Almide is the first language designed for WebAssembly 3.0 — zero-cost error handling, stack-safe recursion, and isolated memory regions, all without GC overhead.
