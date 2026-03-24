# Performance Research: Path to World #1 [ON-HOLD]

**Research thesis**: High-level semantic information preserved through the compilation pipeline enables optimizations impossible in traditional low-level compilers. A language designed for LLM accuracy is simultaneously optimized for compiler analysis — both benefit from explicit semantics, clear data flow, and minimal aliasing.

**Current state**: n-body 1.74s (Almide) vs 1.69s (hand-written Rust) = 2.9% overhead.

**Target**: Consistently outperform hand-written Rust across standard benchmarks.

---

## Phase 0: Close the 2.9% Gap (Engineering)

**Goal**: 1.00x vs hand-written Rust on n-body and 3+ additional benchmarks.

**Root cause of 2.9%**: Redundant `.clone()` on Copy types, unnecessary `as f64` casts, suboptimal variable binding patterns.

| Task | Effect | Difficulty |
|------|--------|-----------|
| Clone on Copy type elimination | `.clone()` on i64/f64/bool → remove | Low |
| Redundant cast elimination | `x as f64` when x is already f64 → remove | Low |
| Let-binding reduction | `let tmp = expr; use(tmp)` where tmp is used once → inline | Medium |
| Stream fusion Phase 2 | Detection done → implement actual IR rewriting | Medium |
| Clone reduction Phase 4 | for-in, member access, match subject, record spread | Medium |

**Metric**: n-body ≤ 1.69s, spectral-norm ≤ Rust reference, binary-trees ≤ Rust reference.

**Benchmark suite**: Implement 5 programs from Benchmarks Game in Almide:
- [x] n-body (gravitational simulation)
- [ ] spectral-norm (eigenvalue)
- [ ] binary-trees (GC stress / allocation)
- [ ] fannkuch-redux (combinatorial)
- [ ] mandelbrot (floating-point parallel)

---

## Phase 1: Surpass Human-Written Rust (Applied Research)

**Goal**: Beat hand-written Rust on ≥3 benchmarks by ≥5%.

**Research question**: Can whole-program semantic analysis generate code that humans wouldn't write?

### 1a. Whole-Program Inlining

Almide sees the entire program. rustc's inlining is constrained by crate boundaries and heuristics.

```
Almide approach:
  1. Build call graph with use-counts (already have this)
  2. Inline functions with use_count == 1 unconditionally
  3. Inline small functions (body ≤ 3 IR nodes) unconditionally
  4. For hot paths: inline up to depth 3
  5. Emit single monolithic function for hot loops
```

rustc + LTO does some of this, but Almide has better information: it knows which functions are pure (effect system), which variables escape (borrow analysis), and exact use-counts.

### 1b. Allocation Elimination

Almide's type system guarantees immutability by default. This means:

```
// Almide knows this list is never modified after creation
let xs = [1, 2, 3, 4, 5]
let sum = xs.fold(0, |acc, x| acc + x)

// Instead of Vec::new() + push + push + ..., emit:
// static array on stack + direct fold (no heap allocation)
```

| Pattern | Current codegen | Optimized codegen |
|---------|----------------|-------------------|
| Small list literal | `vec![1, 2, 3]` (heap) | `[1, 2, 3]` (stack array) |
| List used only in fold | `Vec<T>` + iterator | Direct accumulation (no list) |
| String concatenation chain | Multiple `String::new()` + `push_str` | Pre-sized `String::with_capacity` |
| Record created and immediately destructured | Struct allocation + field access | Direct value forwarding |

### 1c. Semantic-Aware Dead Code Elimination

Effect system enables aggressive DCE:

```
// Almide knows: pure fn never has side effects
fn square(x: Int) -> Int = x * x

let a = square(10)  // pure, use_count == 0 → eliminate entire call
```

Current DCE is conservative (keeps all function calls). With effect annotations, pure function calls with unused results can be eliminated.

### 1d. Type-Specialized Container Operations

Almide knows the concrete element type at every call site:

```
// list.sort_by knows T = Int at this call site
users.sort_by(|u| u.age)

// Instead of generic comparison, emit specialized:
// .sort_by(|a, b| a.age.cmp(&b.age))  ← already specific
// But also: for Int lists, use counting sort / radix sort when len > threshold
```

**Metric**: ≥3 Benchmarks Game programs where Almide beats Rust reference implementation by ≥5%.

---

## Phase 2: LLVM IR Direct Emission (Engineering + Research)

**Goal**: Skip rustc entirely. Emit LLVM IR from Almide IR.

**Research question**: Does removing the Rust abstraction layer enable novel optimization opportunities?

### Architecture

```
Current:   Almide IR → Rust source → rustc → LLVM IR → LLVM → binary
Proposed:  Almide IR → LLVM IR → (custom passes) → LLVM → binary
```

### Why this matters

1. **Compile time**: rustc frontend is 60-70% of compile time. Direct LLVM IR cuts `almide run` from ~2s to ~0.6s.
2. **Custom LLVM passes**: Insert optimization passes between Almide lowering and LLVM optimization that use Almide's semantic information.
3. **Fine-grained control**: LLVM IR annotations (`noalias`, `readonly`, `nounwind`, `willreturn`) can be emitted precisely based on Almide's effect/purity analysis.

### Semantic annotations Almide can emit

| Almide knowledge | LLVM annotation | Effect |
|-----------------|-----------------|--------|
| Pure function (no effects) | `readonly`, `willreturn`, `nounwind` | Enables aggressive CSE, LICM |
| Immutable variable | `noalias` on pointer args | Enables load elimination |
| Use-count == 1 | `nocapture` | Enables stack promotion |
| No aliasing (ownership) | `noalias`, `dereferenceable` | Enables vectorization |
| Known loop bounds (Range) | `!llvm.loop` metadata | Enables loop unrolling |

rustc adds some of these, but Almide can be **more precise** because it has higher-level semantic information.

### Implementation path

| Step | Work | LOC estimate |
|------|------|-------------|
| LLVM IR text emitter | New codegen target in nanopass pipeline | ~2,000 |
| Runtime library in LLVM IR | Port core_runtime.txt to LLVM bitcode | ~1,000 |
| Semantic annotation pass | Emit noalias/readonly/nounwind from effect analysis | ~500 |
| Custom LLVM pass: deforestation | Eliminate intermediate data structures | ~1,500 (C++) |
| Custom LLVM pass: arena coalescing | Merge allocations with known lifetimes | ~1,000 (C++) |

**Dependency**: `inkwell` crate (Rust LLVM bindings) or raw LLVM-C API.

---

## Phase 3: Semantic-Aware Optimization (Pure Research)

**Goal**: Novel optimization passes that exploit Almide's semantic richness.

**Research question**: What optimizations become possible when the compiler knows purity, effects, ownership, and algebraic laws?

### 3a. Deforestation (Intermediate Structure Elimination)

Stream fusion (Phase 0) fuses `map.filter.fold` into single loops. Deforestation generalizes this to **all intermediate data structures**:

```almide
fn process(users: List[User]) -> List[String] =
  users
    |> filter(|u| u.age > 18)
    |> map(|u| u.name)
    |> sort_by(|n| n)
    |> take(10)

// Current: filter → new Vec → map → new Vec → sort → new Vec → take → new Vec
// Deforested: single pass with in-place sort on pre-filtered view
```

**Research contribution**: Deforestation guided by algebraic laws from TypeConstructorRegistry. Almide already has the law infrastructure (HKT foundation Phase 1-4).

### 3b. Effect-Guided Parallelization

Effect annotations enable automatic parallelization:

```almide
// Almide knows: square is pure, map preserves order
let results = large_list.map(|x| square(x))

// Safe to parallelize because:
// 1. square has no effects
// 2. map is a Functor operation (preserves structure)
// 3. No mutable state captured
```

Emit `rayon::par_iter()` or SIMD intrinsics when:
- Function is pure (effect analysis)
- Operation is embarrassingly parallel (algebraic law: map distributes)
- Collection size exceeds threshold

### 3c. Memoization of Pure Functions

```almide
fn fib(n: Int) -> Int =
  if n <= 1 { n }
  else { fib(n - 1) + fib(n - 2) }

// Almide knows: fib is pure, recursive, Int → Int
// Auto-memoize: HashMap<i64, i64> cache
```

Criteria: pure function + recursive + small input domain.

### 3d. Data Layout Optimization

Almide knows field access patterns from use-count analysis:

```almide
type User { name: String, age: Int, email: String, score: Float }

// If hot loop only accesses .age and .score:
for u in users { process(u.age, u.score) }

// Emit SoA (Struct of Arrays) for hot fields:
// ages: Vec<i64>, scores: Vec<f64> — cache-friendly
```

**Research contribution**: Automatic AoS → SoA transformation guided by field use-counts and loop analysis.

---

## Phase 4: Validation & Publication

### Benchmark targets

| Benchmark | Current | Phase 0 | Phase 1 | Phase 2-3 |
|-----------|---------|---------|---------|-----------|
| n-body | 1.03x | 1.00x | 0.97x | 0.90x |
| spectral-norm | — | 1.00x | 0.95x | 0.90x |
| binary-trees | — | 1.00x | 0.90x | 0.85x |
| fannkuch-redux | — | 1.00x | 0.98x | 0.92x |
| mandelbrot | — | 1.00x | 0.95x | 0.88x |

(x = ratio vs best Rust implementation. < 1.0 = Almide is faster.)

### Publication targets

| Phase | Title | Venue |
|-------|-------|-------|
| 0-1 | "Almide: Near-Zero-Overhead High-Level Compilation via Semantic-Aware Rust Generation" | arXiv preprint |
| 2 | "Semantic Annotations for LLVM: Exploiting High-Level Language Properties for Low-Level Optimization" | CGO / CC |
| 3 | "Effect-Guided Optimization: How Purity Enables Automatic Parallelization and Deforestation" | PLDI / OOPSLA |
| All | "Modification Survival Rate × Performance: A Language Designed for Both LLMs and Metal" | POPL |

### Competitive positioning

| Language | Performance | LLM accuracy | Almide advantage |
|----------|------------|--------------|-----------------|
| C | Baseline | Low (UB, manual memory) | Safer + faster via semantic opts |
| C++ | ~C | Low (complexity) | Simpler + comparable |
| Rust | ~C | Medium (borrow checker errors) | Same backend + semantic opts |
| Zig | ~C | Medium | Semantic richness |
| Mojo | Claims ~C | Unknown | Proven MSR metric |
| Go | ~2x C | High | 10-50x faster |
| TypeScript | ~20x C | High | 100x+ faster |

---

## Key Insight: LLM-Friendliness = Optimization-Friendliness

This is Almide's unique research contribution. The same properties that make a language easy for LLMs to write accurately also provide rich information for optimization:

| LLM-friendly property | Optimization benefit |
|-----------------------|---------------------|
| Immutable by default | No aliasing → aggressive optimization |
| Explicit effects (`effect fn`) | Purity analysis → DCE, parallelization, memoization |
| No hidden control flow (no exceptions in pure fn) | Predictable optimization |
| Type-dispatched operators in IR | No runtime type checks |
| UFCS (no method resolution ambiguity) | Complete call graph → whole-program optimization |
| No inheritance / vtable dispatch | Static dispatch → inlining |

**This correlation is not coincidental — it's structural.** A language that minimizes ambiguity for LLMs also minimizes ambiguity for optimizers. This thesis, if validated with benchmarks, is a publishable result.

---

## Priority

Phase 0 (close the gap) → Phase 1b-1c (allocation elimination + semantic DCE) → Phase 1a (inlining) → Phase 2 (LLVM direct) → Phase 3a (deforestation) → Phase 3b (parallelization).

Phase 0 is engineering. Phase 1 is applied research. Phase 2-3 is novel research. Phase 4 is validation.
