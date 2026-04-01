<!-- description: LLM benchmark Phase 2-3: cross-language comparison, harder problems, publication -->
# LLM Benchmark: Next Phase

Phase 1 (Almide single-language MSR, n≥10) is complete. This document plans the remaining phases to produce publishable, statistically rigorous results.

---

## Current Data

| Language | Model | n | Mean MSR | Range |
|----------|-------|---|----------|-------|
| Almide | Haiku | 14 | 92.1% | 80–100% |
| Almide | Sonnet | 9 | 92.2% | 70–100% |
| Python | Haiku | 3 | 88.9% | 77–100% |

**Gap:** Python data is insufficient (n=3) for statistical comparison. Need n≥10.

---

## Phase 2A: Python n=10 (cross-language comparison)

**Goal:** Determine if Almide's MSR advantage over Python is statistically significant.

**Tasks:**
1. Run Python MSR benchmark n=10 with Haiku
   - Script: `research/benchmark/msr/python/modifications/run.sh`
   - ~30 min (10 runs × 9 tasks × ~20s per task)
2. Run Python MSR benchmark n=10 with Sonnet
3. Apply Fisher's exact test per-task and aggregate
4. Report: confidence interval, p-value, effect size

**Expected outcome:** ~8-12% gap (Almide 92% vs Python ~85-89%). With n=10×10 tasks, Fisher's test should yield p < 0.05 if the true difference is ≥10%.

**Risk:** If Python also hits 90%+, the gap may not be significant at this difficulty level. That's a valid finding — it means harder problems are needed to separate.

---

## Phase 2B: Harder Problems

**Goal:** Create modification tasks where Almide's type system provides a larger measurable advantage.

**Problem categories that stress type systems:**

| Category | Why Almide should win | Example |
|----------|----------------------|---------|
| **Multi-module refactor** | Compiler catches all call sites; Python relies on grep | Rename a type used across 3 modules |
| **Error propagation chain** | `effect fn` + auto-`?` vs manual try/except | Add a new error case that propagates through 4 functions |
| **Generic type change** | `List[A]` → `Map[String, A]`; all consumers must update | Change a function's return type from List to Map |
| **Exhaustiveness enforcement** | Adding a variant forces updating ALL matches | Add 3 variants to a 5-case enum used in 4 functions |
| **Record field addition with default** | Type checker catches missing fields | Add a required field to a record used in 10 places |
| **Effect boundary crossing** | Pure fn accidentally calling effect fn = compile error | Refactor to move IO into a dedicated effect fn |

**Difficulty tiers:**
- Tier 1 (current): Single-file, 50-80 lines, 1-2 type changes — MSR ~92%
- Tier 2 (target): Single-file, 100-200 lines, 3-5 cascading changes
- Tier 3 (stretch): Multi-file, 200-500 lines, architectural modifications

**Implementation:**
1. Write 10 Tier 2 tasks (v1 solution + modification instruction + v2 tests)
2. Add to `research/benchmark/msr/modifications/prompts/`
3. Run n=10 for both Almide and Python
4. Compare per-tier MSR

---

## Phase 3: Publication

**Goal:** Publish results that credibly support "the language LLMs can write most accurately."

**Deliverables:**
1. **README section** — 3-sentence summary with key numbers
2. **Blog post / technical report** — methodology, raw data, statistical analysis
3. **Reproducibility package** — scripts + prompts + raw outputs committed in research/

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

## Execution Order

1. **Phase 2A** first — low effort, uses existing infrastructure
2. **Phase 2B** — requires problem design (1-2 days)
3. **Phase 3** — after data is complete

**Estimated total effort:** 3-5 days
**Blocked by:** Nothing — can start anytime
