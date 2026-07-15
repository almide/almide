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
- The Almide entry in `data.json` `languages[]` was refreshed on 2026-07-15 from the same-model snapshot below (**Sonnet 5**, 20 trials). Almide never uses Opus because it has no training data; the model difference is documented in the chart label.
- `upstream/benchmark.rb` is the vanilla upstream runner — it does **not** include Almide in its language list. `runner.rb` in this directory is our Almide-specific replacement, referencing only `upstream/SPEC-v*.txt` and `upstream/test-v*.sh`.

## Same-model snapshot (2026-07)

A one-shot, same-conditions comparison of Almide against its modern peer group
(Gleam, MoonBit) plus two mainstream anchors (Rust, TypeScript): **one model
(`claude-sonnet-5`), 20 trials per language, identical prompts and harness**,
run 2026-07-15 via `runner_multi.rb`. This removes the model mismatch that the
historic chart documents in its labels.

| Language | Toolchain | v1 pass | v2 pass | avg time (s) | v2 LOC | avg cost |
|---|---|---|---|---|---|---|
| Almide | 0.29.0 | 20/20 | **20/20** | 573 ± 112 | **233 ± 27** | $3.19 |
| Gleam | 1.15.2 | 20/20 | 19/20 | 715 ± 258 | 394 ± 27 | $2.66 |
| MoonBit | moon 0.1.20260330 | 20/20 | 20/20 | 962 ± 358 | 448 ± 54 | $5.28 |
| Rust | rustc 1.94.1 | 20/20 | 20/20 | 297 ± 89 | 351 ± 18 | $1.45 |
| TypeScript | tsx 4.23.1 | 20/20 | 20/20 | 297 ± 99 | 386 ± 138 | $1.44 |

Chart: `docs/figures/lang-bench-snapshot-2026-07.png`. Aggregated data lives in
`data.json` under `snapshot_2026_07`; raw per-trial records in
`raw/<lang>-sonnet5.jsonl` (committed, force-added past the `raw/*` ignore).

Methodology notes:

- Prompts are the upstream `benchmark.rb` prompts verbatim. Almide additionally
  receives `CHEATSHEET.md` in its work dir and a build-command hint (it is the
  only language absent from training data — it learns the language in-context).
- **Infra-retry policy**: trials where the `claude` CLI died before attempting
  the task (≤4 agent turns, 0 LOC written, zero tests executed) were re-run
  once; the original records are quarantined in `raw/retired/` with reasons.
  4/100 trials were affected (rust 5 & 11, gleam 16, moonbit 8). Genuine model
  failures were never retried.
- The single genuine failure in the snapshot is Gleam trial 20 v2: the model's
  `checkout`/`reset` implementation fails 12 of 26 executed tests (the harness
  recorded 0/0 because the suite aborted before its summary line; a manual
  re-run confirmed the failures — see `v2_note` on the record).
- Time figures were measured under 6–9× self-parallelism (all languages ran
  concurrently), so they are comparable within the snapshot but conservative
  versus the sequential upstream numbers.

Reproduce:

```bash
ruby runner_multi.rb --lang gleam --trials 20      # one language
ruby aggregate_multi.rb                            # summary table
python3 plot_snapshot.py                           # regenerate the chart
```
