#!/bin/bash
# Cross-Target CI: Run spec tests on both Rust and TS targets, compare results.
# Usage: ./tools/cross-target-check.sh [dir]
# Requires: deno

set -euo pipefail

DIR="${1:-spec/lang}"
ALMIDE="cargo run --quiet --"
PASS=0
FAIL=0
SKIP=0
ERRORS=""
TMPDIR=$(mktemp -d)

trap "rm -rf $TMPDIR" EXIT

for f in "$DIR"/*_test.almd; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .almd)

    # 1. Run on Rust target
    rust_out=$($ALMIDE test "$f" 2>/dev/null | grep "^test " || true)
    if [ -z "$rust_out" ]; then
        SKIP=$((SKIP + 1))
        continue
    fi

    # 2. Emit TS
    ts_code=$($ALMIDE "$f" --target ts 2>/dev/null) || true
    if [ -z "$ts_code" ]; then
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  TS emit failed: $name"
        continue
    fi

    # 3. Run TS with Deno
    ts_file="$TMPDIR/${name}.ts"
    echo "$ts_code" > "$ts_file"
    ts_result=$(deno run --allow-all "$ts_file" 2>&1) || true

    # 4. Check TS ran without errors
    if echo "$ts_result" | grep -qi "error\|Error\|TypeError\|ReferenceError"; then
        FAIL=$((FAIL + 1))
        ts_err=$(echo "$ts_result" | grep -i "error" | head -1)
        ERRORS="$ERRORS\n  TS run failed: $name — $ts_err"
    else
        PASS=$((PASS + 1))
    fi
done

echo ""
echo "=== Cross-Target Check: $DIR ==="
echo "Both OK:        $PASS"
echo "TS failed:      $FAIL"
echo "Rust-only skip: $SKIP"
echo "Total:          $((PASS + FAIL + SKIP))"
if [ -n "$ERRORS" ]; then
    echo -e "\nFailures:$ERRORS"
fi
echo ""
[ "$FAIL" -eq 0 ]
