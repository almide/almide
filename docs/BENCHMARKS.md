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

Runtime performance against Rust is tracked per-benchmark on the wasm leg (see
[WASM-OUTPUT.md](./WASM-OUTPUT.md) and the benchmark suite); a native runtime
scoreboard with a stated baseline and repro script is #917 — no number is
published here until it exists.

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
