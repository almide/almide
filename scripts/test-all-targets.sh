#!/usr/bin/env bash
# Run all exercises on both Rust and TS targets.
# Usage: ./scripts/test-all-targets.sh [almide-binary]

set -euo pipefail

ALMIDE="${1:-almide}"
FAILED=0

for target in rust ts; do
  echo "===== Target: $target ====="
  for f in exercises/*/*.almd; do
    result=$("$ALMIDE" run "$f" --target "$target" 2>&1) || true
    rc=${PIPESTATUS[0]:-$?}
    # error_test files are expected to fail
    if echo "$f" | grep -q "error_test"; then
      continue
    fi
    if [ $rc -ne 0 ]; then
      echo "FAIL ($target): $f"
      echo "$result" | tail -5
      FAILED=$((FAILED + 1))
    fi
  done
  echo ""
done

if [ $FAILED -eq 0 ]; then
  echo "All exercises passed on both targets."
else
  echo "$FAILED exercise(s) failed."
  exit 1
fi
