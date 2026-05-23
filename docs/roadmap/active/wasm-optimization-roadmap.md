# WASM Optimization Roadmap

> Almide's WASM emitter matches Rust+LLVM on 11/11 benchmarks (v0.23.3).
> This roadmap targets **surpassing** LLVM through double-specialization:
> language semantics × WASM target knowledge.

## Status Baseline (v0.23.3)

| Benchmark       | Almide | Rust  | Ratio |
|-----------------|--------|-------|-------|
| fib35           | 35ms   | 34ms  | ≈1.0x |
| sort_100k       | 0.5ms  | 0.4ms | 1.25x |
| list_map_100k   | 0.3ms  | 0.3ms | 1.0x  |
| list_filter_100k| 0.2ms  | 0.1ms | ≈1.5x |
| list_fold_100k  | 0.0ms  | 0.0ms | ~1x   |
| map_insert_10k  | 0.5ms  | 0.4ms | 1.25x |
| map_get_10k     | 0.6ms  | 0.5ms | 1.2x  |
| int_parse_100k  | 4.9ms  | 5.9ms | 0.83x WIN |
| int_tostring    | 3.0ms  | 4.9ms | 0.63x WIN |

Optimizations already applied: hash table map, binary recursion transform,
lambda inlining, branchless filter, run-detection sort, adaptive scratch locals,
TCO in WASM pipeline.

---

## Tier 1 — Immediate (days)

### 1.1 Stream Fusion (Loop Fusion)

**Impact**: 2-5x on chained list operations
**Effort**: Medium (IR nanopass)

```almide
// Before: 2 intermediate lists, 3 loops
data |> list.map((x) => x * 2) |> list.filter((x) => x > 10) |> list.fold(0, (a, x) => a + x)

// After: 1 loop, 0 intermediate allocations
var acc = 0
for x in data:
  let mapped = x * 2
  if mapped > 10: acc += mapped
```

**Implementation**:
- New IR node: `FusedPipeline { source, stages: Vec<PipelineStage> }`
- `PipelineStage`: `Map(body)`, `Filter(body)`, `Fold(init, body)`, `Take(n)`, `TakeWhile(pred)`
- Nanopass `StreamFusionPass` detects `MethodCall(list.X, MethodCall(list.Y, ...))` chains
- WASM emitter: single loop with inlined stage bodies
- Lambda inline infrastructure already exists — reuse `emit_expr(body)` with bound params

**Prerequisite**: Lambda inlining (done)

### 1.2 Escape Analysis + Stack Allocation

**Impact**: 10x+ on int.parse, option/result-heavy code
**Effort**: Medium (IR analysis + WASM emit change)

```almide
let x = int.parse("123") ?? 0
// Currently: heap alloc Result (12 bytes), unwrap, discard
// After: WASM multi-value return (i32 tag, i64 value), zero alloc
```

**Implementation**:
- IR analysis pass: track which Result/Option values escape the current scope
- Non-escaping Results → WASM multi-value return `(i32, i64)` instead of heap ptr
- Non-escaping Options → WASM `(i32, T)` where i32=0 means none
- Requires: per-function return type specialization for runtime fns (int_parse etc.)
- Fallback: heap alloc for escaping values (unchanged)

**Key insight**: Most Result/Option usage is `let x = f() ?? default` or `match f() { ... }` — the wrapper never escapes.

### 1.3 Constant Folding Enhancement

**Impact**: Eliminates dead computation, improves JIT
**Effort**: Small (extend existing ConstFoldPass)

```almide
let x = 3 * 4 + 1       // → LitInt(13)
string.len("hello")     // → LitInt(5)
list.len([1, 2, 3])     // → LitInt(3)
int.to_string(42)       // → LitStr("42") (pure fn, known input)
```

**Implementation**:
- Extend `ConstFoldPass` to fold stdlib calls with literal args
- Whitelist of pure stdlib fns: `string.len`, `list.len`, `int.to_string`, `string.to_upper`, etc.
- Evaluate at compile time when all args are literals

### 1.4 Loop Unrolling for Small Known-Size Lists

**Impact**: 2-3x on small list operations (≤8 elements)
**Effort**: Small (WASM emitter pattern)

```almide
[1, 2, 3, 4] |> list.map((x) => x * 2)
// Currently: alloc + loop with idx check per iteration
// After: alloc + 4 inline stores (no loop, no branch)
```

**Implementation**:
- In `emit_list_map`, detect when source is `IrExprKind::List { elements }` with known len
- For len ≤ 8: unroll loop, emit inline body for each element
- Eliminates: loop counter, bounds check, branch per element

---

## Tier 2 — Medium-term (weeks)

### 2.1 WASM SIMD for Numeric Lists

**Impact**: 2-4x on list.map/filter/fold with Int/Float
**Effort**: Large (new WASM emit path)

```wasm
;; Current: 1 element per iteration
i64.load  ; load element
i64.const 2
i64.mul
i64.store ; store result

;; SIMD: 2 i64 elements per iteration
v128.load           ; load 2 elements
i64x2.mul (const 2) ; multiply both
v128.store          ; store both
```

**Implementation**:
- Detect numeric list ops (Int/Float only)
- Emit SIMD loop for bulk of elements + scalar tail for remainder
- Requires: WASM SIMD proposal support in wasmtime (already available)
- Guard: `elem_size == 8` (i64/f64) for v128 = 2 lanes

### 2.2 Arena / Region-Based Allocator

**Impact**: Enables long-running WASM programs
**Effort**: Large (runtime redesign)

Current bump allocator never frees memory. For benchmarks this is fine, but production
WASM programs will OOM. Options:

1. **Arena per function call**: alloc arena on entry, free on return. Works for
   non-escaping allocations (pairs well with escape analysis from 1.2).
2. **Generational GC**: Young gen (bump) + old gen (mark-sweep). Complex but general.
3. **Reference counting**: Already have COW refcount header. Enable actual RC with
   free-on-zero. Simplest path from current architecture.

**Recommended**: Start with arena per function call (leverages escape analysis), add RC later.

### 2.3 Partial Evaluation / Function Specialization

**Impact**: Eliminates polymorphic dispatch overhead
**Effort**: Large (IR transform)

```almide
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)
apply((n) => n + 1, 5)
// Specialize: apply_add1(5) → 5 + 1 = 6 (no call_indirect)
```

**Implementation**:
- At call sites where function args are known lambdas, clone + specialize
- Inline the lambda body into the specialized function
- Combined with stream fusion for maximum effect

### 2.4 Improved Sort (Introsort / pdqsort)

**Impact**: 2-3x on random data sort
**Effort**: Medium (WASM runtime)

Current: merge sort + run detection.
Target: quicksort with median-of-3 pivot + insertion sort for small partitions.
Falls back to merge sort on pathological input (introsort pattern).

---

## Tier 3 — Long-term (architecture)

### 3.1 WASM GC Proposal

When WASM GC (struct/array/ref types) is widely supported:
- Replace linear-memory heap with WASM-native managed objects
- Eliminate manual alloc/free, let the engine's GC handle it
- Struct field access becomes `struct.get` (single instruction) instead of `i32.load(offset)`
- Array operations become `array.get`/`array.set`
- Massive simplification of the emitter + potential perf gains from engine-level optimization

### 3.2 WASM Component Model

- Inter-module calls without serialization
- Expose Almide libraries as WASM components consumable by any language
- Import components from other languages (Rust, Go, Python) natively

### 3.3 Profile-Guided Optimization (PGO)

- Collect runtime profiles from wasmtime/browser
- Feed back into compiler: inline hot functions, optimize hot loops
- Specialize polymorphic call sites based on observed types
- Requires: PGO infrastructure (profile format, feedback pipeline)

### 3.4 Ahead-of-Time Compilation Cache

- Cache compiled WASM modules for instant startup
- Compile-time specialization for known deployment targets (wasmtime vs V8 vs SpiderMonkey)
- Pre-compute JIT hints based on target engine

---

## Principles

1. **Language knowledge beats generic optimization.** Every Almide-specific transform
   (binary rec, lambda inline, branchless filter) outperforms LLVM's generic equivalent.

2. **WASM knowledge beats portability.** Targeting one output format lets us exploit
   its specific characteristics (bump alloc zero-fill, memory.copy, select instruction).

3. **Measure before optimizing.** Every change must show improvement on the benchmark
   suite. No speculative optimization without data.

4. **Correctness first.** All 239 tests must pass after every change. CI must be green
   before merge.
