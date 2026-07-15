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

Comparison with 15 established languages using [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) (MiniGit implementation task).

![Execution Time](./figures/lang-bench-time.png?v=1775655978)
![Code Size](./figures/lang-bench-loc.png?v=1775655978)
![Pass Rate](./figures/lang-bench-pass-rate.png?v=1775655978)

> Almide uses Sonnet 4.6 (unknown language); all others use Opus 4.6 (known language). Almide achieves 100% pass rate with fewer lines of code than most languages, despite needing more time due to the model having no prior training data for the language.

## MSR — Modification Survival Rate

The language's core metric, measured daily by [almide-dojo](https://github.com/almide/almide-dojo) across 30 tasks (basic / intermediate / advanced). The headline scorecard lives in the [README](../README.md#msr-scorecard).
