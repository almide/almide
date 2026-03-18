# Experiment: optional-handling

## Hypothesis

Combinator-style Option handling (`option.map` + `option.unwrap_or` pipe chains) has higher modification survival rate than match-based handling (`match some/none`).

## Setup

- **Model**: Claude Haiku 4.5
- **Trials**: 3 per task per variant
- **Tasks**: 10
- **Total runs**: 60 (10 tasks x 2 variants x 3 trials)

### Variants

| | match | combinator |
|---|---|---|
| Option decomposition | `match opt { some(x) => ..., none => ... }` | `opt \|> option.map((x) => ...) \|> option.unwrap_or(default)` |
| Transpile | None | `option.map` -> `match`, `option.unwrap_or` -> `match` |
| Layer2 prompt | match-style examples | combinator-style examples |

## Results

| Variant | Pass | Rate |
|---------|------|------|
| **match** | **21/30** | **70%** |
| **combinator** | **12/30** | **40%** |

### Per-task breakdown

| Task | match | combinator | Notes |
|------|-------|-----------|-------|
| t01 change_default | 3/3 | 3/3 | Both pass |
| t02 add_format | 3/3 | 0/3 (compile) | Transpile bug: string interpolation `${}` inside `option.map` body breaks regex |
| t03 add_function | 3/3 | 3/3 | Both pass |
| t04 apply_discount | 3/3 | 3/3 | Both pass |
| t05 chain_fields | 3/3 | 0/3 (compile) | Transpile bug: `int.to_string()` call inside `option.map` body breaks regex |
| t06 change_predicate | 3/3 | 0/3 (compile) | Transpile bug: `int.to_string()` call inside `option.map` body breaks regex |
| t07 change_none_msg | 0/3 | 0/3 | Both fail — LLM struggles with `target` variable capture in none-branch message |
| t08 sort_direction | 0/3 | 0/3 | Both fail — LLM doesn't correctly remove `list.reverse()` or renames function |
| t09 add_condition | 3/3 | 3/3 | Both pass |
| t10 add_step | 0/3 | 0/3 (compile) | Both fail — LLM struggles with multi-step pipeline creation |

### Adjusted results (excluding transpile-bug tasks t02, t05, t06)

| Variant | Pass | Rate |
|---------|------|------|
| **match** | **12/21** | **57%** |
| **combinator** | **12/21** | **57%** |

**Identical.** When transpile infrastructure works correctly, match and combinator have the same survival rate.

## Conclusion

**No statistically significant difference between match-style and combinator-style Option handling.**

The 70% vs 40% raw difference is entirely explained by transpile infrastructure bugs (regex can't handle nested parentheses in string interpolation). When those tasks are excluded, both variants achieve identical 57% survival rate.

### Implications for Almide design

1. **Match is sufficient.** Adding `option.map`/`option.flat_map` to stdlib would not improve LLM accuracy.
2. **`?.` syntax would also not help.** If combinators don't outperform match, syntactic sugar for combinators (`?.`/`??`) wouldn't either.
3. **Task difficulty dominates.** The variance is between tasks (easy: 100%, hard: 0%), not between styles. Improving LLM accuracy requires better error messages, examples, and language rules — not syntax changes.

### Caveats

- N=3 trials per cell is small. Results should be replicated with N=5+.
- Transpile bugs affected 3/10 combinator tasks. A proper stdlib implementation of `option.map` would eliminate this confound.
- Only tested on Haiku. Sonnet may show ceiling effects (100% on both).
- The failing tasks (t07, t08, t10) may have task design issues rather than LLM capability issues.

## Lessons

1. **Transpile quality dominates combinator results** — same lesson as the lambda-syntax experiment. Regex-based transpile breaks on nested parens inside strings.
2. **Layer 1 rules matter more than syntax style** — both variants succeed/fail on the same tasks, suggesting the language rules prompt is the key factor.
3. **Haiku is the right model for benchmarking** — 70% overall rate shows meaningful variance. Sonnet would likely hit 90%+ on both.
