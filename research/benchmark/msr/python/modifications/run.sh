#!/bin/bash
# python/modifications/run.sh — Run Python Modification Survival Rate benchmark
#
# Usage:
#   ./msr/python/modifications/run.sh                              # default: haiku
#   ./msr/python/modifications/run.sh --model sonnet               # use sonnet
#   ./msr/python/modifications/run.sh --exercise m01               # single task

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

PROMPT_DIR="research/benchmark/msr/python/modifications/prompts"
OUTPUT_DIR="research/benchmark/msr/python/modifications/outputs/$MODEL"

mkdir -p "$OUTPUT_DIR"

SYSTEM_PROMPT="You are modifying Python code. Output ONLY the complete modified .py file. No markdown, no explanation, no code fences. When adding an enum member, update ALL match/case statements. When changing a return type or error handling, update ALL callers and test assertions."

# Collect exercises
if [ -n "$EXERCISE" ]; then
  PROMPTS_LIST=$(ls "$PROMPT_DIR"/${EXERCISE}*.py 2>/dev/null || true)
  if [ -z "$PROMPTS_LIST" ]; then
    echo "Exercise not found matching: $EXERCISE"
    exit 1
  fi
else
  PROMPTS_LIST=$(ls "$PROMPT_DIR"/*.py 2>/dev/null)
fi

PASS=0
FAIL=0
TOTAL=0
RESULTS=""

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║  Python MSR Modification Benchmark — $MODEL"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

for prompt_file in $PROMPTS_LIST; do
  name=$(basename "$prompt_file" .py)
  output_file="$OUTPUT_DIR/${name}.py"
  TOTAL=$((TOTAL + 1))

  echo -n "⏳ $name ... "

  # Read the entire prompt file and send it to the LLM
  # This avoids bash variable expansion issues with \t \n etc.
  PROMPT="$(cat "$prompt_file")

Modify the code above according to the MODIFICATION INSTRUCTION section.
Output ONLY the complete modified .py file containing:
- All original imports and code (updated as specified)
- All original tests (updated as specified in the instruction)
- All new V2 tests
Do not omit any tests."

  claude --model "$MODEL" --print --system-prompt "$SYSTEM_PROMPT" "$PROMPT" > "$output_file" 2>/dev/null || true

  # Strip markdown code fences if present
  sed -i.bak '/^```/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true
  sed -i.bak '/^python$/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true

  # Run with Python
  TEST_OUTPUT=$(python3 "$output_file" 2>&1 || true)
  if [ -z "$TEST_OUTPUT" ]; then
    echo "✅"
    PASS=$((PASS + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":true},"
  else
    # Extract error info
    ERROR_LINE=$(echo "$TEST_OUTPUT" | tail -1 | head -c 200)
    echo "❌ $ERROR_LINE"
    FAIL=$((FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"$(echo "$ERROR_LINE" | sed 's/"/\\"/g')\"},"
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
echo "  Language:       Python"
echo "  Tasks:          $TOTAL"
echo "  Passed:         $PASS / $TOTAL"
echo ""
echo "  ┌──────────────────────────────┐"
echo "  │  Modification MSR: ${MSR}%      │"
echo "  └──────────────────────────────┘"

RESULTS_DIR="research/benchmark/msr/python/modifications/results"
mkdir -p "$RESULTS_DIR"
DATE=$(date +%Y-%m-%d)
RESULTS="${RESULTS%,}"

# Find next run number for today
RUN=1
while [ -f "$RESULTS_DIR/${MODEL}_${DATE}_run${RUN}.json" ]; do
  RUN=$((RUN + 1))
done

cat > "$RESULTS_DIR/${MODEL}_${DATE}_run${RUN}.json" <<ENDJSON
{
  "date": "$DATE",
  "language": "python",
  "type": "modification",
  "model": "$MODEL",
  "tasks": $TOTAL,
  "passed": $PASS,
  "failed": $FAIL,
  "msr": $(echo "scale=4; $PASS / $TOTAL" | bc),
  "results": [$RESULTS]
}
ENDJSON

echo ""
echo "Results saved: $RESULTS_DIR/${MODEL}_${DATE}_run${RUN}.json"
echo "Solutions:     $OUTPUT_DIR/"
