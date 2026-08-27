# Benchmarks

## WASM Binary Size

Almide emits WASM bytecode directly (no LLVM, no Cranelift). Each binary is self-contained — allocator, string handling, and runtime are all included. No external GC or host runtime dependency. Since the unverified v0 emitter was retired (#782), every wasm build comes from a verified renderer — the certified MIR spine, or since commissioning (#1599) the structural engine, whichever the router picks — and **the shipped binary is the exact module that renderer produced** (on the incumbent leg, the exact module the certificate was checked against): reachability DCE inside the renderer prunes unreached preamble helpers, imports, and data segments before assembly, and the debug-name section keeps only function names (for trap backtraces) — but no post-hoc optimizer touches the shipped bytes.

| Program | Verified, as shipped | After `wasm-opt -Oz --all-features` |
|---------|-----:|-----:|
| Hello World | **703 B** | **545 B** |
| FizzBuzz 1–100 | **1,793 B** | **1,092 B** |
| Fibonacci (recursive) | **1,441 B** | **771 B** |
| Closure + call_indirect | **2,744 B** | **1,672 B** |
| Variant (match + float) | **11,965 B** | **6,868 B** |

The "as shipped" column is raw `almide build --target wasm` output (measured 2026-07-23). Running `wasm-opt` is an explicit opt-in that leaves the verified envelope — it goes beyond the renderer's own reachability DCE with instruction-level rewrites (local coalescing, inlining, more aggressive dead-code removal). The float row is dominated by the self-hosted Dragon4 shortest-round-trip printer that `float.to_string` demand-links; programs that never display a Float never pay for it. Full dissection: [WASM-OUTPUT.md](../wasm/WASM-OUTPUT.md).

## Native Performance

Almide compiles to Rust, which then compiles to native machine code. No runtime, no GC, no interpreter.

| Metric | Value |
|--------|-------|
| Binary size (minigit CLI) | **418 KB** (stripped) |
| Dependencies | **0** (single static binary) |
| WASM target | `almide build app.almd --target wasm` |

### Runtime scoreboard (2026-07-30, Apple M4 Pro, rustc 1.96.1)

Benchmarks-Game-style programs, Almide `--release` vs handwritten Rust compiled
with the same flags (opt-level=3, LTO, 1 CGU), median of 5 interleaved runs,
byte-identical stdout verified across every variant before timing. Produced by
[research/benchmark/perf/bench.py](../../research/benchmark/perf/README.md); raw
per-run data: [results/2026-07-30-m4pro.json](../../research/benchmark/perf/results/2026-07-30-m4pro.json),
and for the rows dated 2026-08-13 below,
[results/2026-08-13-m4pro.json](../../research/benchmark/perf/results/2026-08-13-m4pro.json).

| Benchmark (workload) | Almide native | Handwritten Rust | Ratio |
|---|---:|---:|---:|
| n-body (50M steps) | **1.135s** | 1.134s same-shape / 1.552s array-based | **1.00×** / 0.73× |
| spectral-norm (n=5500) | **0.685s** | 0.685s | **1.00×** |
| fasta (25M) | **3.590s** | 3.083s | 1.16× |
| FFT (2^22) † | **0.180s** | 0.153s | 1.18× |

† **The FFT row was re-measured 2026-08-13 and its number moved on purpose.**
It previously built its 8.4M-element input with `data = data + [x]` inside a
loop, so roughly 8% of the row's wall clock was list construction reported as
codegen — a benchmark partly measuring its own setup (#1338). It now
preallocates and writes by index, the same shape as the reference's
`Vec::with_capacity` + push, which is the fairness rule the suite already
applies to `nbody_unrolled.rs`. A/B in one interleaved run of both binaries
plus the reference, 2^22, median of 11, repeated three times (before
194.8 / 199.9 / 193.6 ms, after 179.9 / 184.0 / 176.8 ms):
**0.195s → 0.180s wall clock, 1.27× → 1.18×**. The transform is bit-identical
across the change, verified out of band at 2^12 / 2^16 / 2^20 on a
position-weighted checksum of the whole array. The other three rows are the
2026-07-30 run, unchanged.

Almide beats the array-based n-body reference because its unrolled-scalar idiom
compiles to bounds-check-free locals; the same-shape reference isolates pure
codegen overhead (≤1%). The `perf-ratchet` CI job
([scripts/check-perf-ratio.sh](../../scripts/check-perf-ratio.sh)) gates these
ratios against a committed baseline so they can only move on purpose.

### Build shape: what the recommended idiom costs (2026-08-13)

`listbuild` is one workload written three ways — the same 2^23-element
interleaved `Float` array, identical arithmetic, identical checksum consumer,
differing only in the build loop. It exists because the shape the idiom docs
recommend was the slowest of the three, which is an MSR problem before it is a
perf problem: the idiom docs are the in-context material that steers generated
code, so "recommended" and "fast" have to name the same shape.

| Build shape (2^23) | Almide native | Handwritten Rust | Ratio |
|---|---:|---:|---:|
| preallocate + indexed write | **0.166s** | 0.105s | 1.57× |
| `var` + `for` + `data = data + [x]` | **0.176s** | 0.110s | 1.60× |
| `list.range \|> list.flat_map` (recommended) | **0.172s** | 0.105s | 1.64× |

Median of 15 interleaved runs; all three shapes produce byte-identical output,
checked before timing. The three ratios are within run-to-run noise of each
other on this box — the point of the table is that the spread is gone.

**Read the ratio column as machine-specific, not as a property of the
language.** Unlike the arithmetic kernels above, this workload's cost is
allocation, and the two allocators disagree: the same commit measured on the
ubuntu-latest CI runner reads **0.91×** on the preallocated row — Almide
*beating* the reference, because glibc `calloc` hands back zero pages that
`Vec::with_capacity` + push has to fault in — against 1.57× here. What is
stable across both machines, and what CI therefore gates, is the *relation*
between the three shapes (1.018× here, 1.045× on the runner), not any row's
absolute ratio.

It was not. The A/B at 2^22 (median of 11, same run):

| Build shape (2^22) | Before | After |
|---|---:|---:|
| preallocate + indexed write | 0.084s | 0.083s |
| `var` + `for` + append | 0.084s | 0.083s |
| `list.range \|> list.flat_map` | **0.140s** | **0.085s** |

`flat_map`'s lambda was forced by its runtime signature to heap-allocate its
intermediate list on every element — 44 ms of a 79 ms build, the whole of the
gap. It now returns a fixed-size array instead, and the recommended idiom went
from **1.67× the append loop it replaces to 1.02×**.

The ~1.6× all three shapes share against handwritten Rust was long attributed
to materialization (#1004). It is **not** (measured 2026-08-13): swapping only
`almide_rt_libm_sin`/`_cos` — the deterministic software libm the cross-target
byte-identity contract requires — for the platform ones in the emitted Rust
takes the row from 194.5 ms to 105.5 ms against a 123.9 ms reference. The
whole 1.6× is transcendental determinism, and without it the emitted code
*beats* the handwritten reference by 1.17×. The build shape is innocent:
`list.repeat` + bounds-checked indexed writes measure 75.5 ms against
`Vec::with_capacity` + push at 90.6 ms. Evidence and method:
[string-gap-1004.md](../../research/benchmark/perf/string-gap-1004.md).
`check-perf-ratio.sh` reports these rows and gates the relation between them,
so the idiom cannot silently become the slow path again.

### Strings: what the stdlib's contract costs (2026-08-13)

`strchurn` is the allocation-heavy string workload from #1004:
`int.to_string` → `string.join` → `string.split` → `string.len` → `list.sum`.
It carries two references, and which one you compare against is the whole
story (`bench.py --quick`, 2M, median of 3, M4 Pro):

| strchurn (2M) | Time | Ratio |
|---|---:|---:|
| Almide native | **0.123s** | — |
| Rust, same shape + same semantics (owned `String`s from `split`, `chars().count()`) | 0.109s | **1.12×** |
| Rust, idiomatic (borrowed `&str`, byte `len()`) | 0.059s | 2.08× |

The second reference does what a Rust programmer writes; the first does what
Almide's stdlib obliges the program to do. Almide is within **1.12×** of
same-work Rust, and the remaining spread to idiomatic Rust is **the API
contract, not codegen** — 75% of it is `string.split` returning owned
`String`s (`List[String]` has no borrowed element type) and 11% is `string.len`
being a character count. The `RcCow` representation the issue title blamed is
not on this path at all: Almide's `String` lowers to Rust's `String`, and
`RcCow` is used only for `Bytes` and `Matrix`. Full ladder, the refuted
rlib-boundary hypothesis, and the resulting work list:
[string-gap-1004.md](../../research/benchmark/perf/string-gap-1004.md).
Like the listbuild rows, this one is reported rather than anchored — an
allocation-dominated ratio is an allocator comparison first.

The wasm leg is measured in the same dated results file: within 1.1–1.2× of
native on the compute kernels (n-body 1.278s, spectral-norm 0.764s,
fannkuch-redux 1.892s) and *faster* than native on binary-trees (0.239s vs
0.835s), but it craters on hot list index writes (FFT: ~3,500× at 2^18) and on
mandelbrot (~130×) — those two cliffs are the current wasm perf arc, tracked in
#917's follow-up.

### Ablation: what the IR optimizer buys (2026-08-18)

Koka's benchmark methodology publishes ablation legs (`std` = no FIP reuse
optimization) so a performance claim is about the *optimizer*, not just the
language — `#1466` adopts the same axis. `ALMIDE_DISABLE_OPT=1` skips the IR
perf passes (constant fold, DCE, constant propagation; the lowering enablers
and the unsigned-lane correctness re-fold stay), and the perf gate runs every
anchored benchmark both ways each CI round. The delta (ablated / optimized)
is anchored in `scripts/perf-ratio-baseline.txt` with a floor at 0.90 and the
standard re-anchor-on-purpose ceiling.

| bench | ablated / optimized |
|---|---:|
| nbody | 1.00 |
| spectralnorm | 1.00 |
| fasta | 1.00 |
| fft | 1.00 |

**The measured finding, stated plainly:** on the rustc-backed native leg the
IR perf passes buy ~0% on these benchmarks — LLVM runs the same folds
downstream and subsumes them. That is the honest ablation answer today, and
it is exactly what the axis is FOR: a pass that earns will show up as the
delta leaving 1.00 (and gets re-anchored, visibly); a pass that starts
*costing* trips the 0.90 floor. The passes' present value is compile-time
shape and the wasm leg (which has no LLVM behind it — ablating there costs a
file its lowering, not its speed; see `optimize_program`'s header note).

## Edit-loop scale (#1334)

The README's build-speed row is measured on a 268-line file, which does not establish
that the edit loop is *scale-independent* — a fast check on a small file is also exactly
what a quadratic compiler produces. So the same command is measured over a ladder of
nested prefixes of this repo's own stdlib (303 hand-written modules, 30,624 lines;
nothing synthesized), 40 interleaved runs per rung:

| project size | `almide check` p50 | p95 | µs/line |
|---:|---:|---:|---:|
| 2,047 lines | 18.4 ms | 19.5 ms | 2.87 |
| 5,456 lines | 31.2 ms | 32.3 ms | 3.29 |
| **10,151 lines** | **52.1 ms** | **53.3 ms** | 3.78 |
| 20,378 lines | 97.4 ms | 99.5 ms | 4.06 |
| 30,624 lines | 141.5 ms | 145.1 ms | 4.06 |

arm64 Darwin, almide 0.57.0, 2026-08-13, load average 6.7. **The p95 column is an
observation, not a promise**: the same commit on the same box at load average 42 read
p95 332 ms at the 10,151-line rung — 6× — so the CI ratchet
(`scripts/check-edit-loop-scale.sh`) anchors two dimensionless same-run ratios instead:
the log-log slope of marginal check time against project lines (**1.13**, where 1.0 is
linear and 2.0 quadratic; it moved 0.6% across that 7× load swing) and the cost of the
10k-line rung in units of the empty-project floor (**4.4×**). Reproduce with
`python3 research/benchmark/editloop/scale.py`.

## AI Coding Language Benchmark

Based on [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) (MiniGit implementation task: v1 implement, v2 extend).

### Same-model snapshot (2026-07)

Five languages, one model (Claude Sonnet 5), 20 trials each, identical prompts and harness — Almide vs its modern peer group (Gleam, MoonBit) plus mainstream anchors (Rust, TypeScript):

![Same-model snapshot](../figures/lang-bench-snapshot-2026-07.png?v=1784109014)

> Almide is the only language absent from training data (the model learns it in-context from CHEATSHEET.md), yet it passes 40/40 phases, produces the most concise code of all five languages (233 LOC), and completes faster than both modern peers. Methodology, retry policy, and raw per-trial records: [research/benchmark/lang-bench](../../research/benchmark/lang-bench/README.md).

### Historic comparison vs 15 established languages

![Execution Time](../figures/lang-bench-time.png?v=1784109014)
![Code Size](../figures/lang-bench-loc.png?v=1784109014)
![Pass Rate](../figures/lang-bench-pass-rate.png?v=1784109014)

> The Almide row was refreshed 2026-07-15 (Sonnet 5, 20 trials, from the snapshot above); the other 15 languages use the upstream Opus 4.6 runs. Almide achieves 100% pass rate with fewer lines of code than most languages, despite needing more time because the model has no prior training data for the language.

## MSR — Modification Survival Rate

The language's core metric, measured daily by [almide-dojo](https://github.com/almide/almide-dojo) across 30 tasks (basic / intermediate / advanced). The headline scorecard lives in the [README](../../README.md#msr-scorecard).
