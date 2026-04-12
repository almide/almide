# Almide LLM Benchmark (archived)

> **Daily MSR measurement has moved to [almide/almide-dojo](https://github.com/almide/almide-dojo).** This Python-based framework is superseded by the Dojo's Almide-native harness. Preserved for historical reference and cross-language comparison data.

Measures LLM code generation accuracy across Almide, Python, and TypeScript.

## Metrics

| Metric | Description |
|--------|-------------|
| **FAR** (First-Attempt success Rate) | Does the LLM produce working code on the first try? |
| **MSR** (Modification Survival Rate) | Can the LLM modify existing code without breaking it? |
| **FLE** (Fix-Loop Efficiency) | How many attempts does the LLM need to fix broken code? |

## Setup

```bash
pip install anthropic pyyaml
cp benchmark/config.example.yaml benchmark/config.yaml
# Edit config.yaml with your API key, or set ANTHROPIC_API_KEY env var
```

## Usage

```bash
# Full benchmark
python3 benchmark/runner.py --metric far --lang almide,python,ts --trials 10

# Single language, single problem
python3 benchmark/runner.py --metric far --lang almide --problem pangram --trials 5

# Dry run (no API calls)
python3 benchmark/runner.py --metric far --lang almide --dry-run

# Fix-loop efficiency
python3 benchmark/runner.py --metric fle --lang almide,python,ts --trials 10

# Custom model
python3 benchmark/runner.py --metric far --model claude-sonnet-4-6 --trials 10
```

## Problem Set

| Problem | Level | Description |
|---------|-------|-------------|
| pangram | 1 (Easy) | Check if a string contains every letter of the alphabet |
| calculator | 2 (Medium) | Expression evaluator with algebraic data types |
| config-merger | 3 (Hard) | Config file parser, merger, serializer with error handling |

## Directory Structure

```
benchmark/
  runner.py              Main orchestrator
  config.example.yaml    Configuration template
  adapters/              Language-specific compile+test adapters
    almide.py            almide test
    python.py            pytest
    typescript.py        deno test
  llm/                   LLM API interaction
    client.py            Claude API client
    prompt.py            Prompt construction per metric
  analysis/              Results processing
    aggregate.py         JSONL aggregation + Fisher exact test
    report.py            Markdown report generation
  problems/              Problem definitions
    <name>/
      SPEC.md            Language-agnostic specification
      almide/            template.almd, solution.almd, tests.almd
      python/            template.py, solution.py, test_*.py
      typescript/        template.ts, solution.ts, *.test.ts
  results/               Output (gitignored)
```

## Adding Problems

1. Create `benchmark/problems/<name>/SPEC.md` with the problem description
2. For each language, create:
   - `template.*` — function signatures with `// TODO` placeholders
   - `solution.*` — reference implementation (used by MSR)
   - `test_*.*` / `tests.*` / `*.test.*` — test file
3. Run `python3 benchmark/runner.py --metric far --problem <name> --dry-run` to verify discovery

## Output Format

Results are stored as JSONL in `benchmark/results/`:

```json
{"metric":"far","problem":"pangram","language":"almide","trial":1,"model":"claude-sonnet-4-6","success":true,"compile_error":false,"input_tokens":1234,"output_tokens":567,"latency_ms":2345.6,"timestamp":"2025-01-01T00:00:00+00:00"}
```

Reports are generated as Markdown with per-problem success rates and Fisher exact test comparisons.
