# Lang Bench Chart Generation

Regenerate AI Coding Language Benchmark charts from data and update README figures.

## Steps

1. Run the plot script:
   ```bash
   python3 research/benchmark/lang-bench/plot.py
   ```

2. Verify the generated PNGs in `docs/figures/`:
   - `lang-bench-time.png` — Execution time chart
   - `lang-bench-loc.png` — Code size chart
   - `lang-bench-pass-rate.png` — Pass rate chart

3. If data has changed, update `research/benchmark/lang-bench/data.json` first, then re-run

4. Commit: `Update lang-bench charts: {brief summary}`

## Data Source

- `research/benchmark/lang-bench/data.json` — All benchmark data (15 official languages + Almide)
- Official 15 languages: [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) (Opus 4.6, 20 trials)
- Almide: Internal runs (Sonnet 4.6, model differs because Almide has no training data in Opus corpus)

## Notes

- Charts are output to `docs/figures/` and referenced from README's "AI Coding Language Benchmark" section
- Almide is highlighted in orange; all other languages in blue
- To add/remove languages, edit `data.json` and re-run `plot.py`
