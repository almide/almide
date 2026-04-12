# Lang Bench Chart Generation

Regenerate AI Coding Language Benchmark charts and (optionally) run additional Almide trials.

## Regenerate charts only

If `data.json` is already up to date:

```bash
python3 research/benchmark/lang-bench/plot.py
```

Outputs `docs/figures/lang-bench-{time,loc,pass-rate}.png` and cache-busts README image URLs.

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

## Data source

- `research/benchmark/lang-bench/data.json` — aggregated stats for all languages (plot.py input)
- `research/benchmark/lang-bench/raw/almide.jsonl` — append-only raw Almide trial records
- Other 15 languages sourced from [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) (Opus 4.6, 20 trials each)
- Almide: Sonnet 4.6 (Almide has no Opus training data)

## Notes

- Charts are referenced from README's "AI Coding Language Benchmark" section
- Almide is highlighted in orange; all other languages in blue
- Commit message: `Update lang-bench charts: {brief summary}`
