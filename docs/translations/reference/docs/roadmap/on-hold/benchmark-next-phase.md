> Source description: LLM benchmark Phase 2-3: cross-language comparison, harder problems, publication
<a id="llm-benchmark-next-phase"></a>

# LLM Benchmark: Next Phase

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](benchmark-next-phase.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](benchmark-next-phase.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](benchmark-next-phase.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/benchmark-next-phase.md)
> Historical observations lack run/model versions in this record. Keep the old n≥10 target, proposed tests, effort estimates and hoped-for advantages distinct from statistical sufficiency, independence, current commitments or demonstrated superiority. Dojo owns any resumed experiment; no old local harness is restored here.


This records an earlier benchmark/publication plan and its reported Phase 1
observations. It is not a runnable current benchmark guide. Per
[CLAUDE.md](../../../../../../CLAUDE.md), MSR collection belongs to
[Almide Dojo](https://github.com/almide/almide-dojo); the local harness was removed
on 2026-08-08. Reusing this research plan requires Dojo-owned runs and provenance,
not recreating a competing local MSR number.

---

<a id="historical-reported-data-runmodel-versions-not-recorded-here"></a>

## Historical reported data (run/model versions not recorded here)

| Language | Model | n | Mean MSR | Range |
|----------|-------|---|----------|-------|
| Almide | Haiku | 14 | 92.1% | 80–100% |
| Almide | Sonnet | 9 | 92.2% | 70–100% |
| Python | Haiku | 3 | 88.9% | 77–100% |

**Gap:** Python data is insufficient (n=3) for statistical comparison. Need n≥10.

---

<a id="phase-2a-python-n10-cross-language-comparison"></a>

## Phase 2A: Python n=10 (cross-language comparison)

**Goal:** Determine if Almide's MSR advantage over Python is statistically significant.

**Tasks:**
1. Run Python MSR benchmark n=10 with Haiku
   - Original script location: `research/benchmark/msr/python/modifications/run.sh` (removed; not a current command). Define any new run in Dojo.
   - ~30 min (10 runs × 9 tasks × ~20s per task)
2. Run Python MSR benchmark n=10 with Sonnet
3. Apply Fisher's exact test per-task and aggregate
4. Report: confidence interval, p-value, effect size

**Original hypothesis:** an ~8–12% gap. This is not a measured result or a
power guarantee. Choose the sampling unit and analysis before collection;
repeated tasks/runs are not automatically independent observations.

**Risk:** If Python also hits 90%+, the gap may not be significant at this difficulty level. That's a valid finding — it means harder problems are needed to separate.

---

<a id="phase-2b-harder-problems"></a>

## Phase 2B: Harder Problems

**Goal:** Create modification tasks where Almide's type system provides a larger measurable advantage.

**Problem categories that stress type systems:**

| Category | Why Almide should win | Example |
|----------|----------------------|---------|
| **Multi-module refactor** | Compiler catches all call sites; Python relies on grep | Rename a type used across 3 modules |
| **Error propagation chain** | Explicit `Result` handling/`!` propagation vs manual try/except | Add a new error case that propagates through 4 functions |
| **Generic type change** | `List[A]` → `Map[String, A]`; all consumers must update | Change a function's return type from List to Map |
| **Exhaustiveness enforcement** | Adding a variant exposes affected non-exhaustive matches (wildcard matches may remain exhaustive) | Add 3 variants to a 5-case enum used in 4 functions |
| **Required record field addition** | Type checker catches missing fields | Add a required field to a record used in 10 places |
| **Effect boundary crossing** | Pure fn accidentally calling effect fn = compile error | Refactor to move IO into a dedicated effect fn |

**Difficulty tiers:**
- Tier 1 (original baseline): Single-file, 50-80 lines, 1-2 type changes — MSR ~92%
- Tier 2 (target): Single-file, 100-200 lines, 3-5 cascading changes
- Tier 3 (stretch): Multi-file, 200-500 lines, architectural modifications

**Implementation:**
1. Write 10 Tier 2 tasks (v1 solution + modification instruction + v2 tests)
2. Add tasks to the Dojo-owned task bank; `research/benchmark/msr/modifications/prompts/` was the removed local location
3. Run n=10 for both Almide and Python
4. Compare per-tier MSR

---

<a id="phase-3-publication"></a>

## Phase 3: Publication

**Goal:** Publish a reproducible comparison scoped to the measured tasks, models
and language versions, including negative or inconclusive results.

**Deliverables:**
1. **README section** — 3-sentence summary with key numbers
2. **Blog post / technical report** — methodology, raw data, statistical analysis
3. **Reproducibility package** — scripts + prompts + raw outputs published with the Dojo-owned experiment

**Statistical methodology:**
- Per-task: Fisher's exact test (2×2 table: pass/fail × language)
- Aggregate: Paired proportion test (McNemar's or stratified Fisher)
- Report: p-value, 95% CI for difference, effect size (Cohen's h)
- Transparency: all raw LLM outputs committed, full reproducibility

**Credibility checklist:**
- [ ] n ≥ 10 for all conditions
- [ ] Multiple models (Haiku + Sonnet minimum)
- [ ] Multiple difficulty tiers
- [ ] Python comparison with identical task structure
- [ ] Raw data publicly available
- [ ] Statistical test with p-value reported

---

<a id="execution-order"></a>

## Execution Order

1. **Phase 2A** first — re-establish the experiment in Dojo; the original local infrastructure is removed
2. **Phase 2B** — requires problem design (1-2 days)
3. **Phase 3** — after data is complete

**Original effort estimate:** 3–5 days; not a current commitment.
**Before resuming:** agree on Dojo ownership, locate the historical raw data,
and define the new experiment. No current external run status is asserted here.
