<!-- description: Fix ProcessStatus struct missing in cross-crate codegen — blocks almai dependency usage -->
# Cross-crate stdlib struct injection

> **Priority: high** — blocks `almide-ai/almai` from being used as a dependency.
> **Scope**: `crates/almide-codegen/src/lib.rs` + `runtime/rs/src/process.rs`

## Problem

When crate A depends on crate B, and crate B uses `process.exec_status`, the generated Rust code references `ProcessStatus` but the struct definition is never emitted. Single-crate compilation works fine.

```
error[E0425]: cannot find type `ProcessStatus` in this scope
  --> <generated.rs>:1405:79
```

## Reproduction

```toml
[dependencies]
almai = { git = "https://github.com/almide-ai/almai" }
```

```almide
import almai

test "simple" {
  let msg = almai.user("hello")
  assert_eq(msg.role, "user")
}
```

`almide test` → codegen error: `ProcessStatus` not found.

`almai` itself passes all 30 tests when compiled standalone.

## Root Cause

`crates/almide-codegen/src/lib.rs` lines 215-222:

```rust
// FileStat is handled:
if needed.contains("fs") && !user_code.contains("struct FileStat") {
    output.push_str("...struct FileStat...");
}
// ProcessStatus: NO EQUIVALENT — this is the bug
```

The codegen emits the entire `process` runtime module when `process` is in the needed set. The runtime includes `exec_status()` which returns `ProcessStatus`, but the struct definition is never injected.

## Current Status (in-progress)

### What was tried

1. **Added struct to runtime source** (`runtime/rs/src/process.rs`): ProcessStatus struct definition added. build.rs regenerates `generated/rust_runtime.rs` with the struct.

2. **Added injection guard**: `runtime_has()` closure to check if runtime source already contains the struct → skip injection. However, the guard doesn't prevent double definition.

3. **The real issue**: runtime source is embedded via build.rs and ALREADY includes the struct. The injection (`output.push_str(...)`) runs BEFORE runtime modules are concatenated, but `runtime_has()` should detect the embedded definition. Despite this, the struct appears twice in output.

4. **Debug findings**: `strings` on the compiled binary shows 3 occurrences of "struct ProcessStatus". The `runtime_has` closure's filter condition may not match due to `needed` HashSet lifetime/type issues.

### Files modified (uncommitted)
- `crates/almide-codegen/src/lib.rs` — injection logic (partial, needs fix)
- `runtime/rs/src/process.rs` — struct definition added

### What needs to happen
1. **Debug why `runtime_has()` returns false** when the generated source DOES contain the struct string. Add println debug to the closure.
2. **Or: move struct definitions INTO the runtime source** and remove ALL injection logic. The build.rs embedding already handles cross-crate — the struct travels with the runtime source.
3. **Audit FileStat** — is it also in the runtime source? If so, the fs injection can be removed too.
4. Same pattern for `AlmideHttpResponse` / `AlmideHttpRequest` (already in `runtime/rs/src/http.rs`).

## Exit criteria

- `almide test` passes for any crate that depends on a crate using `process` module
- Specifically: `import almai` + `almai.user("hello")` compiles and runs
- No double struct definitions in generated Rust output
