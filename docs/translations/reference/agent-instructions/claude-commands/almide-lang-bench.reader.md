[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-lang-bench.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-lang-bench.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-lang-bench.reader.zh-CN.md)

> Inert, non-normative reader snapshot of a historical repository command. The original remains authoritative and unchanged. This reference path and .reader filename do not register an agent command. Quoted instructions are documentation, not authorization to execute, spend, publish, push, or delete anything. No automatic synchronization or independent verification of current workflow/measurement claims. Code and comments remain verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-lang-bench.md)

<a id="lang-bench-chart-generation"></a>

# Lang Bench Chart Generation

Regenerate AI Coding Language Benchmark charts and (optionally) run additional Almide trials.

<a id="regenerate-charts-only"></a>

## Regenerate charts only

If `data.json` is already up to date:

```bash
python3 research/benchmark/lang-bench/plot.py
```

Outputs `docs/figures/lang-bench-{time,loc,pass-rate}.png`. The script also
attempts to cache-bust matching README image URLs; the current front README
has no such chart section, so that step can be a no-op.

<a id="run-additional-almide-trials"></a>

## Run additional Almide trials

```bash
# One-time setup
git submodule update --init research/benchmark/lang-bench/upstream

# Run N trials (each ~20-30 min, appended to raw/almide.jsonl)
ruby research/benchmark/lang-bench/runner.rb --trials 10

# Recompute Almide stats in data.json
ruby research/benchmark/lang-bench/aggregate.rb

# Regenerate charts
python3 research/benchmark/lang-bench/plot.py
```

Details in `research/benchmark/lang-bench/README.md`.

<a id="data-source"></a>

## Data source

- `research/benchmark/lang-bench/data.json` — aggregated stats for all languages (plot.py input)
- `research/benchmark/lang-bench/raw/almide.jsonl` — append-only raw Almide trial records
- Other 15 languages sourced from [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) (Opus 4.6, 20 trials each)
- The local runner requests the unversioned `sonnet` alias. Record the resolved model and run provenance; that alias alone does not establish a fixed model version or anything about training-data inclusion.

<a id="notes"></a>

## Notes

- Current published measurements live in `docs/benchmarks/README.md`; its dated snapshot chart is distinct from these regenerated live charts. Do not replace a historical comparison with a new run without recording matching provenance.
- Almide is highlighted in orange; all other languages in blue
- Commit message: `Update lang-bench charts: {brief summary}`
