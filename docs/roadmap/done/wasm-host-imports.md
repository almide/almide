<!-- description: Allow WASM target to import functions from custom host modules -->
<!-- done: 2026-04-09 -->
# Custom WASM Host Imports

## Status

完了。`@extern(wasm, "module", "func")` で任意の WASM ホストモジュールから関数をインポートできる。program-level / module-level の両方を `crates/almide-codegen/src/emit_wasm/mod.rs` でサポート。

## Original Goal

Allow Almide's WASM target to import functions from arbitrary host modules, not just `wasi_snapshot_preview1`.

## Motivation

`@extern(rs, "module", "func")` lets native targets call Rust functions. The WASM target has no equivalent — it can only call WASI functions hardcoded in the codegen.

WASM runtimes (wasmtime, wasmer, etc.) support injecting custom host functions under arbitrary module namespaces. Almide should be able to declare imports from these.

## Proposed Syntax

Reuse `@extern` with a `wasm` target specifier:

```almide
// Import from any host module
@extern(wasm, "my_runtime", "do_something")
fn do_something(ptr: Int, len: Int) -> Int
```

This emits:
```wasm
(import "my_runtime" "do_something" (func (param i32 i32) (result i32)))
```

## Examples

```almide
// A porta runtime host function
@extern(wasm, "porta", "http_request")
fn http_request(req_ptr: Int, req_len: Int, resp_ptr: Int, resp_cap: Int) -> Int

// A Fastly Compute host function
@extern(wasm, "fastly_http_req", "send")
fn fastly_send(handle: Int, body: Int) -> Int

// A custom game engine host function
@extern(wasm, "engine", "draw_sprite")
fn draw_sprite(x: Int, y: Int, id: Int) -> Int
```

## Implementation

In `emit_wasm/mod.rs`, when a function has `@extern(wasm, module, name)`:
1. Emit a WASM import with the specified module and function name
2. Map Almide types to WASM types (Int → i32/i64, Float → f64)
3. Generate a call wrapper that the rest of the Almide code can invoke

Same pattern as WASI imports, just with a configurable module name.
