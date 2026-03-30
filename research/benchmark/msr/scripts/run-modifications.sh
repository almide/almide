#!/bin/bash
# msr/scripts/run-modifications.sh — Run Modification Survival Rate benchmark
#
# Usage:
#   ./msr/scripts/run-modifications.sh                              # default: haiku, all tasks
#   ./msr/scripts/run-modifications.sh --model sonnet               # use sonnet
#   ./msr/scripts/run-modifications.sh --exercise m01-traffic-light-add-variant  # single task
#   ./msr/scripts/run-modifications.sh --model sonnet --exercise m03
#
# Each modification task provides:
#   - A working v1 solution
#   - A natural language modification instruction
#   - V2 tests that must pass alongside surviving v1 tests
#
# Prerequisites:
#   - claude CLI installed and authenticated
#   - almide CLI available

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

MODEL="haiku"
EXERCISE=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --model) MODEL="$2"; shift 2 ;;
    --exercise) EXERCISE="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

PROMPT_DIR="research/benchmark/msr/modifications/prompts"
OUTPUT_DIR="research/benchmark/msr/modifications/outputs/$MODEL"
CHEATSHEET="docs/CHEATSHEET.md"

mkdir -p "$OUTPUT_DIR"

# System prompt with language reference
SYSTEM_PROMPT="You are modifying Almide code (.almd files). Here is the language reference:

$(cat "$CHEATSHEET")

IMPORTANT RULES:
- Output ONLY the complete modified .almd file. No markdown, no explanation, no code fences.
- Include ALL original tests (updated as specified in the instruction) AND the new v2 tests.
- Lambda syntax: (x) => expr, NOT fn(x) => expr
- if requires then: if cond then a else b
- String concat uses +
- Use match for pattern matching, not if chains on variants
- effect fn returns Result, auto-unwraps with !
- When adding a variant to a type, update ALL match expressions on that type.
- When changing a return type, update ALL callers and test assertions.
"

# Extract sections from a prompt file (macOS compatible — no head -n -1)
extract_v1() {
  local content
  content=$(sed -n '/^\/\/ ========== V1 SOLUTION/,/^\/\/ ========== MODIFICATION INSTRUCTION/p' "$1")
  echo "$content" | tail -n +2 | sed '$d'
}

extract_instruction() {
  local content
  content=$(sed -n '/^\/\/ ========== MODIFICATION INSTRUCTION/,/^\/\/ ========== V2 TESTS/p' "$1")
  echo "$content" | tail -n +2 | sed '$d' | sed 's/^\/\/ //'
}

extract_v2_tests() {
  sed -n '/^\/\/ ========== V2 TESTS/,$p' "$1" | tail -n +2
}

# Collect exercises to run
if [ -n "$EXERCISE" ]; then
  # Support partial match: --exercise m01 matches m01-traffic-light-add-variant.almd
  PROMPTS_LIST=$(ls "$PROMPT_DIR"/${EXERCISE}*.almd 2>/dev/null || true)
  if [ -z "$PROMPTS_LIST" ]; then
    echo "Exercise not found matching: $EXERCISE"
    exit 1
  fi
else
  PROMPTS_LIST=$(ls "$PROMPT_DIR"/*.almd 2>/dev/null)
fi

PASS=0
FAIL=0
CHECK_FAIL=0
TEST_FAIL=0
TOTAL=0
RESULTS=""

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║  MSR Modification Benchmark — $MODEL"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

for prompt_file in $PROMPTS_LIST; do
  name=$(basename "$prompt_file" .almd)
  output_file="$OUTPUT_DIR/${name}.almd"
  TOTAL=$((TOTAL + 1))

  echo -n "⏳ $name ... "

  # Extract sections
  # Count v1 and v2 tests directly from file (avoids bash variable expansion issues)
  V1_TEST_COUNT=$(sed -n '/^\/\/ ========== V1 SOLUTION/,/^\/\/ ========== MODIFICATION INSTRUCTION/p' "$prompt_file" | grep -c '^test ' || true)
  V2_TEST_COUNT=$(sed -n '/^\/\/ ========== V2 TESTS/,$p' "$prompt_file" | grep -c '^test ' || true)

  # Send entire prompt file to LLM (avoids bash expansion of \t \n etc.)
  PROMPT="$(cat "$prompt_file")

Modify the code above according to the MODIFICATION INSTRUCTION section.
Output ONLY the complete modified .almd file containing:
- All type declarations (updated as specified)
- All functions (updated as specified)
- All original tests (updated as specified in the instruction)
- All new V2 tests
Do not omit any tests."

  # Call LLM
  claude --model "$MODEL" --print --system-prompt "$SYSTEM_PROMPT" "$PROMPT" > "$output_file" 2>/dev/null || true

  # Strip markdown code fences if present
  sed -i.bak '/^```/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true
  # Strip language identifier lines (e.g., "almide" after opening fence)
  sed -i.bak '/^almide$/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true

  # Count tests in output
  OUTPUT_TEST_COUNT=$(grep -c '^test ' "$output_file" 2>/dev/null || true)

  # Verify: type check
  if ! almide check "$output_file" > /dev/null 2>&1; then
    echo "❌ type check failed ($OUTPUT_TEST_COUNT tests found, v1:$V1_TEST_COUNT v2:$V2_TEST_COUNT expected)"
    FAIL=$((FAIL + 1))
    CHECK_FAIL=$((CHECK_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"type_check\",\"v1_tests\":$V1_TEST_COUNT,\"v2_tests\":$V2_TEST_COUNT,\"output_tests\":$OUTPUT_TEST_COUNT},"
    continue
  fi

  # Verify: run tests
  TEST_OUTPUT=$(almide test "$output_file" 2>&1 || true)
  if echo "$TEST_OUTPUT" | grep -q "passed.*0 failed"; then
    # Extract pass count
    PASSED=$(echo "$TEST_OUTPUT" | grep -o '[0-9]* passed' | head -1 | grep -o '[0-9]*')
    echo "✅ ($PASSED tests passed, v1:$V1_TEST_COUNT v2:$V2_TEST_COUNT)"
    PASS=$((PASS + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":true,\"tests_passed\":$PASSED,\"v1_tests\":$V1_TEST_COUNT,\"v2_tests\":$V2_TEST_COUNT},"
  else
    # Try to extract pass/fail counts
    PASSED=$(echo "$TEST_OUTPUT" | grep -o '[0-9]* passed' | head -1 | grep -o '[0-9]*' || echo "0")
    FAILED_COUNT=$(echo "$TEST_OUTPUT" | grep -o '[0-9]* failed' | head -1 | grep -o '[0-9]*' || echo "?")
    echo "❌ test failed ($PASSED passed, $FAILED_COUNT failed, v1:$V1_TEST_COUNT v2:$V2_TEST_COUNT)"
    FAIL=$((FAIL + 1))
    TEST_FAIL=$((TEST_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"test_fail\",\"tests_passed\":$PASSED,\"tests_failed\":$FAILED_COUNT,\"v1_tests\":$V1_TEST_COUNT,\"v2_tests\":$V2_TEST_COUNT},"
  fi
done

echo ""
echo "════════════════════════════════════════════════════════════"

if [ "$TOTAL" -eq 0 ]; then
  echo "No modification tasks found."
  exit 1
fi

MSR=$((PASS * 100 / TOTAL))

echo "  Model:          $MODEL"
echo "  Tasks:          $TOTAL"
echo "  Passed:         $PASS / $TOTAL"
echo "  Check failures: $CHECK_FAIL"
echo "  Test failures:  $TEST_FAIL"
echo ""
echo "  ┌──────────────────────────────┐"
echo "  │  Modification MSR: ${MSR}%      │"
echo "  └──────────────────────────────┘"

# Save JSON result
RESULTS_DIR="research/benchmark/msr/modifications/results"
mkdir -p "$RESULTS_DIR"
DATE=$(date +%Y-%m-%d)
RESULTS="${RESULTS%,}"

cat > "$RESULTS_DIR/${MODEL}_${DATE}.json" <<ENDJSON
{
  "date": "$DATE",
  "language": "almide",
  "type": "modification",
  "model": "$MODEL",
  "tasks": $TOTAL,
  "passed": $PASS,
  "failed": $FAIL,
  "check_failures": $CHECK_FAIL,
  "test_failures": $TEST_FAIL,
  "msr": $(echo "scale=4; $PASS / $TOTAL" | bc),
  "results": [$RESULTS]
}
ENDJSON

echo ""
echo "Results saved: $RESULTS_DIR/${MODEL}_${DATE}.json"
echo "Solutions:     $OUTPUT_DIR/"
