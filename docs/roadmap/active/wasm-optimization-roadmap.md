# WASM Optimization Roadmap

> Almide's WASM emitter beats Rust+LLVM on 7/11 benchmarks (v0.23.3+develop).
> This roadmap targets winning **all 11** through double-specialization:
> language semantics × WASM target knowledge.

## Precision Benchmark (1M scale, 2026-05-24)

| Benchmark          | Almide  | Rust   | Ratio      | Status     |
|--------------------|---------|--------|------------|------------|
| fib38              | 152.0ms |157.7ms | **0.96x**  | ✓ WIN      |
| sort_1M            |   6.2ms |  7.0ms | **0.89x**  | ✓ WIN      |
| list_map_1M        |   3.7ms |  1.6ms | 2.3x       | ✗ LOSING   |
| list_filter_1M     |   2.8ms |  3.8ms | **0.74x**  | ✓ WIN      |
| list_fold_1M       |   0.8ms |  0.9ms | **0.89x**  | ✓ WIN      |
| str_concat_100k    |   0.5ms |  0.3ms | 1.7x       | ✗ LOSING   |
| map_insert_100k    |   9.1ms |  5.7ms | 1.6x       | ✗ LOSING   |
| map_get_100k       |   1.8ms |  1.7ms | 1.06x      | ≈ (fused)  |
| int_parse_1M       |  53.5ms | 64.7ms | **0.83x**  | ✓ WIN      |
| int_tostring_1M    |  34.0ms | 52.8ms | **0.64x**  | ✓ WIN      |
| math_sqrt_1M       |   0.5ms |  1.4ms | **0.36x**  | ✓ WIN      |

**Score: 7 wins, 1 tie, 3 losses**

---

## Remaining Losses — Root Cause Analysis

### list_map (2.3x slower)

**Root cause**: LLVM vectorizes `iter().map(|x| x*2).collect()` with SIMD.
Our loop does 1 element per iteration (i64_load → i64_mul → i64_store).
Rust/LLVM processes 2-4 elements per iteration via auto-vectorization.

**Evidence**: `map_identity` (no-op body) takes same time as `map_x2`,
proving the bottleneck is memory throughput, not lambda body cost.
Pointer-based iteration already implemented (no multiply per element).

**Fix**: WASM SIMD — emit `v128.load` + `i64x2.mul` + `v128.store` for
Int/Float element types. Process 2 i64 elements per iteration. Scalar
tail for remainder. wasmtime supports WASM SIMD.

**Effort**: Large. New emit path for SIMD-eligible list ops.

### str_concat (1.7x slower)

**Root cause**: `s = s + "x"` → peephole rewrites to `__string_append(s, "x")`
runtime function call. 100k calls × function call overhead.
Rust's `String::push('x')` is inlined by LLVM to direct buffer write.

**Fix**: Inline string append for 1-byte literals. When the RHS of
`s = s + lit` is a 1-char string literal, emit inline WASM:
```
if len < cap:
  mem[ptr + STRING_DATA_OFFSET + len] = byte
  mem[ptr] = len + 1  // update len
else:
  call __string_append  // fallback for grow
```
Eliminates function call overhead for the common case (cap sufficient).

**Effort**: Medium. Peephole pattern in statements.rs WASM emit.

### map_insert (1.6x slower)

**Root cause**: 13 resizes from cap=16 to cap=131072. Each resize
rehashes all existing entries (hash + probe + copy per entry).
Rust HashMap has the same resize count but LLVM optimizes the
rehash loop (vectorized memcpy, inlined hash).

**Potential fixes** (pick one or combine):
1. **Initial capacity hint**: `map.with_capacity(n)` stdlib fn.
   Eliminates most resizes for known-size inserts.
2. **Faster resize**: `memory.copy` bulk transfer of tag+entry arrays
   (already partially implemented in Swiss Table layout).
3. **Growth factor 4x**: Grow by 4x instead of 2x. Fewer resizes
   (7 instead of 13) at cost of memory waste.
4. **Inline hash for Int keys**: Currently `emit_hash_key` generates
   ~8 WASM instructions. Could reduce to 4 with simpler hash
   (e.g., multiply-shift instead of splitmix).

**Effort**: Small (capacity hint) to Medium (inline hash).

---

## Completed Optimizations (this session)

| # | Optimization | Impact | Files |
|---|---|---|---|
| 1 | Hash table map (open addressing) | map 1000x faster | calls_map.rs, list_layout.rs |
| 2 | Sort run detection (asc/desc) | sort 5x faster | calls_list_helpers.rs |
| 3 | Lambda inlining (capture-free) | map/filter/fold ~2x | calls_list_closure2.rs, pass_closure_conversion.rs |
| 4 | Binary recursion transform | fib 2x faster | pass_tco.rs, target.rs |
| 5 | Stream fusion (map→filter→fold) | pipeline 4x faster | calls_list_closure2.rs |
| 6 | Branchless filter | filter 1.5x faster | calls_list_closure2.rs |
| 7 | Pointer-based iteration | loop overhead reduced | calls_list_closure2.rs, calls_list_helpers.rs |
| 8 | TCO in WASM pipeline | tail recursion → loop | target.rs |
| 9 | Adaptive scratch locals | fib 88→8 locals | functions.rs |
| 10 | String layout migration (data@8) | fixed 10+ files | runtime.rs, rt_*.rs, calls_*.rs |
| 11 | Swiss Table layout (1-byte tags) | cache-friendlier probing | calls_map.rs, expressions.rs |
| 12 | map.get??default → get_or fusion | eliminate Option heap alloc | pass_peephole.rs |
| 13 | 1-pass reverse copy for sort | sort matched Rust | calls_list_helpers.rs |
| 14 | Pointer-based list.map | eliminate idx multiply | calls_list_closure2.rs |

## Tier 1 — Next Steps (days)

### 1.1 Inline String Append for Literals

Detect `s = s + "x"` where RHS is 1-char literal. Emit inline
capacity check + byte store instead of `__string_append` call.

### 1.2 Map Initial Capacity Hint

Add `map.with_capacity(n)` that pre-allocates for n entries.
Eliminates resize cascade for bulk insert patterns.

### 1.3 Escape Analysis for Option/Result

Detect non-escaping Option/Result and return as WASM multi-value
`(i32, i64)` instead of heap-allocating a wrapper.

## Tier 2 — Medium-term (weeks)

### 2.1 WASM SIMD for Numeric List Ops

Emit `v128.load` + `i64x2.mul` + `v128.store` for Int/Float
list.map/filter. 2x throughput per iteration.

### 2.2 Arena Allocator

Region-based memory management for non-escaping allocations.
Pairs with escape analysis.

### 2.3 Partial Evaluation

Specialize higher-order functions at known call sites.

## Tier 3 — Long-term

### 3.1 WASM GC Proposal
### 3.2 WASM Component Model
### 3.3 Profile-Guided Optimization

---

## Principles

1. **Language knowledge > generic optimization.** Every Almide-specific
   transform outperforms LLVM's generic equivalent.
2. **Measure the real bottleneck.** map.get's problem was Option heap
   alloc, not hash table layout. Always profile before optimizing.
3. **WASM ≠ native.** Swiss Table tag separation hurt perf on WASM
   (extra address computation) despite helping on native CPUs.
4. **1M scale reveals truth.** 100k benchmarks are noise-dominated.
   Always verify at 1M+ scale.
