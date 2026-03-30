<!-- description: LLM accuracy benchmarks comparing Almide, Python, and MoonBit -->
# LLM Benchmark Execution

**Priority:** High — Proving Almide's raison d'etre: "the language LLMs can write most accurately"
**Prerequisites:** benchmark/ framework + msr/ tool already built

---

## Write-from-Scratch Results (2026-03-25, Haiku, n=1)

| Language | Score | Cheatsheet | Training data |
|----------|-------|------------|---------------|
| **Almide** | 24/24 (100%) | Yes (449 lines) | Near zero |
| **Python** | 25/25 (100%) | No | Massive |
| **MoonBit** | 24/24 (100%) | No | Limited |

**Conclusion:** At this difficulty level, no meaningful difference emerges. Almide matches Python with zero training data + CHEATSHEET alone.

## Modification Survival Rate (MSR) Results (2026-03-30, Haiku, n=3)

10 modification tasks across 6 categories: variant addition, record field addition, return type change, error handling, function addition, behavioral change.

| Language | Run 1 | Run 2 | Run 3 | Mean | Tasks |
|----------|-------|-------|-------|------|-------|
| **Almide** | 10/10 (100%) | 9/10 (90%) | 10/10 (100%) | **96.7%** | 10 |
| **Python** | 8/9 (88%) | 7/9 (77%) | 9/9 (100%) | **88.9%** | 9 (no M04) |

### Per-task stability (across 3 runs)

| Task | Category | Almide | Python |
|------|----------|--------|--------|
| M01 traffic-light variant | Variant add | 3/3 | 3/3 |
| M02 todo-app field | Record field | 3/3 | 3/3 |
| M03 expr-eval variant | Variant add | 3/3 | 2/3 (type union `\|` on 3.9) |
| M04 expr-eval return type | Return type change | 3/3 | N/A |
| M05 todo-app error handling | Effect fn | 3/3 | 3/3 |
| M06 config-merger add fn | Function add | 3/3 | 3/3 |
| M07 grade-report add fn | Function add + change | 2/3 (sort_by API confusion) | 3/3 |
| M08 bob behavioral | Behavioral (control) | 3/3 | 3/3 |
| M09 expr-eval compound | Compound change | 3/3 | 2/3 (type union `\|` on 3.9) |
| M10 traffic-light compound | Compound change | 3/3 | 3/3 |

### Key findings

1. **Almide's type system provides measurable advantage in modification tasks** — exhaustive match, effect fn, and Result types guide LLMs to correct modifications
2. **Python's weaknesses are non-deterministic** — the `X | Y` type union syntax fails on Python 3.9, and LLMs inconsistently use it
3. **Almide's sole failure (M07 run 2)** was sort_by API confusion (comparator fn vs key fn) — addressable via CHEATSHEET improvement
4. **Python lacks M04 equivalent** — Result type changes have no Python counterpart, which is itself a data point

## Tools

- `research/benchmark/msr/scripts/run-modifications.sh` — Almide MSR runner
- `research/benchmark/msr/python/modifications/run.sh` — Python MSR runner
- Results: `research/benchmark/msr/modifications/results/` (JSON per run)
- Results: `research/benchmark/msr/python/modifications/results/` (JSON per run)

## Next Steps

### Phase 1: Statistical Significance (n=10)
- [x] Run n=3 for both languages
- [ ] Run n=10 for statistical significance (Fisher's exact test)
- [ ] Sonnet model comparison

### Phase 2: Harder Problems
- [ ] Multi-module coordination, error propagation chains, complex generics usage
- [ ] Add production-scale problems (hundreds of lines)

### Phase 3: Analysis and Publication
- [ ] Aggregation, statistical significance testing
- [ ] Publish results on README / website
