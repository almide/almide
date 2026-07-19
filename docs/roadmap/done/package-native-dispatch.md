<!-- description: First-class @inline_rust for packages, remove bundled-only walls -->
<!-- done: 2026-05-22 -->
# Package Native Dispatch

> **Shipped: v0.22.1**
> **Status: Done**

## Problem

`@inline_rust` / `@intrinsic` / borrow inference only work for bundled stdlib modules. Packages are second-class citizens — they can't use hardware-accelerated native code on Rust target.

This blocks Almide crypto packages (sha1, aes) from matching Rust crate performance (~700x gap on SHA-1, ~4500x on AES-CFB8). Every other language solves this with FFI/C bindings. Almide already has the infrastructure (`[native-deps]`, `native/*.rs` injection) but the codegen pipeline refuses to use it for non-bundled modules.

## Root Cause

`is_bundled_module()` checks gate every codegen feature:

| File | Gate | Effect |
|---|---|---|
| `pass_resolve_calls.rs:87` | `is_any_stdlib(m)` | Only stdlib Module calls get rewritten |
| `pass_stdlib_lowering.rs:248` | `is_bundled_module(m)` | `@inline_rust` table only scans bundled modules |
| `pass_borrow_inference.rs:479` | `is_bundled_module(m)` | Borrow inference skips package calls |
| `walker/mod.rs:145` | attrs check | `@inline_rust` fns skip body emission (even if fallback exists) |

## Dependency Call Lifecycle

When `bench.almd` imports `sha1` package:

```
Parser:     sha1.hash(data)
Checker:    CallTarget::Module { module: "sha1", func: "hash" }
Lowering:   same (dependency modules preserved)
ResolveCalls: skipped (is_any_stdlib("sha1") = false)
StdlibLowering: @inline_rust table doesn't include sha1
IrLinkFlatten: func moved to program.functions, name stays "hash", module_origin = "sha1_v0"
Walker:     emits almide_rt_sha1_v0_hash(&data) — calls the pure fallback body
```

The `@inline_rust` template is never applied because:
1. `StdlibLowering` only builds the inline_rust table from bundled modules
2. By the time `IrLinkFlatten` runs, `StdlibLowering` has already finished

## Solution

### Phase 1: Unified `@inline_rust` resolution

Remove `is_bundled_module` filter from `StdlibLoweringPass` inline_rust table construction. Scan ALL `program.modules` for `@inline_rust` attributes.

For dependency calls that arrive as `CallTarget::Named` (versioned), add reverse-lookup: `almide_rt_sha1_v0_hash` → `(sha1, hash)` → inline_rust table.

### Phase 2: Fallback body emission

`@inline_rust` with a non-Hole body = "use native on Rust, fallback on WASM". Walker must:
- Rust target: emit the `@inline_rust` template at call sites, AND emit the fallback body as a function (for same-module calls like tests)
- WASM target: ignore `@inline_rust`, emit the fallback body

### Phase 3: Capability unification

Replace scattered `is_bundled_module` checks with a single `ModuleCapabilities` table:

```rust
struct ModuleCapabilities {
    has_inline_rust: bool,    // @inline_rust templates available
    has_borrow_sigs: bool,    // borrow inference applies
    has_runtime_fns: bool,    // almide_rt_* symbols exist
}
```

Built once from `program.modules` (both bundled and package). All passes query this table instead of hardcoded `is_bundled_module`.

### Phase 4: `[native-deps]` TOML table support

Support Cargo-style rename syntax:
```toml
[native-deps]
sha1_crate = { package = "sha1", version = "0.10" }
```

Requires extending `parse_kv` in `project.rs` to handle TOML inline tables.

### Phase 5: `effect fn` + explicit `Result` warning

Emit a warning when `effect fn` declares `-> Result[T, E]`:
```
warning: effect fn already wraps return type in Result — use `-> T` instead
```

## Package API Design (Target State)

```almide
// sha1/src/mod.almd
import bytes

@inline_rust("sha1_native::native_sha1_hash(&{message})")
fn hash(message: Bytes) -> Bytes = hash_pure(message)
//                                  ↑ WASM fallback, Rust test fallback
```

```toml
# sha1/almide.toml
[native-deps]
sha1_crate = { package = "sha1", version = "0.10" }
```

```rust
// sha1/native/sha1_native.rs
pub fn native_sha1_hash(data: &Vec<u8>) -> Vec<u8> {
    use sha1::{Sha1, Digest};
    let mut h = Sha1::new();
    h.update(data);
    h.finalize().to_vec()
}
```

## Expected Performance

| Benchmark | Pure Almide | With native dispatch | Rust crate direct |
|---|---|---|---|
| SHA-1 x10000 | 860ms | ~1.2ms | 1.2ms |
| AES-CFB8 4096B | 300ms | ~67μs | 67μs |

Package native dispatch closes the gap to zero overhead vs Rust.
