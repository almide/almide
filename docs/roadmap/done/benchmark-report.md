<!-- description: Benchmark comparing LLM code generation cost across 16 languages -->
<!-- done: 2026-03-25 -->
# Benchmark Report: LLM Code Generation Cost by Language

## Goal

Publish a credible benchmark report showing that Almide achieves the lowest LLM code generation cost among 16 languages. This directly validates the mission: "the language LLMs can write most accurately."

## Key Claim

With a warmup session, Almide achieves **$0.19** per task — less than half of Python ($0.49), Go ($0.48), Rust ($0.51), and Haskell ($0.51). Speed is comparable to Python/Go (87s vs 91s).

## Vulnerabilities (must address before publishing)

### V1: Sample size (n=1)
- Current Almide warmup trial: n=1. Not statistically significant.
- **Fix**: Run 10 trials with warmup, 10 without. Report mean ± stddev.

### V2: CLAUDE.md gives Almide extra information
- Almide gets a full language reference (CLAUDE.md). Other languages rely on pre-training only.
- **Counterargument**: Pre-training IS the warmup for known languages. CLAUDE.md compensates for Almide's absence from training data. This is the real-world scenario — new languages always ship docs.
- **Fix**: Acknowledge explicitly. Also run Python/Go WITH a cheatsheet to show the gap persists.

### V3: Warmup fairness
- Warmup (114s, same Claude session) gives Almide a huge advantage. Other languages don't get one.
- **Counterargument**: Known languages benefit from billions of tokens of pre-training. Warmup is a fraction of that.
- **Fix**: Report both warmup and no-warmup numbers. Frame warmup as "first-use cost" that amortizes over many tasks. Also explore: does giving Python a warmup help it too? (Probably not — it already knows Python.)

### V4: Single task (minigit)
- Only one task type: CLI + file I/O. Not representative of all programming.
- **Fix**: Add miniconf (config management) task. Ideally add 1-2 more: data processing, algorithm, web handler.

### V5: All languages pass 100%
- No differentiation on correctness. Every language achieves 100% pass rate.
- **Fix**: Add v3 (breaking change — type change, data structure migration) to measure modification survival rate. This is where Almide's type system should shine.

### V6: Single model (Claude only)
- Results might be Claude-specific.
- **Fix**: Acknowledge as limitation. Optionally run GPT-4o on a subset to check.

## Execution Plan

### Phase 1: Data Collection (you run these)

#### 1.1 Almide trials (warmup)
```bash
cd ~/workspace/github.com/almide/benchmark
ruby benchmark.rb --lang almide --trials 10 --start 35
```
Expected: ~40 min (4 min/trial × 10). Produces trials 35-44.

#### 1.2 Almide trials (no warmup)
```bash
ruby benchmark.rb --lang almide --trials 10 --start 45 --no-warmup
```
Expected: ~50 min. Produces trials 45-54.

#### 1.3 Major languages (fresh data on same machine/model)
```bash
ruby benchmark.rb --lang python,ruby,javascript,go,rust,haskell --trials 5 --start 35
```
Expected: ~2-3 hours. 6 languages × 5 trials.

#### 1.4 Python with cheatsheet (control experiment)
Create a Python cheatsheet (stdlib signatures, common patterns) and add it as extra_files for python. Run 5 trials. This answers "does a cheatsheet help known languages too?"

### Phase 2: Analysis

#### 2.1 Generate report
```bash
ruby report.rb
python3 plot.py
```

#### 2.2 Key metrics to highlight
- **Cost per task** (primary metric): bar chart, all 16 languages + Almide warmup/no-warmup
- **Time per task**: bar chart (secondary)
- **Turn count**: shows how many iterations LLM needed (proxy for "accuracy")
- **v1 vs v2 breakdown**: new project vs modification
- **Modification survival rate**: v1 tests still pass after v2 changes

### Phase 3: Report Writing

#### 3.1 Structure
1. **TL;DR**: Almide costs $0.19/task, Python $0.49, Rust $0.51. 2-3x cheaper.
2. **Motivation**: LLM code generation is real. Language choice affects cost. Nobody has measured this.
3. **Method**: 16 languages, minigit task (CLI + fs), Claude Opus, N trials each.
4. **Results**: Cost table, time table, turn count, plots.
5. **Discussion**: Why Almide is cheaper (clear errors → fewer retries, CLAUDE.md → fewer mistakes). Warmup analysis. Limitations.
6. **Reproduction**: All code is open source. `ruby benchmark.rb --lang almide`.

#### 3.2 Honest framing
- Don't hide warmup cost. Show both numbers.
- Don't hide CLAUDE.md advantage. Explain why it's fair.
- Acknowledge single-task limitation. Commit to expanding.
- Frame as "early results" not "definitive proof."

### Phase 4: Strengthen (after initial report)

#### 4.1 miniconf benchmark
- Already partially built (`benchmark-conf.rb`, `SPEC-conf-v1.txt`, etc.)
- Complete and run across all languages
- Two tasks makes the claim much stronger

#### 4.2 v3 (breaking change)
- Design a v3 spec: change a data type in minigit (e.g., commit hash from string to struct, add metadata field)
- Run on all languages
- Measure: does v1+v2 still pass? How many turns to fix?
- This is where Almide should dominate (type checker catches breakage immediately)

#### 4.3 Warmup optimization
- Analyze warmup logs: what does the LLM learn?
- Bake those learnings into CLAUDE.md
- Target: no-warmup cost < $0.30 (currently $1.01)
- This makes the claim even stronger — no special session needed

## Pre-Publish Checklist

- [ ] 10+ Almide warmup trials, mean ± stddev
- [ ] 10+ Almide no-warmup trials
- [ ] 5+ trials for Python, Ruby, JS, Go, Rust, Haskell (fresh)
- [ ] Python-with-cheatsheet control experiment
- [ ] Plots generated (cost, time, turns)
- [ ] Report written with honest limitations section
- [ ] Reproduction instructions verified (someone can clone and run)
- [ ] All raw data committed to benchmark repo

## Success Criteria

- Almide warmup cost is **statistically significantly lower** than Python (p < 0.05)
- Report is honest enough that critics can't find hidden advantages
- At least 2 task types (minigit + miniconf) before calling it "general"
