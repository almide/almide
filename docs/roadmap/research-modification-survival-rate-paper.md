# Research: Modification Survival Rate Paper

Target: arXiv preprint — *"Designing Programming Languages for LLM Code Modification: Measuring Survival Rate Across Iterative Edits"*

### Data collection (in progress)
- [x] Benchmark infrastructure (benchmark.rb, SPEC-v1/v2, test scripts, multi-language)
- [x] Baseline data: 16 languages × minigit task
- [ ] Post-UFCS Almide data (v0.3.0+, 10+ trials)
- [ ] Before/after comparison: same task, same LLM, pre-UFCS vs post-UFCS
- [ ] 10+ trials per language for statistical significance (ideally 30)

### Ablation study
- [ ] UFCS ON vs OFF
- [ ] LLM error messages ON vs OFF
- [ ] CLAUDE.md template provided vs not provided
- [ ] Measure which design factor contributes most to survival rate

### Paper structure
- [ ] Formal definition of "modification survival rate" (task spec, modification steps, pass/fail criteria)
- [ ] Multi-language comparison (Almide vs Python vs Go vs TS vs Rust, same LLM, same trials)
- [ ] Analysis: why does language design affect modification success?
- [ ] Reproducibility: all benchmarks, specs, and test scripts are open source
