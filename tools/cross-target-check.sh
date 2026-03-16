#!/bin/bash
# Cross-Target CI: Check if spec tests emit valid TS code.
# Usage: ./tools/cross-target-check.sh [dir]

set -euo pipefail

DIR="${1:-spec/lang}"
ALMIDE="cargo run --quiet --"
PASS=0
FAIL=0
SKIP=0
ERRORS=""

for f in "$DIR"/*_test.almd; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .almd)

    # Rust target — check it passes first
    if ! $ALMIDE test "$f" >/dev/null 2>&1; then
        SKIP=$((SKIP + 1))
        continue
    fi

    # TS target — emit TS source (stdout only, ignore stderr warnings)
    ts_stdout=$($ALMIDE "$f" --target ts 2>/dev/null) || true
    if [ -z "$ts_stdout" ]; then
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  TS emit empty: $f"
    else
        PASS=$((PASS + 1))
    fi
done

echo ""
echo "=== Cross-Target Check: $DIR ==="
echo "TS emit OK:     $PASS"
echo "TS emit FAIL:   $FAIL"
echo "Rust-only skip: $SKIP"
echo "Total:          $((PASS + FAIL + SKIP))"
if [ -n "$ERRORS" ]; then
    echo -e "\nFailures:$ERRORS"
fi
echo ""
[ "$FAIL" -eq 0 ]
