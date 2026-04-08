#!/bin/bash
# msr/scripts/run.sh — Run MSR measurement using Claude Code CLI
#
# Usage:
#   ./msr/scripts/run.sh                          # default: sonnet, all exercises
#   ./msr/scripts/run.sh --model opus              # use opus
#   ./msr/scripts/run.sh --exercise bob            # single exercise
#   ./msr/scripts/run.sh --model sonnet --exercise bob
#   ./msr/scripts/run.sh --target wasm             # WASM target
#   ./msr/scripts/run.sh --max-attempts 1          # single-shot (no retry)
#
# Prerequisites:
#   - claude CLI installed and authenticated
#   - research/benchmark/msr/prompts/ populated (run extract.sh first)

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

MODEL="sonnet"
EXERCISE=""
TARGET="wasm"
MAX_ATTEMPTS=3

while [[ $# -gt 0 ]]; do
  case $1 in
    --model) MODEL="$2"; shift 2 ;;
    --exercise) EXERCISE="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    --max-attempts) MAX_ATTEMPTS="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

PROMPT_DIR="research/benchmark/msr/prompts"
OUTPUT_DIR="research/benchmark/msr/outputs/$MODEL"
CHEATSHEET="docs/CHEATSHEET.md"

mkdir -p "$OUTPUT_DIR"

# Write system prompt to temp file (avoids shell argument length limits)
SYSTEM_PROMPT_FILE=$(mktemp)
trap "rm -f '$SYSTEM_PROMPT_FILE'" EXIT
cat > "$SYSTEM_PROMPT_FILE" <<SYSPROMPT
You are writing Almide code (.almd files). Here is the language reference:

$(cat "$CHEATSHEET")

IMPORTANT RULES:
- Write ONLY the Almide code. No markdown, no explanation, no code fences.
- Replace \`todo\` with working implementations.
- Keep all test blocks exactly as they are.
- Keep all type declarations exactly as they are.
- Lambda syntax: (x) => expr, NOT fn(x) => expr
- if requires then: if cond then a else b
- String concat uses +
- Use match for pattern matching, not if chains on variants
- effect fn returns Result, auto-unwraps with ?
SYSPROMPT

# Collect exercises to run
if [ -n "$EXERCISE" ]; then
  PROMPTS="$PROMPT_DIR/$EXERCISE.almd.prompt"
  if [ ! -f "$PROMPTS" ]; then
    echo "Exercise not found: $PROMPTS"
    exit 1
  fi
  PROMPTS_LIST="$PROMPTS"
else
  PROMPTS_LIST=$(ls "$PROMPT_DIR"/*.almd.prompt 2>/dev/null)
fi

PASS=0
FAIL=0
CHECK_FAIL=0
TEST_FAIL=0
TOTAL=0
RESULTS=""

echo ""
echo "╔══════════════════════════════════════════════════╗"
echo "║  MSR Measurement — $MODEL (target: $TARGET, max $MAX_ATTEMPTS attempts)"
echo "╚══════════════════════════════════════════════════╝"
echo ""

for prompt_file in $PROMPTS_LIST; do
  name=$(basename "$prompt_file" .almd.prompt)
  output_file="$OUTPUT_DIR/${name}.almd"
  TOTAL=$((TOTAL + 1))

  # Skip wasm-smoke for rust target
  if [ "$name" = "wasm-smoke" ] && [ "$TARGET" = "rust" ]; then
    echo "⏭  $name (skipped — wasm only)"
    TOTAL=$((TOTAL - 1))
    continue
  fi

  echo -n "⏳ $name ... "

  BASE_PROMPT="$(cat "$prompt_file")

Implement the functions above (replace \`todo\` with working code). Output ONLY the complete .almd file."

  ATTEMPT=0
  EXERCISE_PASSED=false

  while [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; do
    ATTEMPT=$((ATTEMPT + 1))

    if [ "$ATTEMPT" -eq 1 ]; then
      PROMPT="$BASE_PROMPT"
    else
      # Retry: include previous code + error for the model to fix
      PREV_CODE=$(cat "$output_file" 2>/dev/null || echo "")
      PROMPT="Previous code:

$PREV_CODE

Compiler errors:
$ERROR_MSG

Output the corrected .almd file below. ALL code, ALL tests. Nothing else — no analysis, no explanation, no markdown fences. Start directly with the first line of code."
    fi

    claude --model "$MODEL" --print --system-prompt-file "$SYSTEM_PROMPT_FILE" "$PROMPT" > "$output_file" 2>/dev/null || true

    # Strip markdown code fences (ERE for macOS)
    sed -i.bak -E '/^```(almide|almd)?$/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true

    # Verify: type check
    ERROR_MSG=$(almide check "$output_file" 2>&1) && CHECK_OK=true || CHECK_OK=false
    if [ "$CHECK_OK" = false ]; then
      if [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; then
        echo -n "retry($ATTEMPT) "
        continue
      fi
      echo "❌ check failed (${ATTEMPT}/${MAX_ATTEMPTS})"
      FAIL=$((FAIL + 1))
      CHECK_FAIL=$((CHECK_FAIL + 1))
      RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"type_check\",\"attempts\":$ATTEMPT},"
      break
    fi

    # Verify: run tests
    TEST_CMD=(almide test "$output_file")
    [ "$TARGET" = "wasm" ] && TEST_CMD+=(--target wasm)
    TEST_OUTPUT=$("${TEST_CMD[@]}" 2>&1) && TEST_EXIT=0 || TEST_EXIT=$?

    if [ "$TEST_EXIT" -ne 0 ]; then
      ERROR_MSG="$TEST_OUTPUT"
      if [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; then
        echo -n "retry($ATTEMPT) "
        continue
      fi
      echo "❌ test failed (${ATTEMPT}/${MAX_ATTEMPTS})"
      FAIL=$((FAIL + 1))
      TEST_FAIL=$((TEST_FAIL + 1))
      RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"test_fail\",\"attempts\":$ATTEMPT},"
      break
    fi

    # Detect 0-test false positive
    if echo "$TEST_OUTPUT" | grep -q "ok\. 0 passed"; then
      ERROR_MSG="0 tests were found. The test blocks may be missing or malformed."
      if [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; then
        echo -n "retry($ATTEMPT) "
        continue
      fi
      echo "❌ 0 tests (${ATTEMPT}/${MAX_ATTEMPTS})"
      FAIL=$((FAIL + 1))
      TEST_FAIL=$((TEST_FAIL + 1))
      RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"zero_tests\",\"attempts\":$ATTEMPT},"
      break
    fi

    # Success
    if [ "$ATTEMPT" -eq 1 ]; then
      echo "✅"
    else
      echo "✅ (attempt $ATTEMPT)"
    fi
    PASS=$((PASS + 1))
    EXERCISE_PASSED=true
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":true,\"attempts\":$ATTEMPT},"
    break
  done
done

echo ""
echo "════════════════════════════════════════════════════"

if [ "$TOTAL" -eq 0 ]; then
  echo "No exercises to measure."
  exit 1
fi

MSR=$((PASS * 100 / TOTAL))

echo "  Model:          $MODEL"
echo "  Target:         $TARGET"
echo "  Max attempts:   $MAX_ATTEMPTS"
echo "  Passed:         $PASS / $TOTAL"
echo "  Check failures: $CHECK_FAIL"
echo "  Test failures:  $TEST_FAIL"
echo ""
echo "  ┌─────────────────────┐"
echo "  │  MSR: ${MSR}%             │"
echo "  └─────────────────────┘"

# Save JSON result
RESULTS_DIR="research/benchmark/msr/results"
mkdir -p "$RESULTS_DIR"
DATE=$(date +%Y-%m-%d)
RESULTS="${RESULTS%,}"

cat > "$RESULTS_DIR/${MODEL}_${TARGET}_${DATE}.json" <<ENDJSON
{
  "date": "$DATE",
  "language": "almide",
  "target": "$TARGET",
  "model": "$MODEL",
  "max_attempts": $MAX_ATTEMPTS,
  "exercises": $TOTAL,
  "passed": $PASS,
  "failed": $FAIL,
  "check_failures": $CHECK_FAIL,
  "test_failures": $TEST_FAIL,
  "msr": $(echo "scale=4; $PASS / $TOTAL" | bc),
  "results": [$RESULTS]
}
ENDJSON

echo ""
echo "Results saved: $RESULTS_DIR/${MODEL}_${TARGET}_${DATE}.json"
echo "Solutions:     $OUTPUT_DIR/"
