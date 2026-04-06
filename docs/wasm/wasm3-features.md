# WASM 3.0 Features

Almide emits WASM 3.0 binaries. No 2.0 fallback.

## Tail Calls

All tail-position calls emit `return_call` / `return_call_indirect`.

**Not just self-recursion.** Any call in tail position — mutual recursion, higher-order calls, cross-function tail calls — all use native WASM tail calls. Loop-based TCO (`TailCallOptPass`) is removed from the WASM pipeline.

### Implementation

- `TailCallMarkPass` (nanopass): walks function bodies, converts `Call` → `TailCall` at tail positions
- Tail positions: direct return, last expr of Block, both If branches, all Match arm bodies
- `TailCall` IR node: same as `Call` but emitter outputs `return_call`
- Pipeline position: **last pass** (after all other passes that may create calls)

### What IS a tail position

```
fn foo(n) =
  if n == 0 then bar()      // ✅ tail — If branch
  else baz(n - 1)            // ✅ tail — If branch

fn qux(x) = {
  let y = compute(x)         // ❌ not tail — Block statement
  transform(y)               // ✅ tail — Block tail expr
}

fn corge(x) = match x {
  0 => alpha()               // ✅ tail — Match arm body
  _ => beta(x)               // ✅ tail — Match arm body
}
```

### What is NOT a tail position

- `ResultOk { Call(...) }` — the Ok wrapper changes the return type
- `Try { Call(...) }` — Try unwraps the Result, type mismatch with return_call
- Call inside a statement (not the tail expression)
- Call whose result is used by another expression

### Runtime support

Universal: Chrome 112, Firefox 121, Safari 18.2, Wasmtime 22, Wasmer 7.1

## Multi-Memory

Two memories in every WASM binary.

| Memory | Size | Purpose |
|--------|------|---------|
| 0 (main) | 64 pages (4MB), growable | Scratch + data segment + heap |
| 1 (scratch) | 1 page (64KB), growable | String builder temporary buffer |

### String Interpolation Optimization

Before (single memory):
```
"${a} is ${b}" → concat(a, concat(" is ", b))
                  ↳ 2 intermediate heap allocations
```

After (multi-memory):
```
"${a} is ${b}" → scratch_write(a) → scratch_write(" is ") → scratch_write(b)
                  → scratch_finalize() → 1 final heap allocation + memory.copy(1→0)
```

N-part interpolation: N-1 intermediate allocations → 0.

### Macro Infrastructure

All load/store macros support optional `memory_index`:
```rust
wasm!(f, { i32_store(0); });        // memory 0 (default)
wasm!(f, { i32_store8(0, 1); });    // memory 1 (explicit)
wasm!(f, { memory_copy(1, 0); });   // src=mem1, dst=mem0
wasm!(f, { memory_grow(1); });      // grow memory 1
```

### Runtime support

Chrome 120, Firefox 125, Wasmtime 15 (default ON), WasmEdge (default ON). Safari missing — not a concern for server-side containers.

## Exception Handling (deferred)

`try_table` / `throw` / `exnref` for zero-cost effect fn error propagation. wasm-encoder 0.225 supports it. Blocked on Wasmtime default-OFF. See [on-hold/wasm-exception-handling.md](../roadmap/on-hold/wasm-exception-handling.md).
