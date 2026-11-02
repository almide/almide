[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-msr.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-msr.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-msr.reader.zh-CN.md)

> Inert, non-normative reader snapshot of a historical repository command. The original remains authoritative and unchanged. This reference path and .reader filename do not register an agent command. Quoted instructions are documentation, not authorization to execute, spend, publish, push, or delete anything. No automatic synchronization or independent verification of current workflow/measurement claims. Code and comments remain verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-msr.md)

<a id="msr-modification-survival-rate-benchmark"></a>

# MSR (Modification Survival Rate) Benchmark

**MSR is measured in [almide/almide-dojo](https://github.com/almide/almide-dojo), not here.**

This repo is compiler correctness: `spec/` tests, `cargo test`, grammar-lab, lang-bench.
Dojo owns daily MSR measurement, the task bank, malicious-hint detection, and the
diagnostics feedback loop. The number on this repo's measurements page (`docs/benchmarks/README.md`) is
published *by Dojo* — do not regenerate it from here.

The local harness that used to live under `research/benchmark/msr/` (25 Exercism-style
prompts driven through `claude --model haiku`) was removed on 2026-08-08: its
`results/` directory had been empty since the Dojo hand-off in April 2026, so running
it would have overwritten a Dojo-measured README number with a locally-measured one.
It is recoverable from git history:

```bash
git log --diff-filter=D -- research/benchmark/msr/
```

<a id="to-run-msr"></a>

## To run MSR

Clone Dojo and follow its README:

```bash
gh repo clone almide/almide-dojo
```

<a id="still-in-this-repo"></a>

## Still in this repo

- `/almide-lang-bench` — lang-bench chart generation (`research/benchmark/lang-bench/`)
- `/almide-native-perf` — native performance (`research/benchmark/perf/`, gated by `scripts/check-perf-ratio.sh`)
- `/almide-wasm-size` — WASM binary size
- `/almide-test` — the tiered test suite
