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

# Run the current-schema experiment (configured models, 3 trials; invokes the model)
/tmp/grammar-lab-bin experiments/law-spec/

# Run with specific model
/tmp/grammar-lab-bin experiments/law-spec/ --model claude-haiku-4-5

# Override trial count
/tmp/grammar-lab-bin experiments/law-spec/ --trials 1
```

### Path Resolution

Binary paths are resolved in this order:
1. Environment variable (`ALMIDE_BIN`, `CLAUDE_BIN`)
2. `which` lookup (if the binary is in PATH)
3. Fallback (relative path `../../target/release/almide`)

## Current schema and historical experiments

`src/mod.almd::load_task` reads each task's `source_dir` and `test_file`, plus
optional `hidden_file`, `laws_file`, and `laws_compiled_file`. Variants can set
`with_laws`, and the experiment can request a repair round. `law-spec` supplies
this schema. `lambda-syntax` below is historical: its old task JSON and
`paren-to-fn-lambda` experiment are not a drop-in current-compiler quick start.
The runner itself can be checked without model calls using `almide check src/mod.almd`.

## Historical lambda-syntax layout

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
    "experiment": "law-spec",
    "variant": "laws",
    "model": "claude-haiku-4-5",
    "task": "t01_rle",
    "trial": 1,
    "compiled_1": true,
    "gate_1": true,
    "hidden_1": true,
    "repaired": false,
    "gate_2": true,
    "hidden_2": true
  }
]
```

A passing visible/law gate with a failing hidden test is reported as
"survived but wrong"; gate pass alone is not correctness. The example above
illustrates the current output schema, not a measured trial.

## Adding Experiments

1. Create `experiments/<name>/experiment.json`
2. Create variant dirs with source `.almd` files
3. Create `tasks/` JSON with `source_dir`, `test_file`, and optional hidden/law paths, following `law-spec/tasks/`; source paths are relative to the experiment and test/law paths to `tasks/`
4. Add variant-specific `prompts/layer2_<variant>.md`
5. Run: `almide run src/mod.almd -- experiments/<name>/`
