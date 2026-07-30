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

The "as shipped" column is raw `almide build --target wasm` output (measured 2026-07-23). Running `wasm-opt` is an explicit opt-in that leaves the verified envelope — it goes beyond the renderer's own reachability DCE with instruction-level rewrites (local coalescing, inlining, more aggressive dead-code removal). The float row is dominated by the self-hosted Dragon4 shortest-round-trip printer that `float.to_string` demand-links; programs that never display a Float never pay for it. Full dissection: [WASM-OUTPUT.md](./WASM-OUTPUT.md).

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
[research/benchmark/perf/bench.py](../research/benchmark/perf/README.md); raw
per-run data: [results/2026-07-30-m4pro.json](../research/benchmark/perf/results/2026-07-30-m4pro.json).

| Benchmark (workload) | Almide native | Handwritten Rust | Ratio |
|---|---:|---:|---:|
| n-body (50M steps) | **1.135s** | 1.134s same-shape / 1.552s array-based | **1.00×** / 0.73× |
| spectral-norm (n=5500) | **0.685s** | 0.685s | **1.00×** |
| fasta (25M) | **3.590s** | 3.083s | 1.16× |
| FFT (2^22) | **0.181s** | 0.143s | 1.27× |

Almide beats the array-based n-body reference because its unrolled-scalar idiom
compiles to bounds-check-free locals; the same-shape reference isolates pure
codegen overhead (≤1%). The `perf-ratchet` CI job
([scripts/check-perf-ratio.sh](../scripts/check-perf-ratio.sh)) gates these
ratios against a committed baseline so they can only move on purpose.

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

![Same-model snapshot](./figures/lang-bench-snapshot-2026-07.png?v=1784109014)

> Almide is the only language absent from training data (the model learns it in-context from CHEATSHEET.md), yet it passes 40/40 phases, produces the most concise code of all five languages (233 LOC), and completes faster than both modern peers. Methodology, retry policy, and raw per-trial records: [research/benchmark/lang-bench](../research/benchmark/lang-bench/README.md).

### Historic comparison vs 15 established languages

![Execution Time](./figures/lang-bench-time.png?v=1784109014)
![Code Size](./figures/lang-bench-loc.png?v=1784109014)
![Pass Rate](./figures/lang-bench-pass-rate.png?v=1784109014)

> The Almide row was refreshed 2026-07-15 (Sonnet 5, 20 trials, from the snapshot above); the other 15 languages use the upstream Opus 4.6 runs. Almide achieves 100% pass rate with fewer lines of code than most languages, despite needing more time because the model has no prior training data for the language.

## MSR — Modification Survival Rate

The language's core metric, measured daily by [almide-dojo](https://github.com/almide/almide-dojo) across 30 tasks (basic / intermediate / advanced). The headline scorecard lives in the [README](../README.md#msr-scorecard).
