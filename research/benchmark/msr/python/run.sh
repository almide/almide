#!/bin/bash
# msr/python/run.sh — Run MSR measurement for Python using Claude Code CLI
#
# Usage:
#   ./msr/python/run.sh                          # default: haiku, all exercises
#   ./msr/python/run.sh --model sonnet            # use sonnet
#   ./msr/python/run.sh --model opus              # use opus
#   ./msr/python/run.sh --exercise pangram         # single exercise
#   ./msr/python/run.sh --model sonnet --exercise bob
#
# Prerequisites:
#   - claude CLI installed and authenticated
#   - python3 available

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

PROMPT_DIR="research/benchmark/msr/python/prompts"
OUTPUT_DIR="research/benchmark/msr/python/outputs/$MODEL"

mkdir -p "$OUTPUT_DIR"

# System prompt — minimal, no cheatsheet (Python is pre-trained)
SYSTEM_PROMPT="You are writing Python code. Write ONLY the Python code. No markdown, no explanation, no code fences."

# Collect exercises to run
if [ -n "$EXERCISE" ]; then
  PROMPT_FILE="$PROMPT_DIR/$EXERCISE.py.prompt"
  if [ ! -f "$PROMPT_FILE" ]; then
    echo "Exercise not found: $PROMPT_FILE"
    exit 1
  fi
  PROMPTS_LIST="$PROMPT_FILE"
else
  PROMPTS_LIST=$(ls "$PROMPT_DIR"/*.py.prompt 2>/dev/null)
fi

PASS=0
FAIL=0
GENERATE_FAIL=0
TEST_FAIL=0
TOTAL=0
RESULTS=""

echo ""
echo "╔══════════════════════════════════════════════════╗"
echo "║  MSR Measurement — Python — $MODEL"
echo "╚══════════════════════════════════════════════════╝"
echo ""

for prompt_file in $PROMPTS_LIST; do
  name=$(basename "$prompt_file" .py.prompt)
  output_file="$OUTPUT_DIR/${name}.py"
  TOTAL=$((TOTAL + 1))

  echo -n "⏳ $name ... "

  # Generate solution via Claude Code CLI
  PROMPT="$(cat "$prompt_file")

Implement the functions above (replace \`pass\` with working code). Output ONLY the complete Python file."

  claude --model "$MODEL" --print --system-prompt "$SYSTEM_PROMPT" "$PROMPT" > "$output_file" 2>/dev/null || true

  # Strip markdown code fences if present
  sed -i.bak '/^```/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true

  # Check if file is empty or generation failed
  if [ ! -s "$output_file" ]; then
    echo "❌ generation failed"
    FAIL=$((FAIL + 1))
    GENERATE_FAIL=$((GENERATE_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"generation_fail\"},"
    continue
  fi

  # Run tests via python3
  if python3 "$output_file" > /dev/null 2>&1; then
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

echo "  Language:          Python"
echo "  Model:             $MODEL"
echo "  Passed:            $PASS / $TOTAL"
echo "  Generation fails:  $GENERATE_FAIL"
echo "  Test failures:     $TEST_FAIL"
echo ""
echo "  ┌─────────────────────┐"
echo "  │  MSR: ${MSR}%             │"
echo "  └─────────────────────┘"

# Save JSON result
RESULTS_DIR="research/benchmark/msr/results"
mkdir -p "$RESULTS_DIR"
DATE=$(date +%Y-%m-%d)
RESULTS="${RESULTS%,}"

cat > "$RESULTS_DIR/${MODEL}_python_${DATE}.json" <<ENDJSON
{
  "date": "$DATE",
  "language": "python",
  "model": "$MODEL",
  "exercises": $TOTAL,
  "passed": $PASS,
  "failed": $FAIL,
  "generation_failures": $GENERATE_FAIL,
  "test_failures": $TEST_FAIL,
  "msr": $(echo "scale=4; $PASS / $TOTAL" | bc),
  "results": [$RESULTS]
}
ENDJSON

echo ""
echo "Results saved: $RESULTS_DIR/${MODEL}_python_${DATE}.json"
echo "Solutions:     $OUTPUT_DIR/"
