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

## Fix

### 1. Add struct injection in codegen (after line 222)

```rust
if needed.contains("process") && !user_code.contains("struct ProcessStatus") {
    output.push_str("#[derive(Clone, Debug, PartialEq)]\npub struct ProcessStatus {\n    pub code: i64,\n    pub stdout: String,\n    pub stderr: String,\n}\n\n");
}
```

### 2. Add struct definition in runtime source

`runtime/rs/src/process.rs` — currently uses `ProcessStatus` but never defines it:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessStatus {
    pub code: i64,
    pub stdout: String,
    pub stderr: String,
}
```

### 3. Audit for other missing structs

Grep `runtime/rs/src/` for struct usages that lack definitions — same pattern may exist elsewhere.

## Exit criteria

- `almide test` passes for any crate that depends on a crate using `process` module
- Specifically: `import almai` + `almai.user("hello")` compiles and runs
