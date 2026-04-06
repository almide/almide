<!-- description: WASM native exception handling (try_table/throw) for zero-cost effect fn error propagation -->
# WASM Exception Handling

## What

Replace Result heap allocation + per-`?` branch with WASM native `try_table` / `throw` / `exnref`.

- effect fn returns value directly; errors propagate via `throw`
- Eliminates Result heap allocation and tag-check branching
- wasm-encoder 0.225 already supports all EH instructions

## Blocked On

Wasmtime has exception handling **default OFF**. All major WASI container runtimes (Spin, wasmCloud, Docker+WASM) inherit this default. Implementing EH now would require dual codegen (EH native + Result chain fallback), which is high cost for limited reach.

## Trigger

Implement when Wasmtime enables EH by default. Track: https://github.com/bytecodealliance/wasmtime/issues/3427
