# Benchmarks

## WASM Binary Size

Almide emits WASM bytecode directly (no LLVM, no Cranelift). Each binary is self-contained — allocator, string handling, and runtime are all included. No external GC or host runtime dependency. Aggressive DCE strips unused runtime functions and data automatically.

| Program | Size |
|---------|-----:|
| Hello World | **467 B** |
| FizzBuzz | **809 B** |
| Fibonacci (recursive) | **682 B** |
| Closure + call_indirect | **812 B** |
| Variant (match + float) | **1,105 B** |

These are raw `almide build --target wasm` output — no post-processing. `wasm-opt -O3` saves only 1–5 more bytes because the compiler's built-in dead code and dead data elimination already strips everything unused.

## Native Performance

Almide compiles to Rust, which then compiles to native machine code. No runtime, no GC, no interpreter.

| Metric | Value |
|--------|-------|
| Binary size (minigit CLI) | **444 KB** (stripped) |
| Runtime (100 ops) | **1.1s** |
| Dependencies | **0** (single static binary) |
| WASM target | `almide build app.almd --target wasm` |

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
