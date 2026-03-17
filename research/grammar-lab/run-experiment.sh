#!/bin/bash
# Grammar Lab: optional-handling experiment runner
# Usage: ./run-experiment.sh [--trials N] [--model MODEL]

set -euo pipefail
cd "$(dirname "$0")"

ALMIDE="${ALMIDE_BIN:-../../target/release/almide}"
CLAUDE="${CLAUDE_BIN:-node /Users/o6lvl4/.local/share/mise/installs/node/22.18.0/lib/node_modules/@anthropic-ai/claude-code/cli.js}"
TRIALS="${TRIALS:-3}"
MODEL="${MODEL:-claude-haiku-4-5}"
EXPERIMENT="experiments/optional-handling"
TMPDIR="/tmp/grammar-lab"

mkdir -p "$TMPDIR" results outputs

# Parse CLI args
while [[ $# -gt 0 ]]; do
  case "$1" in
    --trials) TRIALS="$2"; shift 2;;
    --model) MODEL="$2"; shift 2;;
    *) echo "Unknown arg: $1"; exit 1;;
  esac
done

echo "=== Grammar Lab: optional-handling ==="
echo "Model: $MODEL  Trials: $TRIALS"
echo ""

# Load layer1 rules
LAYER1=$(cat prompts/layer1_rules.md)

# Load tasks
TASKS=(t01_change_default t02_add_format t03_add_function t04_apply_discount t05_chain_fields t06_change_predicate t07_change_none_msg t08_sort_direction t09_add_condition t10_add_step)

# Transpile: convert option.map/flat_map/unwrap_or to match/unwrap_or
transpile_combinator() {
  local code="$1"
  # Step 1: |> option.unwrap_or(X) -> |> match { some(__v) => __v, none => X }
  code=$(echo "$code" | perl -pe '
    s/\|> option\.unwrap_or\(([^)]*)\)/|> match { some(__v) => __v, none => $1 }/g
  ')

  # Step 2: |> option.map((PARAM) => BODY) -> |> match { some(PARAM) => some(BODY), none => none }
  # Handle single-line pipe patterns. Support both (p) => and fn(p) => forms.
  code=$(echo "$code" | perl -pe '
    while (s/\|> option\.map\((?:fn)?\(([^)]+)\) => ([^)]*(?:\([^)]*\))*[^)]*)\)/|> match { some($1) => some($2), none => none }/g) {}
  ')

  # Step 3: |> option.flat_map((PARAM) => BODY) -> |> match { some(PARAM) => BODY, none => none }
  code=$(echo "$code" | perl -pe '
    while (s/\|> option\.flat_map\((?:fn)?\(([^)]+)\) => ([^)]*(?:\([^)]*\))*[^)]*)\)/|> match { some($1) => $2, none => none }/g) {}
  ')

  echo "$code"
}

# Extract code from LLM response (```almide ... ```)
extract_code() {
  local response="$1"
  local in_block=false
  local code=""
  while IFS= read -r line; do
    if [[ "$line" == '```almide'* ]]; then
      in_block=true
      continue
    fi
    if $in_block && [[ "$line" == '```'* ]]; then
      in_block=false
      continue
    fi
    if $in_block; then
      code+="$line"$'\n'
    fi
  done <<< "$response"
  # If no code block found, use entire response
  if [[ -z "$code" ]]; then
    echo "$response"
  else
    echo "$code"
  fi
}

# Strip test blocks from LLM output
strip_tests() {
  local code="$1"
  echo "$code" | perl -0pe 's/\btest\s+"[^"]*"\s*\{[^{}]*(\{[^{}]*\}[^{}]*)*\}//gs'
}

# Results CSV
RESULTS_FILE="results/optional-handling_$(date +%s).csv"
echo "model,variant,task,trial,compiled,tests_passed" > "$RESULTS_FILE"

VARIANTS=("match" "combinator")
VARIANT_DIRS=("variant-match" "variant-combinator")

for vi in 0 1; do
  VARIANT="${VARIANTS[$vi]}"
  VARIANT_DIR="${VARIANT_DIRS[$vi]}"

  # Load layer2
  LAYER2=""
  if [[ -f "prompts/layer2_${VARIANT}.md" ]]; then
    LAYER2=$(cat "prompts/layer2_${VARIANT}.md")
  fi

  echo "Variant: $VARIANT"

  for TASK in "${TASKS[@]}"; do
    # Load task config
    TASK_JSON="$EXPERIMENT/tasks/${TASK}.json"
    INSTRUCTION=$(python3 -c "import json; d=json.load(open('$TASK_JSON')); print(d['instruction'])")
    LAYER3=$(python3 -c "import json; d=json.load(open('$TASK_JSON')); print(d.get('layer3_hint',''))")
    TEST_FILE=$(python3 -c "import json; d=json.load(open('$TASK_JSON')); print(d['test_file'])")

    # Load source and test
    SOURCE=$(cat "$EXPERIMENT/$VARIANT_DIR/${TASK}.almd")
    TEST_CODE=$(cat "$EXPERIMENT/tasks/${TEST_FILE}")

    SYSTEM="You are modifying code written in Almide (.almd files). Return ONLY the modified code, no explanation.

${LAYER1}

${LAYER2}

${LAYER3}"

    USER_PROMPT="## Current Code

\`\`\`almide
${SOURCE}
\`\`\`

## Tests (must pass after modification)

\`\`\`almide
${TEST_CODE}
\`\`\`

## Task

${INSTRUCTION}

Return ONLY the modified source code (the Current Code section above, with your changes applied). Do NOT include the test blocks — tests will be appended automatically. Wrap your code in \`\`\`almide ... \`\`\`."

    for TRIAL in $(seq 1 "$TRIALS"); do
      echo -n "  ${TASK} trial ${TRIAL}/${TRIALS} ... "

      # Call LLM
      LLM_OUTPUT=$($CLAUDE -p --model "$MODEL" --append-system-prompt "$SYSTEM" --output-format text --max-turns 1 "$USER_PROMPT" 2>/dev/null || echo "ERROR")

      # Extract and strip tests
      MODIFIED=$(extract_code "$LLM_OUTPUT")
      MODIFIED=$(strip_tests "$MODIFIED")

      # Save raw output
      OUTDIR="outputs/optional-handling/${MODEL}/${VARIANT}"
      mkdir -p "$OUTDIR"
      echo "$MODIFIED" > "${OUTDIR}/${TASK}_${TRIAL}_raw.almd"

      # Transpile if combinator variant
      if [[ "$VARIANT" == "combinator" ]]; then
        MODIFIED=$(transpile_combinator "$MODIFIED")
      fi
      echo "$MODIFIED" > "${OUTDIR}/${TASK}_${TRIAL}_compiled.almd"

      # Combine with tests and evaluate
      COMBINED="${MODIFIED}

${TEST_CODE}"
      CODE_PATH="${TMPDIR}/${TASK}_${TRIAL}.almd"
      echo "$COMBINED" > "$CODE_PATH"

      COMPILED=false
      PASSED=false

      if $ALMIDE check "$CODE_PATH" >/dev/null 2>&1; then
        COMPILED=true
        if $ALMIDE test "$CODE_PATH" >/dev/null 2>&1; then
          PASSED=true
        fi
      fi

      # Record result
      echo "${MODEL},${VARIANT},${TASK},${TRIAL},${COMPILED},${PASSED}" >> "$RESULTS_FILE"

      if $PASSED; then
        echo "PASS"
      elif $COMPILED; then
        echo "FAIL (test)"
      else
        echo "FAIL (compile)"
      fi
    done
  done
done

echo ""
echo "=== Results ==="
echo ""

# Summary
for VARIANT in "${VARIANTS[@]}"; do
  TOTAL=$(grep ",${VARIANT}," "$RESULTS_FILE" | wc -l | tr -d ' ')
  PASS=$(grep ",${VARIANT},.*,true,true" "$RESULTS_FILE" | wc -l | tr -d ' ')
  if [[ "$TOTAL" -gt 0 ]]; then
    PCT=$((PASS * 100 / TOTAL))
    echo "${VARIANT}: ${PASS}/${TOTAL} (${PCT}%)"
  fi
done

echo ""
echo "Results saved to: $RESULTS_FILE"
