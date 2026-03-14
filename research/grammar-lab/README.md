# Grammar Lab

A/B test framework for measuring how syntax choices affect LLM modification survival rate.

## Quick Start

```bash
cd research/grammar-lab

# Set paths (required if not in PATH)
export ALMIDE_BIN=/path/to/almide          # or add almide to PATH
export CLAUDE_BIN=/path/to/claude          # optional, auto-detected via `which`

# Build the runner
almide build src/mod.almd -o /tmp/grammar-lab-bin

# Run experiment (all models, 5 trials)
/tmp/grammar-lab-bin experiments/lambda-syntax/

# Run with specific model
/tmp/grammar-lab-bin experiments/lambda-syntax/ --model claude-sonnet-4-6

# Override trial count
/tmp/grammar-lab-bin experiments/lambda-syntax/ --trials 3
```

### Path Resolution

Binary paths are resolved in this order:
1. Environment variable (`ALMIDE_BIN`, `CLAUDE_BIN`)
2. `which` lookup (if the binary is in PATH)
3. Fallback (relative path `../../target/release/almide`)

## Structure

```
grammar-lab/
├── src/mod.almd                    # Runner (Almide)
├── prompts/
│   ├── layer1_rules.md             # Common rules (~200 tok)
│   ├── layer2_variant_fn.md        # Examples for fn(x) => variant
│   └── layer2_variant_paren.md     # Examples for (x) => variant
├── experiments/
│   └── lambda-syntax/
│       ├── experiment.json         # Experiment config
│       ├── variant-fn/             # Source files using fn(x) =>
│       ├── variant-paren/          # Source files using (x) =>
│       └── tasks/                  # Task definitions + test files
└── results/                        # Output JSON
```

## Prompt Design

Three-layer system based on research:

| Layer | Content | Varies by |
|-------|---------|-----------|
| Layer 1 | Almide rules (common mistakes) | Nothing (shared) |
| Layer 2 | Example code (3 functions) | Variant |
| Layer 3 | Task-specific function hints | Task |

Design references:
- "One good example beats five adjectives" (Anthropic)
- Grammar Prompting (NeurIPS 2023): minimal grammar per task
- Few-shot sweet spot: 2-5 examples
- System prompt sweet spot: 150-300 words

## Output

Results are JSON arrays:

```json
[
  {
    "experiment": "lambda-syntax",
    "variant": "fn-lambda",
    "model": "claude-sonnet-4-6",
    "task": "add-filter-condition",
    "trial": 1,
    "compiled": true,
    "tests_passed": true,
    "error": ""
  }
]
```

## Adding Experiments

1. Create `experiments/<name>/experiment.json`
2. Create variant dirs with source `.almd` files
3. Create `tasks/` with task `.json` definitions and `_test.almd` files
4. Add variant-specific `prompts/layer2_<variant>.md`
5. Run: `almide run src/mod.almd -- experiments/<name>/`
