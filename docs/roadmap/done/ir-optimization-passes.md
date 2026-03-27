<!-- description: IR-to-IR transform passes applied before codegen for all targets -->
<!-- done: 2026-03-15 -->
# IR Optimization Passes

IR-to-IR transform passes inserted before codegen to improve generated code quality.

## Why

With IR redesign (Phase 1-5), codegen takes only `&IrProgram` as input. This means transforming the IR before passing it to codegen automatically applies optimizations to all targets (Rust/TS/JS). Currently only borrow analysis and use-count exist.

## Passes

### Tier 1: Low-hanging fruit ✅

| Pass | Effect | Status |
|------|--------|--------|
| **Constant folding** | `1 + 2` → `LitInt(3)`, `"a" ++ "b"` → `LitStr("ab")` | ✅ |
| **Dead code elimination** | Remove unreachable branches, unused let bindings | ✅ |
| **Constant propagation** | `let x = 5; x + 1` → `5 + 1` → `6` | future |

### Tier 2: Medium complexity

| Pass | Effect |
|------|--------|
| **Inlining** | Inline expansion of small functions (body is a single expression) |
| **Common subexpression elimination** | Consolidate duplicate computations of identical expressions into let bindings |
| **Loop-invariant code motion** | Move invariant expressions inside for/while loops to outside the loop |

### Tier 3: Advanced

| Pass | Effect |
|------|--------|
| **Tail call optimization** | Self-recursive tail call → labeled loop (can merge with existing roadmap) |
| **Escape analysis** | Avoid heap allocation (evolution of borrow analysis) |
| **Specialization** | Function specialization based on type arguments (precursor to monomorphization) |

## Architecture

```
Lowering → IrProgram
              │
              ▼
         ┌─────────────────┐
         │  Optimization    │   IR → IR transforms (pipeline of passes)
         │  ├── const_fold  │
         │  ├── dce         │
         │  └── inline      │
         └─────────────────┘
              │
              ▼
         Codegen (Rust / TS / JS)
```

Each pass has `fn(ir: &mut IrProgram)` signature. Pass application order is fixed. Controlled by `--opt-level` flag.

## Unlocked by

IR Redesign Phase 5 complete. Since codegen only references the IR, IR transforms are directly reflected in codegen output.

## Affected files

| File | Change |
|------|--------|
| `src/opt/` (new) | Optimization pass module |
| `src/main.rs` | Insert passes into pipeline |
| `src/cli.rs` | `--opt-level` flag |
