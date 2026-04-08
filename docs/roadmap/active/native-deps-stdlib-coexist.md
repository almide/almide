<!-- description: Fix native-deps and stdlib HTTP coexistence in generated Cargo.toml -->

# Native Deps + Stdlib HTTP Coexistence

## Problem

When a project uses both `[native-deps]` (e.g., wasmtime) and `import http` (stdlib), the generated Cargo.toml has conflicting reqwest versions or missing crate imports. The compiler generates `use std::io::Write` twice, and native-deps crates become unresolved.

## Reproduction

```toml
# almide.toml
[native-deps]
wasmtime = "42.0.1"
wasmtime-wasi = "42.0.1"
reqwest = { version = "0.12", features = ["blocking", "rustls-tls"] }
serde_json = "1"

[permissions]
allow = ["FS.read", "FS.write", "IO", "Env", "Time", "Net"]
```

```almide
import http  # stdlib HTTP module

@extern(rs, "wasmtime_bridge", "wt_create")
fn wt_create(wasm_path: String, fuel: Int) -> Int
```

Results in:
- `error[E0252]: the name 'Write' is defined multiple times`
- `error[E0432]: unresolved import 'wasmtime'`

## Expected Behavior

Both native-deps crates and stdlib HTTP should coexist in the same project. The generated Cargo.toml and main.rs should handle both import paths without conflicts.

## Impact

Blocks porta's migration from Rust bridge functions to pure Almide (full-almide roadmap item).
