<!-- description: Fix ProcessStatus struct missing in cross-crate codegen — blocks almai dependency usage -->
<!-- done: 2026-05-25 -->
# Cross-crate stdlib struct injection — DONE

## Problem

When crate A depends on crate B, and crate B uses `process.exec_status`, the generated Rust code references `ProcessStatus` but the struct definition was never emitted. Single-crate compilation worked fine.

## Root Cause

The `FileStat` struct had an injection guard in `lib.rs`, but `ProcessStatus` did not. In single-crate, the walker emits the struct from `type ProcessStatus = { ... }` in `stdlib/process.almd`. In cross-crate, the walker doesn't re-emit dependency types — so the struct was missing.

## Fix (v2 — recreated)

Initial fix used per-type injection guards (whitelist). Recreated to eliminate the whitelist:

1. **Struct definitions live in runtime source** (`runtime/rs/src/process.rs`, `fs.rs`) — single source of truth
2. **Runtime concatenation auto-dedup**: when concatenating runtime module source into output, detect `#[derive(...)]\npub struct X { ... }` blocks. If `user_code` (walker output) already contains `struct X`, skip the block.
3. **Zero injection logic** — no hardcoded struct strings, no per-type guards

**How it works in each scenario:**
- **Single-crate**: walker emits struct from `stdlib/*.almd` type declaration → runtime dedup skips the runtime copy → 1 definition
- **Cross-crate**: walker doesn't re-emit dependency types → runtime copy is NOT skipped → 1 definition

**Design principle**: two categories of runtime structs:
- **User-visible record types** (`ProcessStatus`, `FileStat`): defined in both `stdlib/*.almd` (for type checking) and `runtime/rs/src/*.rs` (for cross-crate). Dedup prevents E0428.
- **Opaque types** (`AlmideHttpResponse`, `AlmideHttpRequest`): defined only in `runtime/rs/src/*.rs`, travel with runtime module. No dedup needed.

## Files changed
- `crates/almide-codegen/src/lib.rs` — injection guards removed, dedup logic in runtime concatenation
- `runtime/rs/src/process.rs` — ProcessStatus struct definition added
- `runtime/rs/src/fs.rs` — FileStat struct definition added

## Verification
- Single-crate with type use: exactly 1 struct definition (walker provides, runtime skipped)
- Single-crate without type use: exactly 1 struct definition (runtime provides)
- All 239 tests pass
- New stdlib record types automatically handled — no codegen changes needed
