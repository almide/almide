#!/usr/bin/env bash
# Usage: bash exercises/run_exercise.sh <almd_file>
set -e
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ALMD="$1"

if [ -z "$ALMD" ]; then
  echo "Usage: bash exercises/run_exercise.sh <file.almd>"
  exit 1
fi

TMPFILE=$(mktemp /tmp/almide_exercise_XXXXXX.ts)
trap "rm -f $TMPFILE" EXIT

# Transpile
if ! deno run --allow-read "$PROJECT_DIR/src/almide.ts" "$ALMD" > "$TMPFILE" 2>/dev/null; then
  echo "TRANSPILE FAIL"
  deno run --allow-read "$PROJECT_DIR/src/almide.ts" "$ALMD" 2>&1 | head -3
  exit 1
fi

# Run tests
deno test --allow-write --allow-read "$TMPFILE" 2>&1
