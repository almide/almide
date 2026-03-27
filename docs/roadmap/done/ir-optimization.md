<!-- description: Constant folding, dead code elimination, and basic inlining passes -->
<!-- done: 2026-03-15 -->
# IR Optimization Passes

## Summary
IR-to-IR optimization passes: constant folding, DCE, and basic inlining.

## Current State
No optimization passes on IR. Generated Rust code is optimized by rustc, but IR-level optimization can eliminate unnecessary clones and allocations.

## Design

### Pass 1: Constant Folding
```
1 + 2        → 3
"a" ++ "b"   → "ab"
not true     → false
if true then a else b → a
```

### Pass 2: Dead Code Elimination
Remove bindings with use-count of 0.
```
let x = expensive()  // x is unused
println("hello")
→
println("hello")
```

### Pass 3: Simple Inlining (future)
Inline expand small functions that are only used once.

## Implementation
- `src/optimize.rs` (new file) — `optimize_program(&mut IrProgram)`
- `src/main.rs` — insert passes after lowering, before codegen
- Each pass transforms `IrProgram` in-place
- Tests: compare output before and after optimization

## Pipeline Position
```
Lower → IR → optimize() → mono() → codegen
```

## Files
```
src/optimize.rs (new, < 500 lines)
src/main.rs
src/lib.rs
```
