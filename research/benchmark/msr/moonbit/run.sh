#!/bin/bash
# msr/moonbit/run.sh — Run MSR measurement for MoonBit using Claude Code CLI
#
# Usage:
#   ./msr/moonbit/run.sh                          # default: haiku, all exercises
#   ./msr/moonbit/run.sh --model sonnet            # use sonnet
#   ./msr/moonbit/run.sh --model opus              # use opus
#   ./msr/moonbit/run.sh --exercise pangram         # single exercise
#   ./msr/moonbit/run.sh --model sonnet --exercise bob
#
# Prerequisites:
#   - claude CLI installed and authenticated
#   - moon CLI installed (MoonBit toolchain)

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

PROMPT_DIR="research/benchmark/msr/moonbit/prompts"
OUTPUT_DIR="research/benchmark/msr/moonbit/outputs/$MODEL"

mkdir -p "$OUTPUT_DIR"

SYSTEM_PROMPT="You are writing MoonBit code. Write ONLY the MoonBit code. No markdown, no explanation."

# Collect exercises to run
if [ -n "$EXERCISE" ]; then
  PROMPT_FILE="$PROMPT_DIR/$EXERCISE.mbt.prompt"
  if [ ! -f "$PROMPT_FILE" ]; then
    echo "Exercise not found: $PROMPT_FILE"
    exit 1
  fi
  PROMPTS_LIST="$PROMPT_FILE"
else
  PROMPTS_LIST=$(ls "$PROMPT_DIR"/*.mbt.prompt 2>/dev/null)
fi

PASS=0
FAIL=0
BUILD_FAIL=0
TEST_FAIL=0
TOTAL=0
RESULTS=""

echo ""
echo "╔══════════════════════════════════════════════════╗"
echo "║  MSR Measurement — MoonBit — $MODEL"
echo "╚══════════════════════════════════════════════════╝"
echo ""

for prompt_file in $PROMPTS_LIST; do
  name=$(basename "$prompt_file" .mbt.prompt)
  output_file="$OUTPUT_DIR/${name}.mbt"
  TOTAL=$((TOTAL + 1))

  # Skip wasm-smoke (no tests, just a main function)
  if [ "$name" = "wasm-smoke" ]; then
    echo "⏭  $name (skipped — no test blocks)"
    TOTAL=$((TOTAL - 1))
    continue
  fi

  echo -n "⏳ $name ... "

  # Generate solution via Claude Code CLI
  PROMPT="$(cat "$prompt_file")

Implement the functions above (replace \`abort(\"todo\")\` with working code). Output ONLY the complete .mbt file."

  claude --model "$MODEL" --print --system-prompt "$SYSTEM_PROMPT" "$PROMPT" > "$output_file" 2>/dev/null || true

  # Strip markdown code fences if present (```moonbit, ```mbt, ```)
  sed -i.bak '/^```/d' "$output_file" 2>/dev/null && rm -f "${output_file}.bak" || true

  # Create temporary MoonBit project
  WORK=$(mktemp -d)

  mkdir -p "$WORK/src"
  cat > "$WORK/moon.mod.json" <<'MODJSON'
{
  "name": "test",
  "version": "0.1.0"
}
MODJSON

  cat > "$WORK/src/moon.pkg.json" <<'PKGJSON'
{}
PKGJSON

  cp "$output_file" "$WORK/src/main.mbt"

  # Verify: build + test
  if ! (cd "$WORK" && moon check 2>&1) > /dev/null 2>&1; then
    echo "❌ build failed"
    FAIL=$((FAIL + 1))
    BUILD_FAIL=$((BUILD_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"build_fail\"},"
    rm -rf "$WORK"
    continue
  fi

  if (cd "$WORK" && moon test 2>&1) > /dev/null 2>&1; then
    echo "✅"
    PASS=$((PASS + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":true},"
  else
    echo "❌ test failed"
    FAIL=$((FAIL + 1))
    TEST_FAIL=$((TEST_FAIL + 1))
    RESULTS="$RESULTS{\"name\":\"$name\",\"passed\":false,\"error\":\"test_fail\"},"
  fi

  rm -rf "$WORK"
done

echo ""
echo "════════════════════════════════════════════════════"

if [ "$TOTAL" -eq 0 ]; then
  echo "No exercises to measure."
  exit 1
fi

MSR=$((PASS * 100 / TOTAL))

echo "  Language:       MoonBit"
echo "  Model:          $MODEL"
echo "  Passed:         $PASS / $TOTAL"
echo "  Build failures: $BUILD_FAIL"
echo "  Test failures:  $TEST_FAIL"
echo ""
echo "  ┌─────────────────────┐"
echo "  │  MSR: ${MSR}%             │"
echo "  └─────────────────────┘"

# Save JSON result
RESULTS_DIR="research/benchmark/msr/moonbit/results"
mkdir -p "$RESULTS_DIR"
DATE=$(date +%Y-%m-%d)
RESULTS="${RESULTS%,}"

cat > "$RESULTS_DIR/${MODEL}_${DATE}.json" <<ENDJSON
{
  "date": "$DATE",
  "language": "moonbit",
  "model": "$MODEL",
  "exercises": $TOTAL,
  "passed": $PASS,
  "failed": $FAIL,
  "build_failures": $BUILD_FAIL,
  "test_failures": $TEST_FAIL,
  "msr": $(echo "scale=4; $PASS / $TOTAL" | bc),
  "results": [$RESULTS]
}
ENDJSON

echo ""
echo "Results saved: $RESULTS_DIR/${MODEL}_${DATE}.json"
echo "Solutions:     $OUTPUT_DIR/"
