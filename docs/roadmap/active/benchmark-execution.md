<!-- description: LLM accuracy benchmarks comparing Almide, Python, and MoonBit -->
# LLM Benchmark Execution

**Priority:** High — Proving Almide's raison d'etre: "the language LLMs can write most accurately"
**Prerequisites:** benchmark/ framework + msr/ tool already built

---

## Initial Results (2026-03-25, Haiku, n=1)

| Language | Score | Cheatsheet | Training data |
|----------|-------|------------|---------------|
| **Almide** | 24/24 (100%) | Yes (449 lines) | Near zero |
| **Python** | 25/25 (100%) | No | Massive |
| **MoonBit** | 24/24 (100%) | No | Limited |

Time taken: Almide ~11min, Python ~6min, MoonBit ~12min

**Conclusion:** At this difficulty level, no meaningful difference emerges. We need modification survival rate and harder problems.
However, the fact that "Almide matches Python with zero training data + CHEATSHEET alone" is now established.

## Tools

- `research/benchmark/msr/msr.almd` — MSR runner for Almide (written in Almide itself)
- `research/benchmark/msr/python/run.sh` — Python runner (25 problems with prompts)
- `research/benchmark/msr/moonbit/run.sh` — MoonBit runner (25 problems with prompts)
- `research/benchmark/framework/runner.py` — General framework (supports FAR/MSR/FLE, unused)

## Next Steps

### Phase 1: n=10 Repeated Execution
- [ ] Run the same 24 problems 10 times per language to measure stability
- [ ] Even if one run passes everything, differences may emerge in success rate across 10 runs

### Phase 2: Modification Survival Rate
- [ ] Provide reference solutions and issue modification instructions (e.g., "change the return type to Result")
- [ ] Modification categories where Almide's effect fn / type system strengths shine:
  - Return type changes (String → Result[String, E])
  - Variant case additions (exhaustiveness check catches all match sites)
  - Record field additions (compiler reports all usage sites)
- [ ] Quantify the gap between Almide and Python/MoonBit on this metric

### Phase 3: Harder Problems
- [ ] Multi-module coordination, error propagation chains, complex generics usage
- [ ] Add production-scale problems (hundreds of lines)

### Phase 4: Analysis and Publication
- [ ] Aggregation, statistical significance testing (Fisher exact test)
- [ ] Publish results on README / website
