#!/bin/bash
# msr/scripts/measure.sh — Verify LLM solutions and compute MSR
#
# Usage: ./msr/scripts/measure.sh <model-name> [--target rust|ts]
#
# Prerequisites:
#   - Place LLM solutions in research/benchmark/msr/outputs/<model-name>/*.almd
#   - Each solution file must have the same name as the exercise
#
# Example:
#   ./msr/scripts/measure.sh claude-opus-4
#   ./msr/scripts/measure.sh gpt-4o --target ts

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

MODEL="${1:?Usage: measure.sh <model-name> [--target rust|ts]}"
TARGET="${3:-rust}"
SOLUTIONS_DIR="research/benchmark/msr/outputs/$MODEL"
RESULTS_DIR="research/benchmark/msr/results"

if [ ! -d "$SOLUTIONS_DIR" ]; then
  echo "Error: $SOLUTIONS_DIR not found"
  echo "Place LLM solutions in $SOLUTIONS_DIR/<exercise>.almd"
  exit 1
fi

mkdir -p "$RESULTS_DIR"

PASS=0
FAIL=0
TOTAL=0
CHECK_FAIL=0
TEST_FAIL=0
RESULTS=""

echo "MSR Measurement — $MODEL (target: $TARGET)"
echo "════════════════════════════════════════════"
echo ""

for solution in "$SOLUTIONS_DIR"/*.almd; do
  [ -f "$solution" ] || continue
  name=$(basename "$solution" .almd)
  TOTAL=$((TOTAL + 1))

  # Step 1: Type check (use exit code, not string matching)
  if ! almide check "$solution" > /dev/null 2>&1; then
    echo "❌ $name — type check failed"
    FAIL=$((FAIL + 1))
    CHECK_FAIL=$((CHECK_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"type_check\"},"
    continue
  fi

  # Step 2: Run tests + detect 0-test false positive
  TEST_OUTPUT=$(almide test "$solution" 2>&1) && TEST_EXIT=0 || TEST_EXIT=$?
  if [ "$TEST_EXIT" -ne 0 ]; then
    echo "❌ $name — test failed"
    FAIL=$((FAIL + 1))
    TEST_FAIL=$((TEST_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"test_fail\"},"
  elif echo "$TEST_OUTPUT" | grep -q "ok\. 0 passed"; then
    echo "❌ $name — 0 tests detected"
    FAIL=$((FAIL + 1))
    TEST_FAIL=$((TEST_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"zero_tests\"},"
  else
    echo "✅ $name"
    PASS=$((PASS + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":true},"
  fi
done

echo ""
echo "────────────────────────────────────────────"

if [ "$TOTAL" -eq 0 ]; then
  echo "No solutions found in $SOLUTIONS_DIR/"
  exit 1
fi

MSR=$((PASS * 100 / TOTAL))

echo "MSR: $PASS/$TOTAL ($MSR%)"
echo "  Check failures: $CHECK_FAIL"
echo "  Test failures:  $TEST_FAIL"

# Write JSON result
DATE=$(date +%Y-%m-%d)
RESULTS="${RESULTS%,}"  # remove trailing comma
cat > "$RESULTS_DIR/${MODEL}_${DATE}.json" <<ENDJSON
{
  "date": "$DATE",
  "language": "almide",
  "target": "$TARGET",
  "model": "$MODEL",
  "exercises": $TOTAL,
  "passed": $PASS,
  "failed": $FAIL,
  "check_failures": $CHECK_FAIL,
  "test_failures": $TEST_FAIL,
  "msr": $(echo "scale=2; $PASS / $TOTAL" | bc),
  "results": [$RESULTS]
}
ENDJSON

echo ""
echo "Results saved to $RESULTS_DIR/${MODEL}_${DATE}.json"
