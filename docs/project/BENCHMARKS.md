# Benchmarks

## WASM Binary Size

Almide emits WASM bytecode directly (no LLVM, no Cranelift). Each binary is self-contained — allocator, string handling, and runtime are all included. No external GC or host runtime dependency. Since the verified (PCC) pipeline became the sole wasm path, **the shipped binary is the exact module the certificate was checked against**: reachability DCE inside the renderer prunes unreached preamble helpers, imports, and data segments before assembly, and the debug-name section keeps only function names (for trap backtraces) — but no post-hoc optimizer touches the shipped bytes.

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

The ~1.6× all three shapes share against handwritten Rust is the
materialization cost tracked in #1004 — closing the shape spread showed that
what remains is not about the shape. `check-perf-ratio.sh` gates each row's
ratio and, separately, the relation between the rows, so the idiom cannot
silently become the slow path again.

The wasm leg is measured in the same dated results file: within 1.1–1.2× of
native on the compute kernels (n-body 1.278s, spectral-norm 0.764s,
fannkuch-redux 1.892s) and *faster* than native on binary-trees (0.239s vs
0.835s), but it craters on hot list index writes (FFT: ~3,500× at 2^18) and on
mandelbrot (~130×) — those two cliffs are the current wasm perf arc, tracked in
#917's follow-up.

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
