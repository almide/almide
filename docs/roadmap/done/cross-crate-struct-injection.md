<!-- description: Fix ProcessStatus struct missing in cross-crate codegen — blocks almai dependency usage -->
# Cross-crate stdlib struct injection — DONE

## Problem

When crate A depends on crate B, and crate B uses `process.exec_status`, the generated Rust code references `ProcessStatus` but the struct definition was never emitted. Single-crate compilation worked fine.

## Root Cause

The `FileStat` struct had an injection guard in `lib.rs`, but `ProcessStatus` did not. In single-crate, the walker emits the struct from `type ProcessStatus = { ... }` in `stdlib/process.almd`. In cross-crate, the walker doesn't re-emit dependency types — so the struct was missing.

## Fix

Added the same injection guard pattern used by `FileStat`:

```rust
if needed.contains("process") && !user_code.contains("struct ProcessStatus") {
    output.push_str("...struct ProcessStatus...");
}
```

**Design principle**: two categories of runtime structs:
- **User-visible record types** (`ProcessStatus`, `FileStat`): defined by Almide type system (`stdlib/*.almd`), walker emits struct. Injection guard handles cross-crate fallback.
- **Opaque types** (`AlmideHttpResponse`, `AlmideHttpRequest`): defined in `runtime/rs/src/*.rs`, travel with runtime module. No injection needed.

## Files changed
- `crates/almide-codegen/src/lib.rs` — ProcessStatus injection guard
- `runtime/rs/src/process.rs` — comment documenting struct source

## Verification
- Single-crate: exactly 1 `struct ProcessStatus` in output
- All 239 tests pass
