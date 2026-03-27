<!-- description: Runtime design targeting best-in-class compiler performance -->
<!-- done: 2026-03-18 -->
# Almide Runtime

> Do not win by being safe. Win by reaching the ideal.

## Why this is possible

Existing languages pay the cost of generality. Almide has strong constraints. That becomes a weapon:

```
Constraints = Information = Room for optimization
```

| What Almide knows | What general compilers don't know | Optimization opportunity |
|---|---|---|
| `fn` = pure (no side effects) | LLVM guesses function purity | Auto-parallelization, memoization, reordering calls |
| `let` = immutable | C/C++ assumes all variables are mutable | No copies needed, references suffice |
| `var` is explicit | Mutation points are unknown | SSA conversion is trivial |
| No null | Null checks required | Zero null checks |
| No exceptions (Result) | Unwind tables required | Simpler calling convention |
| Use-count exists | Lifetime inference required | Statically determine when GC/RC is unnecessary |
| Entire program is visible | Assumes separate compilation | Whole-program optimization always available |
| Code is written by LLMs | Assumes human coding habits | LLM-generated pattern-specific codegen |

---

## Architecture: Multi-Tier Compilation

```
Source (.almd)
  │
  ▼
Almide IR (Typed, Pure/Effect annotated)
  │
  ├─── Tier 0: Direct Interpret     (0ms compile, 10x slow)     ← Dev REPL
  ├─── Tier 1: Copy-and-Patch JIT   (1ms compile, 2x slow)      ← Dev run
  ├─── Tier 2: Almide Optimizer     (100ms compile, 1.0x)        ← Test/staging
  └─── Tier 3: LLVM / rustc         (10s compile, 0.95x — C level)  ← Production build
```

All tiers emit from the same IR. Tiers 0-1 during development, Tiers 2-3 only for release.

---

## Super Optimization 1: Static Region Memory

No GC. No RC. No borrow checker. Everything is determined at compile time.

```almide
fn process(data: List[Int]) -> List[Int] =
  data
    |> list.filter((x) => x > 0)      // region A
    |> list.map((x) => x * 2)          // region B (A dies)
    |> list.take(10)                    // region C (B dies)
```

What the compiler sees:
- `filter`'s result is only used by `map` → region A is bulk-freed when `map` completes
- `map`'s result is only used by `take` → region B is bulk-freed when `take` completes
- Intermediate data doesn't scatter across the heap. Allocated in contiguous memory and discarded at once

Higher-level reasoning than Rust's borrow checker. A combination of use-count + pure fn guarantee + pipeline analysis.

---

## Super Optimization 2: Automatic Parallelism

`fan` provides explicit parallelism. Pure fns can be implicitly parallelized too:

```almide
fn expensive_a(x: Int) -> Int = ...  // pure, expensive
fn expensive_b(x: Int) -> Int = ...  // pure, expensive

fn process(x: Int) -> (Int, Int) = {
  let a = expensive_a(x)   // no dependency
  let b = expensive_b(x)   // no dependency
  (a, b)
}
```

The effect system proves "these two are independent." The compiler auto-parallelizes:

```
process(x) → spawn(expensive_a(x)), spawn(expensive_b(x)), join
```

Smarter than Go's goroutines. Go guesses "everything might be parallelizable." Almide knows "this is definitely parallelizable."

---

## Super Optimization 3: Speculative Deforestation (Stream Fusion)

The biggest enemy of functional programming: intermediate data structure creation.

```almide
xs |> list.map(f) |> list.filter(g) |> list.fold(0, h)
```

Naive implementation: creates and discards 3 lists.

The Almide compiler recognizes this as a composition of pure fns and eliminates intermediate lists entirely:

```rust
// Generated code: traverses the list only once
let mut acc = 0;
for x in xs {
    let y = f(x);
    if g(y) {
        acc = h(acc, y);
    }
}
```

This is the stream fusion / deforestation that GHC (Haskell) does. Almide can do it safely because of the effect system. GHC guesses "probably pure." Almide knows "definitely pure."

---

## Super Optimization 4: Shape-Specialized Codegen

```almide
type Point = { x: Float, y: Float }
let points: List[Point] = ...
```

General compilers: `List<Box<Point>>` — an array of pointers. Cache misses everywhere.

Almide compiler: types are fully visible, so it transforms to Structure of Arrays:

```rust
struct PointList {
    xs: Vec<f64>,  // only x coordinates, contiguous
    ys: Vec<f64>,  // only y coordinates, contiguous
}
```

Processable in bulk with SIMD. The compiler automatically performs the optimization that game engines do manually.

---

## Super Optimization 5: LLM-Aware Compilation

Unique to Almide. If we know code is written by LLMs:

- Statistically profile LLM-generated code patterns
- Prepare specialized codegen templates for common patterns
- Optimize for LLM code characteristics (deep pipelines, many intermediate variables, heavy match usage)

```
LLM writes → Compiler optimizes → Execution results fed back to LLM → Better code written
```

A co-evolution loop between compiler and LLM. No other language has this perspective.

---

## Mathematical Guarantees via Self-Hosting

Rewriting the compiler in Almide gives us:

1. **All passes are pure fns** — the compiler itself proves this
2. **Fixed-point verification** — `compile(compiler_source) = compiler` is strong evidence of correctness
3. **Trusting Trust defense** — pure fns cannot do I/O. Backdoors cannot be injected into the compiler
4. **Recursive self-optimization** — the super optimizations above are applied to the compiler itself

---

## Implementation Roadmap

### Phase 0: Foundation (existing weapons)
- ✅ Typed IR
- ✅ Pure/Effect split
- ✅ Use-count analysis
- ✅ Multi-target codegen (Rust, TS, JS, WASM)
- ✅ Cross-target CI (91/91)

### Phase 1: Instant execution experience
- IR interpreter (Tier 0) — bypass rustc. Instant execution
- TS path improvement — make TS the default for `almide run`

### Phase 2: Pipe Fusion
- `map |> filter |> fold` → single-pass traversal
- Intermediate list elimination
- Safe fusion guaranteed by pure fn

### Phase 3: Region Memory
- Region inference — manage intermediate pipeline data via regions
- One-shot deallocation — bulk free per region
- Static memory management with no GC/RC needed

### Phase 4: JIT
- Copy-and-Patch baseline JIT (Tier 1)
- Almide IR → machine code templates
- Compile time under 1ms

### Phase 5: Auto-Parallelism
- Data dependency analysis for pure fns
- Automatic parallelization of independent pure calls
- Integration with fan (explicit + implicit parallelism coexistence)

### Phase 6: Optimizing Backend (Tier 2)
- Almide-specific optimization pipeline
- Shape specialization (SoA transformation)
- Automatic SIMD vectorization
- Profile-guided optimization

### Phase 7: Self-Hosting
- User-defined generic functions (prerequisite)
- Rewrite the compiler in Almide
- Bootstrap test (fixed-point verification)
- Recursive self-optimization of the compiler

### Phase 8: LLM Co-Evolution
- LLM-generated code pattern statistics
- Codegen specialized for common patterns
- Compiler ↔ LLM feedback loop

---

## Competitive Comparison

| Language | Compile speed | Execution speed | Memory management | Concurrency |
|----------|--------------|-----------------|-------------------|-------------|
| C | Fast | Fastest | Manual (dangerous) | Manual (dangerous) |
| Rust | Slow | Near-fastest | Borrow checker | Manual + async |
| Go | Fast | Good | GC (pause) | goroutine |
| Zig | Fast | Near-fastest | Manual (safe) | Manual |
| **Almide (goal)** | **Fastest** | **Near-fastest** | **Static region (safe)** | **Auto-parallel (safe)** |

Almide's goal: **Go's compile speed × Rust's execution speed × fully automatic memory management × auto-parallelization**.

Strong constraints are the weapon. The more freedom taken from humans, the smarter the compiler becomes.

---

## Reference Technologies

- **Copy-and-Patch JIT**: Adopted by CPython 3.13. Patches together templated machine code
- **Stream Fusion**: GHC (Haskell) intermediate list elimination. `foldr/build` rules
- **Region Inference**: MLKit (ML) static memory management. No GC needed
- **Structure of Arrays**: Data-Oriented Design. Game engines (Unity DOTS, Bevy ECS)
- **Deforestation**: Wadler (1988). Elimination of intermediate data structures
- **YJIT / ZJIT**: Ruby JIT. Copy-and-Patch based
