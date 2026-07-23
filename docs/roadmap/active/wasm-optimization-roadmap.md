# WASM Optimization Roadmap

> Almide's WASM emitter: 336B hello world. **Current: 3 wins / 2 ties / 6 losses vs Rust+LLVM
> (v0.25.0 vs rustc 1.95, 2026-06-07 — 相手も動く。Baseline Update 参照)**. v0.23.4 時点は 5W/2T/4L。
> This roadmap targets winning **all 11** benchmarks against *current stable* rustc.

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

## Baseline Update 2026-06-07 — v0.25.0 vs rustc 1.95 (相手が動いた)

| Benchmark          | Almide 0.25.0 | Rust 1.95 | Ratio     | Status  | v0.23.4比 |
|--------------------|---------------|-----------|-----------|---------|-----------|
| fib38              | 145.2ms       | 142.7ms   | 1.02x     | ≈ TIE   | TIE 維持 |
| sort_1M            |   5.8ms       |  4.8ms    | 1.21x     | ✗ LOSING | **WIN→LOSS 反転** |
| list_map_1M        |   3.3ms       |  0.7ms    | 4.7x      | ✗ LOSING | Almide 改善・比は悪化 |
| list_filter_1M     |   2.6ms       |  1.7ms    | 1.53x     | ✗ LOSING | 横ばい |
| list_fold_1M       |   1.1ms       |  0.4ms    | 2.75x     | ✗ LOSING | 横ばい |
| str_concat_1M      |   3.0ms       |  1.4ms    | 2.14x     | ✗ LOSING | 横ばい |
| map_insert_100k    |   4.2ms       |  3.7ms    | 1.14x     | ✗ LOSING | **TIE→LOSS 反転** |
| map_get_100k       |   4.8ms       |  5.4ms    | **0.89x** | ✓ WIN   | WIN 維持 |
| int_parse_1M       |  55.6ms       | 56.0ms    | 0.99x     | ≈ TIE   | WIN→TIE |
| int_tostring_1M    |  18.6ms       | 50.8ms    | **0.37x** | ✓ WIN   | **35→18.6ms 改善** |
| math_sqrt_1M       |   0.4ms       |  1.4ms    | **0.29x** | ✓ WIN   | WIN 維持 |

**Score: 3 wins / 2 ties / 6 losses.** Almide は非退行(むしろ改善)だが、rustc 1.95 + wasmtime 42 が
map 1.6→0.7ms / sort 6.7→4.8ms と前進し 2 本が反転した。生データ・計測条件:
`research/benchmark/stdlib/results/v0.25.0-vs-rust1.95.txt`(負荷あり機・min-of-5。
sort / map_insert の反転は静音環境で要再確認)。バイナリサイズは Almide 5.9KB vs Rust 109KB(18.6x)。

### 教訓 → タスク: ratio-ratchet gate

凍結した相手への目標値は黙って腐る。下の Fix Plan の Expected 値は旧 rustc 相手の導出で、
達成しても map は ~2.9x 残る。

- [ ] **perf-ratchet gate**: Rust 双子(`rust_wasm_compare/src/precise_all.rs`)を **current stable
      rustc で毎回再ビルド**し、min-of-5 の RATIO をラチェット(悪化で CI 赤)。結果 JSON は
      `make verify` dossier の性能項目を兼ねる
- [ ] **Fix Plan の Expected を rustc 1.95 基準で再導出**(fold ≈TIE、concat ≈TIE が見込み。
      map は hoist 後も ~2.9x → Rust の 0.7ms ≒ 23GB/s は帯域律速であり、そこまで詰める追加施策が
      11/11 の本丸)
- [ ] **sort_1M / map_insert_100k の反転調査** — 静音環境で再計測 → 実差なら根因分析
- [ ] 着手順: fold(小)→ str_concat(小)→ map base-pointer hoist(中)→ filter(大)

---

## Remaining Losses — Root Cause & Fix Plan

All 4 losses are "our emit is less efficient than LLVM's" — not WASM limitations.
Both Almide and Rust output WASM and run on the same wasmtime JIT.
LLVM uses the same v128 instructions we do — it just emits tighter loops.

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

### 3. str_concat (2.2x) — initial cap=0 causes 20 grows

**Root cause**: `var s = ""` creates cap=0 string. Each grow calls
`__string_append` (function call overhead). 1M appends = 20 grows.
Rust also grows from 0 but LLVM fully inlines the grow path.

**Fix**: Emit `var s = ""` with initial cap=16 (or 64). First 16
appends are pure inline byte stores with zero function calls.
Also: inline the grow path entirely (avoid string_append call).

**Effort**: Small. Change empty string emit to include capacity.
**Expected**: 2.6ms → ~1.5ms

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

## Completed Optimizations (v0.23.3 + v0.23.4)

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

## Tier 2 — Medium-term

### 2.1 Escape Analysis for Option/Result

Detect non-escaping Option/Result and return as WASM multi-value
`(i32, i64)` instead of heap-allocating a wrapper.

### 2.2 Arena Allocator

Region-based memory management for non-escaping allocations.
Pairs with escape analysis.

### 2.3 Partial Evaluation

Specialize higher-order functions at known call sites.

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
dedicated differential gate — `tests/wasm_opt_parity_test.rs`, run in CI
(`.github/workflows/ci.yml`) — not just asserted. Full writeup:
[docs/WASM-OUTPUT.md](../../WASM-OUTPUT.md).

### 2.5 Memory layout type safety

Replace magic constants (SCRATCH_ITOA, NEWLINE_OFFSET) with
newtype wrappers (MemOffset, StringPoolOffset, HeapPtr).
Prevents the class of bugs where integer constants collide
with data section offsets.

## Tier 3 — Long-term

### 3.1 WASM GC Proposal
### 3.2 WASM Component Model
### 3.3 Profile-Guided Optimization

---

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
