#!/bin/bash
# msr/scripts/run.sh — Run MSR measurement using Claude Code CLI
#
# Usage:
#   ./msr/scripts/run.sh                          # default: haiku, all exercises
#   ./msr/scripts/run.sh --model sonnet            # use sonnet
#   ./msr/scripts/run.sh --model opus              # use opus
#   ./msr/scripts/run.sh --exercise bob            # single exercise
#   ./msr/scripts/run.sh --model sonnet --exercise bob
#
# Prerequisites:
#   - claude CLI installed and authenticated
#   - msr/prompts/ populated (run extract.sh first)

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

MODEL="haiku"
EXERCISE=""
TARGET="rust"

while [[ $# -gt 0 ]]; do
  case $1 in
    --model) MODEL="$2"; shift 2 ;;
    --exercise) EXERCISE="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

PROMPT_DIR="msr/prompts"
OUTPUT_DIR="msr/outputs/$MODEL"
CHEATSHEET="docs/CHEATSHEET.md"

mkdir -p "$OUTPUT_DIR"

# System prompt with language reference
SYSTEM_PROMPT="You are writing Almide code (.almd files). Here is the language reference:

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
"

# Collect exercises to run
if [ -n "$EXERCISE" ]; then
  PROMPTS="$PROMPT_DIR/$EXERCISE.almd"
  if [ ! -f "$PROMPTS" ]; then
    echo "Exercise not found: $PROMPTS"
    exit 1
  fi
  PROMPTS_LIST="$PROMPTS"
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
echo "╔══════════════════════════════════════════════════╗"
echo "║  MSR Measurement — $MODEL (target: $TARGET)"
echo "╚══════════════════════════════════════════════════╝"
echo ""

for prompt_file in $PROMPTS_LIST; do
  name=$(basename "$prompt_file" .almd)
  output_file="$OUTPUT_DIR/${name}.almd"
  TOTAL=$((TOTAL + 1))

  # Skip wasm-smoke for rust target
  if [ "$name" = "wasm-smoke" ] && [ "$TARGET" = "rust" ]; then
    echo "⏭  $name (skipped — wasm only)"
    TOTAL=$((TOTAL - 1))
    continue
  fi

  echo -n "⏳ $name ... "

  # Generate solution via Claude Code CLI
  PROMPT="$(cat "$prompt_file")

Implement the functions above (replace \`todo\` with working code). Output ONLY the complete .almd file."

  claude --model "$MODEL" --print -s "$SYSTEM_PROMPT" "$PROMPT" > "$output_file" 2>/dev/null || true

  # Strip markdown code fences if present
  sed -i.bak '/^```/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true

  # Verify: type check
  if ! almide check "$output_file" > /dev/null 2>&1; then
    echo "❌ type check failed"
    FAIL=$((FAIL + 1))
    CHECK_FAIL=$((CHECK_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"type_check\"},"
    continue
  fi

  # Verify: run tests
  if almide test "$output_file" > /dev/null 2>&1; then
    echo "✅"
    PASS=$((PASS + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":true},"
  else
    echo "❌ test failed"
    FAIL=$((FAIL + 1))
    TEST_FAIL=$((TEST_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"test_fail\"},"
  fi
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
echo "  Passed:         $PASS / $TOTAL"
echo "  Check failures: $CHECK_FAIL"
echo "  Test failures:  $TEST_FAIL"
echo ""
echo "  ┌─────────────────────┐"
echo "  │  MSR: ${MSR}%             │"
echo "  └─────────────────────┘"

# Save JSON result
RESULTS_DIR="msr/results"
mkdir -p "$RESULTS_DIR"
DATE=$(date +%Y-%m-%d)
RESULTS="${RESULTS%,}"

cat > "$RESULTS_DIR/${MODEL}_${TARGET}_${DATE}.json" <<ENDJSON
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
  "msr": $(echo "scale=4; $PASS / $TOTAL" | bc),
  "results": [$RESULTS]
}
ENDJSON

echo ""
echo "Results saved: $RESULTS_DIR/${MODEL}_${TARGET}_${DATE}.json"
echo "Solutions:     $OUTPUT_DIR/"
