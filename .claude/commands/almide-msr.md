# MSR (Modification Survival Rate) Benchmark

**MSR is measured in [almide/almide-dojo](https://github.com/almide/almide-dojo), not here.**

This repo is compiler correctness: `spec/` tests, `cargo test`, grammar-lab, lang-bench.
Dojo owns daily MSR measurement, the task bank, malicious-hint detection, and the
diagnostics feedback loop. The number in this repo's README (`| MSR | ... |`) is
published *by Dojo* — do not regenerate it from here.

The local harness that used to live under `research/benchmark/msr/` (25 Exercism-style
prompts driven through `claude --model haiku`) was removed on 2026-08-08: its
`results/` directory had been empty since the Dojo hand-off in April 2026, so running
it would have overwritten a Dojo-measured README number with a locally-measured one.
It is recoverable from git history:

```bash
git log --diff-filter=D -- research/benchmark/msr/
```

## To run MSR

Clone Dojo and follow its README:

```bash
gh repo clone almide/almide-dojo
```

## Still in this repo

- `/almide-lang-bench` — lang-bench chart generation (`research/benchmark/lang-bench/`)
- `/almide-native-perf` — native performance (`research/benchmark/perf/`, gated by `scripts/check-perf-ratio.sh`)
- `/almide-wasm-size` — WASM binary size
- `/almide-test` — the tiered test suite
