<!-- description: CSE and inlining passes for cross-target IR optimization -->
# IR Optimization Tier 2

**Priority:** Medium — Optimizations automatically applied to all targets
**Prerequisites:** Tier 1 (constant folding, DCE) complete, nanopass pipeline established
**Goal:** Expand IR-level optimization passes to improve generated code quality without relying on rustc/V8

> "If the IR is smart, all targets become smart."

---

## Why

Tier 1 (constant folding, DCE) is complete. However, there are optimizations that still need to be solved at the IR level:

- Redundant computation of identical subexpressions
- Small functions used only once still carry call overhead
- Heap allocation for objects that don't escape
- High-cost operations that can be replaced with low-cost alternatives

Solving these at the IR level benefits all targets: Rust/TS/WASM. WASM especially, since it doesn't go through rustc optimization — IR optimization effects are direct.

---

## Passes

### ✅ Completed

| Pass | File | Summary |
|------|------|---------|
| LICM | `pass_licm.rs` | Loop-invariant code motion. effect fn calls excluded |
| StreamFusion | `pass_stream_fusion.rs` | Fusion of map/filter/fold chains. Intermediate list elimination |
| Auto Parallel | `pass_auto_parallel.rs` | Automatic parallelization of pure lambdas (Rust target) |

### Not Yet Implemented

### Pass 1: Common Subexpression Elimination (CSE)

Consolidate redundant computations of identical expressions into let bindings.

```almd
// Before
let a = list.len() * 2
let b = list.len() * 3

// After (IR level)
let __cse_0 = list.len()
let a = __cse_0 * 2
let b = __cse_0 * 3
```

**Criteria:** Expression is pure (no effects) and structurally identical.

### Pass 2: Simple Inlining

Inline functions with use-count 1 and single-expression body.

```almd
fn double(x: Int) -> Int { x * 2 }
let y = double(5)
// → let y = 5 * 2 → 10 (chains with constant folding)
```

**Constraints:**
- Body is a single expression only
- Not recursive
- Use-count ≤ threshold (initial value: 1)

### Pass 3: Strength Reduction (Future)

Replace high-cost operations with low-cost alternatives.

```
x * 2      → x << 1      (integer)
x / 4      → x >> 2      (positive integer)
x % 2 == 0 → x & 1 == 0  (integer)
```

---

## Architecture

```
Lower → IR
         │
         ▼
    ┌─────────────────────────┐
    │ Tier 1 (done)             │
    │  ├── ConstFoldPass       │
    │  └── DCEPass             │
    ├─────────────────────────┤
    │ Tier 1.5 (done)          │
    │  ├── LICMPass            │
    │  ├── StreamFusionPass    │
    │  └── AutoParallelPass    │
    ├─────────────────────────┤
    │ Tier 2 (this roadmap)    │
    │  ├── CSEPass             │
    │  └── InlinePass          │
    ├─────────────────────────┤
    │ Tier 3 (future)          │
    │  └── StrengthReduction   │
    └─────────────────────────┘
         │
         ▼
    mono() → nanopass → codegen
```

All passes implement the `NanoPass` trait and are inserted into the existing pipeline.

---

## Phases

### Phase 1: CSE

- [ ] Implement `CSEPass` (structural equality check + binding insertion)
- [ ] Purity check (expressions containing effect fn calls are excluded)
- [ ] Tests: deduplication of stdlib calls

### Phase 2: Inlining

- [ ] Implement `InlinePass` (use-count + body size check)
- [ ] Chaining test with constant folding
- [ ] Monitor code size bloat (WASM binary size regression test)

---

## Success Criteria

- CSE and Inline passes integrated into the nanopass pipeline
- WASM binary size does not regress (balanced with DCE)
- All existing tests continue to pass
- Measurable improvement on benchmarks (Mandelbrot, etc.)
