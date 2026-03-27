<!-- description: Rewrite TS codegen to two-stage pipeline (IR to TsIR to String) -->
<!-- done: 2026-03-15 -->
# TS/JS Codegen Rewrite

## Summary
Rewrite `src/emit_ts/`. Unify with the same two-stage pipeline as Rust codegen (IR → TsIR → String).

## Current State
- Still using the old code (4 files)
- Rust codegen has migrated to the RustIR pipeline, but TS has not
- Since stdlib is empty, the TS runtime (`emit_ts_runtime.rs`) can also be significantly simplified

## Goal
- Two-stage pipeline: IR → TsIR → String
- Each file < 500 lines
- Same design principles as Rust codegen:
  - All decisions made during lowering
  - Rendering is pure pattern matching

## Design
```
emit_ts/
  ts_ir.rs      — TsIR data types
  lower_ts.rs   — IR → TsIR (Result erasure, ok→value, err→throw)
  render_ts.rs  — TsIR → TypeScript/JavaScript source
  mod.rs        — Entry point
```

## Files
```
src/emit_ts/ts_ir.rs (new)
src/emit_ts/lower_ts.rs (new)
src/emit_ts/render_ts.rs (new)
src/emit_ts/mod.rs (rewrite)
src/emit_ts_runtime.rs (simplify)
```
