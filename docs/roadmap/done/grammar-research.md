<!-- description: A/B testing infrastructure for syntax design using LLM benchmarks -->
<!-- done: 2026-03-15 -->
# Grammar Research Infrastructure

## Vision

Drive Almide's grammar design decisions with data, not intuition.

An infrastructure for automatically running A/B tests of syntax variants across multiple LLMs and quantitatively measuring the impact on modification survival rate. The tool itself is implemented in Almide (dogfooding).

## Problem

Current grammar design decisions are based on discussion and intuition. For example:

- `fn(x) => expr` vs `(x) => expr` — which is less likely to be broken by LLMs?
- `emit expr` vs bare expression — does an explicit keyword improve survival rate?
- `web.param(req, "id")` vs `req.param("id")` — does UFCS improve LLM accuracy?

There is no mechanism to "answer these questions with experiments and numbers."

## Core Concept

```
Experiment = {
  hypothesis: "短縮 lambda は modification survival rate を下げない"
  variants: [
    A: { syntax: fn(x) => expr,  corpus: [...tasks...] }
    B: { syntax: (x) => expr,    corpus: [...tasks...] }
  ]
  models: [claude-sonnet, gpt-4o, gemini-pro, ...]
  trials: 30
  metric: modification survival rate (compile + test pass)
}
```

1. Prepare semantically identical code in each syntax variant
2. Give modification tasks to LLMs
3. Automatically determine compile → test → pass/fail
4. Compare survival rate between variants
5. Test for statistical significance

## Architecture

```
grammar-research/
├── src/
│   ├── mod.almd              // entry point
│   ├── experiment.almd       // experiment definition & execution
│   ├── variant.almd          // syntax variant management
│   ├── runner.almd           // LLM execution (API calls)
│   ├── evaluator.almd        // automatic compile + test evaluation
│   ├── stats.almd            // statistical analysis & significance testing
│   └── report.almd           // report generation
├── experiments/
│   ├── lambda-syntax/        // fn(x) => vs (x) =>
│   ├── emit-vs-bare/         // emit keyword vs type dispatch
│   ├── ufcs-external/        // module prefix vs UFCS
│   ├── builder-keyword/      // with keyword vs without
│   └── ...
├── corpus/                   // task definitions
│   ├── tasks.toml            // task metadata
│   └── tasks/                // individual tasks (.almd + spec)
└── results/                  // experiment results (JSON)
```

## Data Model

```almide
type Experiment = {
  name: String
  hypothesis: String
  variants: List[Variant]
  models: List[ModelConfig]
  trials_per_variant: Int
  tasks: List[Task]
}

type Variant = {
  name: String                    // "fn-lambda" / "paren-lambda"
  description: String
  corpus_dir: String              // 構文バリアント版コーパスのパス
}

type ModelConfig = {
  name: String                    // "claude-sonnet-4-6"
  provider: Provider              // Anthropic / OpenAI / Google
  temperature: Float
}

type Provider = | Anthropic | OpenAI | Google

type Task = {
  name: String
  description: String             // modification instructions passed to LLM
  base_file: String               // file to be modified
  test_file: String               // test file
  max_attempts: Int               // retry count
}

type TrialResult = {
  experiment: String
  variant: String
  model: String
  task: String
  trial: Int
  compiled: Bool
  tests_passed: Bool
  error_type: Option[ErrorType]
  llm_output: String
  duration_ms: Int
}

type ErrorType =
  | SyntaxError { message: String }
  | TypeError { message: String }
  | TestFailure { failed: List[String] }
  | Timeout
  | ApiError { message: String }
```

## Experiment Workflow

### 1. Define Experiment

```toml
# experiments/lambda-syntax/experiment.toml

name = "lambda-syntax"
hypothesis = "Short lambda (x) => does not degrade modification survival rate vs fn(x) =>"

[[variants]]
name = "fn-lambda"
description = "Current syntax: fn(x) => expr"
corpus_dir = "experiments/lambda-syntax/fn-lambda/"

[[variants]]
name = "paren-lambda"
description = "Short syntax: (x) => expr"
corpus_dir = "experiments/lambda-syntax/paren-lambda/"

[[models]]
name = "claude-sonnet-4-6"
provider = "anthropic"
temperature = 0.0

[[models]]
name = "gpt-4o"
provider = "openai"
temperature = 0.0

trials_per_variant = 30
```

### 2. Prepare Corpus

Same logic, different syntax:

```almide
// experiments/lambda-syntax/fn-lambda/map_filter.almd
let result = [1, 2, 3, 4, 5]
  |> list.filter(fn(x) => x > 2)
  |> list.map(fn(x) => x * 10)

// experiments/lambda-syntax/paren-lambda/map_filter.almd
let result = [1, 2, 3, 4, 5]
  |> list.filter((x) => x > 2)
  |> list.map((x) => x * 10)
```

Task: "Change `x > 2` to `x > 3` and `x * 10` to `x * 100`"

### 3. Run

```bash
almide run src/mod.almd -- run experiments/lambda-syntax/
```

Runs each variant x model x task x trial. Passes modification instructions + code to LLM, then compile + test the returned code.

### 4. Evaluate

```almide
// evaluator.almd
effect fn evaluate(output: String, task: Task) -> TrialResult = {
  // 1. Extract code from LLM output
  let code = extract_code(output)

  // 2. Write to temporary file
  fs.write(task.base_file, code)

  // 3. almide check (compile check)
  let check = process.run("almide", ["check", task.base_file])
  guard check.exit_code == 0 else return trial_result(compiled: false, error: parse_error(check.stderr))

  // 4. almide test (run tests)
  let test = process.run("almide", ["test", task.test_file])
  guard test.exit_code == 0 else return trial_result(tests_passed: false, error: parse_test_failure(test.stderr))

  // 5. Success
  trial_result(compiled: true, tests_passed: true)
}
```

### 5. Analyze

```almide
// stats.almd

// Survival rate per variant per model
fn survival_rate(results: List[TrialResult]) -> Float =
  let passed = results |> list.filter(fn(r) => r.tests_passed) |> list.len()
  int.to_float(passed) / int.to_float(list.len(results))

// Fisher's exact test for significance
fn fishers_exact(a_pass: Int, a_fail: Int, b_pass: Int, b_fail: Int) -> Float = ...

// Error type distribution
fn error_distribution(results: List[TrialResult]) -> Map[String, Int] = ...
```

### 6. Report

```
=== Lambda Syntax Experiment ===

Hypothesis: Short lambda (x) => does not degrade modification survival rate

Results (30 trials each):

                    fn(x) =>    (x) =>     p-value
claude-sonnet-4-6   87% (26/30) 90% (27/30) 0.73
gpt-4o              80% (24/30) 83% (25/30) 0.68
gemini-pro          73% (22/30) 77% (23/30) 0.61

Conclusion: No significant difference. Short lambda is safe to adopt.

Error breakdown (fn-lambda):
  SyntaxError:  3
  TypeError:    1
  TestFailure:  0

Error breakdown (paren-lambda):
  SyntaxError:  2
  TypeError:    1
  TestFailure:  0
```

## Planned Experiments

### Phase 1: Foundational Experiments

| Experiment | Variant A | Variant B | Hypothesis |
|------------|-----------|-----------|------------|
| Lambda syntax | `fn(x) => expr` | `(x) => expr` | Short form does not reduce survival rate |
| Builder insertion | `emit expr` keyword | bare expr (type dispatch) | Survival rate is equivalent without keyword |
| UFCS | `web.param(req, "id")` | `req.param("id")` | UFCS improves survival rate |
| Module prefix | `web.get(...)` | `get(...)` (selective import) | Omitting prefix does not reduce survival rate |

### Phase 2: Template Syntax Experiments

| Experiment | Variant A | Variant B | Hypothesis |
|------------|-----------|-----------|------------|
| Template keyword | `template name(...)` | `fn name(...) -> HtmlDoc` | `template` keyword improves survival rate |
| HTML tag recognition | Known tags only | Arbitrary identifiers | Known tag restriction reduces typos |
| Builder block style | `html { div { ... } }` | JSX-style `<div>...</div>` | Builder block is less likely to break |

### Phase 3: Cross-Language Experiments

| Experiment | Description |
|------------|-------------|
| Almide vs TypeScript | Have LLMs write the same task in Almide and TS, compare iterative edit survival rate |
| Almide vs Python | Same |
| Almide vs Go | Same |

## Self-Hosting: Implemented in Almide

Reasons for writing the tool itself in Almide:

1. **Dogfooding** — prove Almide's practicality firsthand
2. **Template integration** — write report generation with `template`
3. **Codec integration** — JSON serialize/deserialize of experiment results with `deriving Codec`
4. **HTTP client** — use stdlib's `http.request` for LLM API calls
5. **Process module** — use `process.run` for executing `almide check` / `almide test`

### Almide Features Required for Implementation

| Feature | Status | Necessity |
|---------|--------|-----------|
| HTTP client (`http.request`) | ✅ done | Required (LLM API) |
| JSON parse/stringify | ✅ done | Required |
| File I/O (`fs.read_text`, `fs.write`) | ✅ done | Required |
| Process execution (`process.run`) | ✅ done | Required (almide check/test) |
| `deriving Codec` | active (not yet implemented) | Used in Phase 2 |
| Template | active (not yet implemented) | Used for report generation |
| CLI args | ✅ done | Required |

**Phase 1 can be implemented with current Almide.** Codec / Template integration in Phase 2.

## Implementation Order

### Phase 1: MVP (possible with current Almide)

- Experiment TOML parser (json.parse-based, or manual parse)
- Single model x single variant execution loop
- Automated evaluation via almide check + almide test
- CSV / JSON result output
- First experiment: lambda syntax A/B

### Phase 2: Multi-model + Statistics

- Multiple LLM provider support (Anthropic, OpenAI, Google)
- Parallel execution (async let)
- Fisher's exact test / chi-squared test
- Text report generation

### Phase 3: Codec + Template Integration

- Type-safe experiment definitions and results with `deriving Codec`
- HTML report generation with `template`
- Web UI (view results via web framework)

### Phase 4: CI Integration

- Automatically run survival rate experiments on grammar change PRs
- Regression detection: warn if new syntax reduces survival rate

## Relationship to Existing Research

The existing [Research: Modification Survival Rate Paper](../on-hold/research-modification-survival-rate-paper.md) focuses on **cross-language comparison**. This roadmap focuses on **syntax variant comparison within the same language**.

The two are complementary:
- Paper: demonstrates "Almide has higher modification survival rate than other languages"
- Grammar Research: demonstrates "which syntax choices within Almide contribute to survival rate"

The ablation study section of the paper can be written using the experimental results from this roadmap.

## Success Criteria

- In grammar design discussions, we can say "based on experiment X, Variant A has Y% survival rate, Z% higher than Variant B"
- Running A/B experiments when adding new syntax becomes a standard process
- Almide's Mission ("the language LLMs can write most accurately") is backed by numbers
