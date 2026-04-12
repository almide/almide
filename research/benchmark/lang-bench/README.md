# AI Coding Language Benchmark — Almide Runner

Reproduces the [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) minigit task for Almide, and aggregates results into the chart data.

## Layout

| Path | Purpose | Committed? |
|---|---|---|
| `data.json` | Aggregated stats for all languages (plot.py input) | yes |
| `plot.py` | Generates `docs/figures/lang-bench-*.png` | yes |
| `upstream/` | Git submodule → `mame/ai-coding-lang-bench` (SPEC, test scripts) | yes (pointer) |
| `runner.rb` | Runs N trials of the minigit task against Almide | yes |
| `aggregate.rb` | Recomputes Almide stats in `data.json` from `raw/almide.jsonl` | yes |
| `raw/almide.jsonl` | Append-only trial records (one JSON per line) | yes |
| `.work/`, `.logs/` | Per-trial generated sources and Claude logs | no (gitignored) |

## Setup

```bash
git submodule update --init research/benchmark/lang-bench/upstream
```

Requires: `claude` CLI, `almide` in PATH, `ruby`, `python3` + `matplotlib` (for plot).

## Run trials

```bash
ruby research/benchmark/lang-bench/runner.rb --trials 10
```

Each trial runs two phases:

- **v1** — Claude (Sonnet 4.6) implements `init`, `add`, `commit`, `log`.
- **v2** — Claude extends v1 with `checkout`, `reset`.

Expect ~20–30 min per trial. Trial numbers auto-increment based on existing records in `raw/almide.jsonl`.

Dry run (no LLM calls):

```bash
ruby research/benchmark/lang-bench/runner.rb --trials 1 --dry-run
```

## Update charts

```bash
ruby research/benchmark/lang-bench/aggregate.rb   # raw/ → data.json Almide entry
python3 research/benchmark/lang-bench/plot.py     # data.json → docs/figures/lang-bench-*.png
```

`plot.py` also cache-busts the README image URLs.

## Data notes

- The other 15 languages in `data.json` come from the upstream benchmark (Opus 4.6, 20 trials each). Those numbers are not regenerated here.
- Almide uses **Sonnet 4.6** because Almide has no Opus training data. The model difference is documented in the chart label (`Almide (sonnet)`).
- `upstream/benchmark.rb` is the vanilla upstream runner — it does **not** include Almide in its language list. `runner.rb` in this directory is our Almide-specific replacement, referencing only `upstream/SPEC-v*.txt` and `upstream/test-v*.sh`.
