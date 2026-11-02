> **HISTORICAL (#916).** The vs-Rust scoreboard and all 27 "Completed
> Optimizations" below were measured at v0.25.0 against the **v0 emitter
> retired in #782**, in files that no longer exist — on a machine the raw
> log itself notes was under load. Kept as the record of that campaign; the
> re-benchmark of the v1 legs with a stated baseline is #917. The current
> vs-Rust story lives in docs/wasm/WASM-OUTPUT.md and the memory ledger's
> "Rust-wasm打倒アーク" (v0.34.1, adversarial-Rust 4-0).

<!-- description: WASM optimization roadmap and the running scoreboard against Rust+LLVM -->
<a id="wasm-optimization-roadmap"></a>

# WASM Optimization Roadmap

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](wasm-optimization-roadmap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-optimization-roadmap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-optimization-roadmap.zh-CN.md)

> Non-normative historical reader snapshot. Pinned original authoritative and unchanged; no automatic synchronization or independent benchmark/gate execution. Old v0 emitter scores on a loaded machine, stale current labels, later v1 wasm-opt entry and proposed targets retain their dates. Ratio cells inconsistent with displayed timings are copied without correction. All-wins and universal language-specific optimization claims are source campaign assertions, not new comparative evidence or guarantees. A differential gate is not formal verification of the external wasm-opt transform. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-optimization-roadmap.md)

> Almide's WASM emitter: 336B hello world. **Current: 3 wins / 2 ties / 6 losses vs Rust+LLVM
> (v0.25.0 vs rustc 1.95, 2026-06-07 — the opponent moves too; see Baseline Update)**. At v0.23.4: 5W/2T/4L.
> This roadmap targets winning **all 11** benchmarks against *current stable* rustc.

<a id="precision-benchmark-1m-scale-v0234"></a>

## Precision Benchmark (1M scale, v0.23.4)

| Benchmark          | Almide  | Rust   | Ratio      | Status     |
|--------------------|---------|--------|------------|------------|
| fib38              | 163ms   | 164ms  | ~1.0x      | ≈ TIE      |
| sort_1M            |   6.1ms |  6.7ms | **0.91x**  | ✓ WIN      |
| list_map_1M        |   3.9ms |  1.6ms | 3.9x       | ✗ LOSING   |
| list_filter_1M     |   2.6ms |  2.2ms | 1.5x       | ✗ LOSING   |
| list_fold_1M       |   1.1ms |  0.4ms | 2.8x       | ✗ LOSING   |
| str_concat_1M      |   2.6ms |  1.4ms | 2.2x       | ✗ LOSING   |
| map_insert_100k    |   3.8ms |  4.2ms | **0.90x**  | ≈ TIE      |
| map_get_100k       |   5.0ms |  6.3ms | **0.79x**  | ✓ WIN      |
| int_parse_1M       |  57ms   | 66ms   | **0.86x**  | ✓ WIN      |
| int_tostring_1M    |  35ms   | 59ms   | **0.59x**  | ✓ WIN      |
| math_sqrt_1M       |   0.4ms |  1.5ms | **0.27x**  | ✓ WIN      |

**Score: 5 wins, 2 ties, 4 losses**

---

<a id="baseline-update-2026-06-07--v0250-vs-rustc-195-相手が動いた"></a>

## Baseline Update 2026-06-07 — v0.25.0 vs rustc 1.95 (the opponent improved)

| Benchmark          | Almide 0.25.0 | Rust 1.95 | Ratio     | Status  | vs v0.23.4 |
|--------------------|---------------|-----------|-----------|---------|-----------|
| fib38              | 145.2ms       | 142.7ms   | 1.02x     | ≈ TIE   | TIE maintained |
| sort_1M            |   5.8ms       |  4.8ms    | 1.21x     | ✗ LOSING | **WIN→LOSS reversal** |
| list_map_1M        |   3.3ms       |  0.7ms    | 4.7x      | ✗ LOSING | Almide improved, ratio worsened |
| list_filter_1M     |   2.6ms       |  1.7ms    | 1.53x     | ✗ LOSING | Flat |
| list_fold_1M       |   1.1ms       |  0.4ms    | 2.75x     | ✗ LOSING | Flat |
| str_concat_1M      |   3.0ms       |  1.4ms    | 2.14x     | ✗ LOSING | Flat |
| map_insert_100k    |   4.2ms       |  3.7ms    | 1.14x     | ✗ LOSING | **TIE→LOSS reversal** |
| map_get_100k       |   4.8ms       |  5.4ms    | **0.89x** | ✓ WIN   | WIN maintained |
| int_parse_1M       |  55.6ms       | 56.0ms    | 0.99x     | ≈ TIE   | WIN→TIE |
| int_tostring_1M    |  18.6ms       | 50.8ms    | **0.37x** | ✓ WIN   | **35→18.6ms improvement** |
| math_sqrt_1M       |   0.4ms       |  1.4ms    | **0.29x** | ✓ WIN   | WIN maintained |

**Score: 3 wins / 2 ties / 6 losses.** Almide did not regress, and actually improved, but rustc 1.95 + wasmtime 42 advanced map 1.6→0.7ms / sort 6.7→4.8ms, reversing two results. Raw data and measurement conditions: `research/benchmark/stdlib/results/v0.25.0-vs-rust1.95.txt` (loaded machine, min-of-5; sort / map_insert reversals need reconfirmation in a quiet environment). Binary size: Almide 5.9KB vs Rust 109KB (18.6x).

<a id="教訓--タスク-ratio-ratchet-gate"></a>

### Lesson → Task: ratio-ratchet gate

Targets against a frozen opponent silently rot. Expected values in the Fix Plan below were derived against old rustc; even reaching them leaves map at ~2.9x.

- [ ] **perf-ratchet gate**: rebuild the Rust twin (`rust_wasm_compare/src/precise_all.rs`) with **current stable rustc every time**, ratchet the min-of-5 RATIO, worsening makes CI red. Result JSON doubles as the performance entry in the `make verify` dossier.
- [ ] **Rederive Fix Plan Expected values against rustc 1.95** (fold ≈TIE, concat ≈TIE expected; map after hoisting still ~2.9x. Rust's 0.7ms ≒ 23GB/s is bandwidth-bound; further measures to reach that are the core of 11/11).
- [ ] **Investigate sort_1M / map_insert_100k reversals** — remeasure quietly; analyze roots if differences are real.
- [ ] Order: fold (small) → str_concat (small) → map base-pointer hoist (medium) → filter (large).

---

<a id="remaining-losses--root-cause--fix-plan"></a>

## Remaining Losses — Root Cause & Fix Plan

All 4 losses are "our emit is less efficient than LLVM's" — not WASM limitations.
Both Almide and Rust output WASM and run on the same wasmtime JIT.
LLVM uses the same v128 instructions we do — it just emits tighter loops.

<a id="1-list_fold-28x--simd-accumulator-not-implemented"></a>

### 1. list_fold (2.8x) — SIMD accumulator not implemented

**Root cause**: We emit a scalar loop. LLVM emits i64x2.add accumulator.

```
Almide (scalar):
  loop: i64.load → i64.add acc → ptr += 8

LLVM (SIMD):
  loop: v128.load → i64x2.add acc_v128 → ptr += 16
  end:  extract_lane 0 + extract_lane 1 → final sum
```

**Fix**: Detect `fold(init, (a, x) => a + x)` with Int element.
Emit i64x2 accumulator with horizontal add at end. 4× unroll.

**Effort**: Small. Pattern match on fold lambda body, same approach as map SIMD.
**Expected**: 1.1ms → ~0.4ms (match Rust)

<a id="2-list_map-39x--simd-loop-overhead"></a>

### 2. list_map (3.9x) — SIMD loop overhead

**Root cause**: SIMD is implemented but each v128 op has redundant
`local.get dst_ptr` / `local.get src_ptr`. LLVM avoids this by using
`offset` in load/store instructions relative to a single base pointer.

```
Almide:
  local.get dst_ptr          ;; redundant per-op
  local.get src_ptr          ;; redundant per-op
  v128.load offset=0
  ...
  v128.store offset=0

LLVM:
  v128.load offset=0 (base=src)   ;; no extra local.get
  ...
  v128.store offset=0 (base=dst)
```

**Fix**: Hoist `local.get src_ptr` and `local.get dst_ptr` outside
the unrolled block. Use `local.tee` to keep base on stack.
Each unrolled op uses only `v128.load offset=N` with N=0,16,32,48.

**Effort**: Medium. Restructure SIMD emit to stack-based addressing.
**Expected**: 3.9ms → ~2ms

<a id="3-str_concat-22x--initial-cap0-causes-20-grows"></a>

### 3. str_concat (2.2x) — initial cap=0 causes 20 grows

**Root cause**: `var s = ""` creates cap=0 string. Each grow calls
`__string_append` (function call overhead). 1M appends = 20 grows.
Rust also grows from 0 but LLVM fully inlines the grow path.

**Fix**: Emit `var s = ""` with initial cap=16 (or 64). First 16
appends are pure inline byte stores with zero function calls.
Also: inline the grow path entirely (avoid string_append call).

**Effort**: Small. Change empty string emit to include capacity.
**Expected**: 2.6ms → ~1.5ms

<a id="4-list_filter-15x--scalar-predicate"></a>

### 4. list_filter (1.5x) — scalar predicate

**Root cause**: Our branchless filter is good but scalar.
LLVM may batch predicate checks via SIMD.

**Fix**: For simple predicates like `x % 2 == 0` (now `x & 1 == 0`
via ModInt peephole), emit SIMD batch check:
`v128.load` → `v128.and(mask)` → `i64x2.eq(zero)` → conditional copy.
Challenging because WASM SIMD lacks compress-store.

**Effort**: Large. SIMD filter requires creative approach.
**Expected**: 2.6ms → ~2.0ms (modest gain, SIMD filter is hard)

---

<a id="completed-optimizations-v0233--v0234"></a>

## Completed Optimizations (v0.23.3 + v0.23.4)

<a id="session-1-v0233-14-optimizations"></a>

### Session 1 (v0.23.3): 14 optimizations

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

<a id="session-2-v0234-13-optimizations"></a>

### Session 2 (v0.23.4): 13 optimizations

| # | Optimization | Impact | Files |
|---|---|---|---|
| 15 | SIMD v128 list.map (4× unrolled) | map SIMD path | calls_list_closure2.rs, wasm_macro.rs |
| 16 | Inline 1-char string append | skip function call | statements.rs |
| 17 | string_append memory.copy | 3 byte loops eliminated | runtime.rs |
| 18 | Pointer-based filter/fold | eliminate idx multiply | calls_list_closure2.rs |
| 19 | ModInt peephole (x%n==0 → x&(n-1)==0) | avoid i64.rem_s | expressions.rs |
| 20 | Map growth factor 4× | 13→7 resizes | calls_map.rs |
| 21 | Exponential growth allocator | amortized O(1) grow | runtime.rs |
| 22 | Initial memory 128KB | minimal footprint | mod.rs |
| 23 | Conditional fs init | skip preopen for non-fs | mod.rs, functions.rs, dce.rs |
| 24 | Aggressive function DCE | element table cleanup | mod.rs |
| 25 | Dead data elimination | strip unused strings | dce.rs |
| 26 | String pool at offset 4096 | safe data DCE | mod.rs |
| 27 | Binary size: 25KB → 336B | -98.7% hello world | all above |

<a id="tier-2--medium-term"></a>

## Tier 2 — Medium-term

<a id="21-escape-analysis-for-optionresult"></a>

### 2.1 Escape Analysis for Option/Result

Detect non-escaping Option/Result and return as WASM multi-value
`(i32, i64)` instead of heap-allocating a wrapper.

<a id="22-arena-allocator"></a>

### 2.2 Arena Allocator

Region-based memory management for non-escaping allocations.
Pairs with escape analysis.

<a id="23-partial-evaluation"></a>

### 2.3 Partial Evaluation

Specialize higher-order functions at known call sites.

<a id="24-wasm-opt-integration-optional--done-2026-07-23"></a>

### 2.4 wasm-opt integration (optional) — DONE (2026-07-23)

Shipped as `almide build --target wasm --wasm-opt`. The "<1% additional"
estimate above was measured against the old v0 emitter's own built-in DCE
(pre-retirement, #782) and no longer holds: v1's in-renderer reachability DCE
(landed this session — preamble/import/function/data-segment pruning, plus a
function-names-only name-section trim) cuts the *default* verified module
from 8,713 B to 770 B on Hello World, but `wasm-opt -Oz` still saves a further
28–43% on TOP of that (measured 2026-07-23): 770→548 B (Hello), 11,965→6,868 B
(Variant). Reachability DCE only removes whole unreached units; it can't do
wasm-opt's instruction-level work (local coalescing, inlining, dead-store
elimination) — the two are complementary, not redundant.

Kept strictly opt-in and default-off: the trust-spine ships the exact bytes
its own certified rendering process produced, and `wasm-opt` is an external,
unverified transform on that output. `--wasm-opt`'s own safety claim (that it
never changes observable behavior on Almide's generated wasm) is backed by a
dedicated differential gate — `tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`, run in CI
(`.github/workflows/ci.yml`) — not just asserted. Full writeup:
[docs/wasm/WASM-OUTPUT.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md).

<a id="25-memory-layout-type-safety"></a>

### 2.5 Memory layout type safety

Replace magic constants (SCRATCH_ITOA, NEWLINE_OFFSET) with
newtype wrappers (MemOffset, StringPoolOffset, HeapPtr).
Prevents the class of bugs where integer constants collide
with data section offsets.

<a id="tier-3--long-term"></a>

## Tier 3 — Long-term

<a id="31-wasm-gc-proposal"></a>

### 3.1 WASM GC Proposal
<a id="32-wasm-component-model"></a>

### 3.2 WASM Component Model
<a id="33-profile-guided-optimization"></a>

### 3.3 Profile-Guided Optimization

---

<a id="principles"></a>

## Principles

1. **Same WASM, same runtime.** Both Almide and Rust produce WASM
   bytecode for the same wasmtime JIT. Performance gaps are emit
   quality, not platform limitations.
2. **Language knowledge > generic optimization.** Every Almide-specific
   transform (get_or fusion, itoa, sqrt) outperforms LLVM's generic equivalent.
3. **Measure the real bottleneck.** map.get's problem was Option heap
   alloc, not hash table layout. Always profile before optimizing.
4. **1M scale reveals truth.** 100k benchmarks are noise-dominated.
   Always verify at 1M+ scale.
