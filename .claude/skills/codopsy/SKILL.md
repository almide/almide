---
name: codopsy
description: Run code quality analysis on the Almide compiler source
disable-model-invocation: true
---

# Codopsy Analysis

Run AST-level code quality analysis on the Almide compiler using codopsy.

## Steps

1. **Run analysis**: Execute `codopsy analyze src/ -v` on the compiler source.

2. **Report summary**: Show the quality score, total issues, and top hotspot functions (sorted by cognitive complexity).

3. **Compare to baseline** (if exists): Run with `--no-degradation --baseline-path .codopsy-baseline.json` to detect quality regressions. If no baseline exists, note this.

4. **Actionable output**: List the top 5 worst functions by cognitive complexity with file paths and line numbers.

## Options

- `/codopsy` — Run analysis and show summary
- `/codopsy save-baseline` — Save current results as baseline for future comparison
- `/codopsy diff` — Only analyze files changed from main branch

## Commands

```bash
# Full analysis
codopsy analyze src/ -v

# Save baseline
codopsy analyze src/ --save-baseline --baseline-path .codopsy-baseline.json

# Regression check
codopsy analyze src/ --no-degradation --baseline-path .codopsy-baseline.json

# Changed files only
codopsy analyze src/ --diff main
```

## Notes

- Report is written to `codopsy-report.json` (gitignored)
- Baseline file `.codopsy-baseline.json` should be committed
- Quality score: A (90+), B (75+), C (60+), D (45+), F (<45)
- Thresholds: cyclomatic complexity 30, cognitive complexity 30
