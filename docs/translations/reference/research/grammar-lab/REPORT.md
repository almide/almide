<a id="grammar-lab-lambda-syntax-実験レポート"></a>

# Grammar Lab: Lambda Syntax Experiment Report

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](REPORT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REPORT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](REPORT.zh-CN.md)

> Non-normative historical source-pinned reader snapshot. The unchanged original remains authoritative; no automatic synchronization, independent runs or measurements. Historical workarounds and commands are not current instructions. Code comments are verbatim. LESSONS expressly corrects old auto-unwrap, API names and argv[0] assumptions in its opening; do not apply the old recommendations as current API guidance. REPORT’s equivalence/significance claims, general recommendations and bug status are the original authors’ dated interpretations, not universal guarantees. Its duplicate section 5, four-improvements/three-row mismatch, Sonnet-rule attribution versus run table, and later t05 difficulty/pattern-enumeration descriptions are retained without silently reconciling them. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/REPORT.md)

<a id="実験概要"></a>

## Experiment Overview

**Hypothesis:** The shortened lambda `(x) => expr` has the same modification survival rate as the current syntax `fn(x) => expr`.

**Method:** Have an LLM perform the same modification tasks using two syntax variants, and compare compile + test pass rates.

**Feature:** paren-lambda is not implemented in Almide. Transpile LLM output into the current syntax before compile + test (allowing an experiment without implementing the syntax).

---

<a id="結果"></a>

## Results

<a id="最終結果"></a>

### Final Results

| Model | fn-lambda | paren-lambda | p-value |
|-------|-----------|-------------|---------|
| claude-sonnet-4-6 | **100%** (25/25) | **100%** (25/25) | 1.0 |
| claude-haiku-4-5 | **86%** (26/30) | **86%** (26/30) | 1.0 |

**Conclusion: No difference.** Introducing paren-lambda does not affect the survival rate.

<a id="改善の推移"></a>

### Progression of Improvements

| Run | Change | Sonnet fn/paren | Haiku fn/paren |
|-----|------|-----------------|----------------|
| 1 | Initial | 100% / 93% | — |
| 2 | Fixed duplicate test type definitions | 100% / 100% | — |
| 3 | Added `if then` rule to Layer1 | — | 80% / 72% |
| 4 | Strip test blocks + improve prompt | 100% / 100% | 88% / 96% |
| 5 | Transpile bug-fix version (pattern enumeration) | — | 90% / 26% ← transpile bug |
| 6 | Generalized transpile (bracket matching) | — | **86% / 86%** ← correct result |

<a id="タスク別-breakdown-haiku-run-6-10-tasks--3-trials"></a>

### Per-Task Breakdown (Haiku, Run 6, 10 tasks × 3 trials)

| Task | fn-lambda | paren-lambda |
|------|-----------|-------------|
| t01 (change filter condition) | 100% | 100% |
| t02 (add map step) | 100% | 100% |
| t03 (change sort key) | 66% | 100% |
| t04 (add fold function) | 100% | 100% |
| t05 (modify compound pipe) | 100% | 100% |
| t06 (extract lambda) | 100% | 100% |
| t07 (compound transformation) | 0% | 66% |
| t08 (Option handling) | 100% | 100% |
| t09 (change type) | 100% | 0% |
| t10 (add variant) | 100% | 100% |

The variants differ on t07 and t09, but in opposite directions (paren wins t07; fn wins t09). Their totals are exactly equal.

---

<a id="知見"></a>

## Findings

<a id="1-paren-lambda-は-fn-lambda-と完全に同等"></a>

### 1. paren-lambda Is Completely Equivalent to fn-lambda

Final results (after generalizing transpile): fn 86% = paren 86%, p=1.0. No statistically significant difference.

**Removing `fn(x) => expr` and standardizing on `(x) => expr` does not change the survival rate.** Based on this result, the decision was made to retire `fn` lambda syntax.

<a id="2-実験基盤の品質が結果を支配する"></a>

### 2. Experimental Infrastructure Quality Dominates Results

Of four improvements, three addressed **experimental infrastructure bugs**, not LLM problems:

| Run | Cause | Was it the LLM's fault? |
|-----|------|------------|
| 1→2 | Duplicate type definitions in test files | ❌ Runner bug |
| 2→3 | Missing `if then` rule | △ Layer 1 deficiency |
| 3→4 | LLM outputs test blocks → duplicate definitions | ❌ Runner did not strip them |

**Lesson: To obtain trustworthy experimental data, improve infrastructure quality first.**

<a id="3-layer-1-のルール-1-行が直接効く"></a>

### 3. One Layer 1 Rule Has a Direct Effect

Adding just one line specifying `if` syntax eliminated Sonnet's failures. Assuming the LLM does not know Almide, accurately communicating likely mistakes is important.

<a id="4-トランスパイルで未実装構文を事前テストできる"></a>

### 4. Transpilation Can Test Unimplemented Syntax in Advance

paren-lambda is not implemented in Almide's parser, but text conversion from `(x) =>` to `fn(x) =>` allows compile + test to pass. **Measure survival rate before implementing syntax and use it to inform the implementation decision.**

<a id="5-transpile-の品質が結果を支配する"></a>

### 5. Transpile Quality Dominates Results

The drop of paren-lambda to 26% in Run 5 was a transpile bug. Pattern-enumeration transpilation is fragile and cannot cover all variable names an LLM uses. Switching to a general bracket-matching algorithm in Run 6 restored it to 86%.

**Lesson: Question transpile quality. When LLM results are poor, first check for transpile bugs.** A mechanism was added to save raw LLM output in `outputs/`, allowing the causes to be separated.

<a id="5-弱いモデルで差が出る"></a>

### 5. Weaker Models Reveal Differences

Sonnet hits the ceiling at 100% on all tasks. Differences become visible only with Haiku. **Use weaker models as benchmarks for syntax-design evaluation.**

---

<a id="実験設計の反省"></a>

## Reflections on Experimental Design

<a id="うまくいったこと"></a>

### What Worked

- **Three-layer prompt design** (Layer 1: rules, Layer 2: examples, Layer 3: task hints) — the research-based design worked
- **Claude Code provider** — experiments ran without API key management
- **Transpilation** — enabled testing of unimplemented syntax
- **Writing the runner in Almide** — worked as dogfooding and also exposed language issues

<a id="改善が必要なこと"></a>

### What Needs Improvement

- **Insufficient N** — with 25, statistical significance is hard to obtain; 30+ is needed
- **Task difficulty is uneven** — t01-t04 are too easy and reach 100%, while only t05 is difficult
- **t05 instructions are ambiguous** — task-design quality directly affects results
- **String-based transpilation** — pattern enumeration because regex back references cannot be used; new transpile rules must be written for each new syntax experiment

---

<a id="次のステップ"></a>

## Next Steps

1. **Improve t05 instructions + add tasks** — balance difficulty
2. **Rerun with N=30** — test statistical significance
3. **Other syntax experiments** — template keyword, builder emit/embed, UFCS, etc. can use the same infrastructure
4. **Support regex back references** — improve transpile generality
5. **Automatic result analysis** — Fisher's exact test and automated error classification

---

<a id="技術的な副産物"></a>

## Technical Byproducts

Developing Grammar Lab exposed three Almide compiler issues:

1. **Err from effect fn main silently exits** → fixed (added `eprintln!`)
2. **Ownership moves in nested loops** → codegen bug, not fixed
3. **Missing `?` in process.exec_with_stdin** → stdlib TOML bug, not fixed

These issues would not have been found without Grammar Lab: the value of dogfooding.
