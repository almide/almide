#!/bin/bash
# Cross-Target CI: Run spec tests on Rust, TS, and WASM targets, compare results.
# Usage: ./tools/cross-target-check.sh [dir]
# Requires: deno (for TS), wasmtime (for WASM)

set -uo pipefail

DIR="${1:-spec/lang}"
ALMIDE="${ALMIDE:-./target/release/almide}"
PASS=0
FAIL_TS=0
FAIL_WASM=0
SKIP=0
ERRORS=""
TMPDIR=$(mktemp -d)

trap "rm -rf $TMPDIR" EXIT

has_deno=$(command -v deno >/dev/null 2>&1 && echo 1 || echo 0)
has_wasmtime=$(command -v wasmtime >/dev/null 2>&1 && echo 1 || echo 0)

for f in "$DIR"/*_test.almd; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .almd)

    # 1. Run on Rust target
    if ! $ALMIDE test "$f" >/dev/null 2>&1; then
        SKIP=$((SKIP + 1))
        continue
    fi

    ok=1

    # 2. TS target
    if [ "$has_deno" = "1" ]; then
        if ! $ALMIDE test "$f" --target ts >/dev/null 2>&1; then
            FAIL_TS=$((FAIL_TS + 1))
            ts_err=$($ALMIDE test "$f" --target ts 2>&1 | tail -3)
            ERRORS="$ERRORS\n  TS   failed: $name — $ts_err"
            ok=0
        fi
    fi

    # 3. WASM target
    if ! $ALMIDE test "$f" --target wasm >/dev/null 2>&1; then
        FAIL_WASM=$((FAIL_WASM + 1))
        wasm_err=$($ALMIDE test "$f" --target wasm 2>&1 | tail -3)
        ERRORS="$ERRORS\n  WASM failed: $name — $wasm_err"
        ok=0
    fi

    [ "$ok" = "1" ] && PASS=$((PASS + 1))
done

total=$((PASS + FAIL_TS + FAIL_WASM + SKIP))
echo ""
echo "=== Cross-Target Check: $DIR ==="
echo "All targets OK: $PASS"
[ "$has_deno" = "1" ] && echo "TS failed:      $FAIL_TS" || echo "TS:             (skipped — deno not found)"
echo "WASM failed:    $FAIL_WASM"
echo "Rust-only skip: $SKIP"
echo "Total files:    $total"
if [ -n "$ERRORS" ]; then
    echo -e "\nFailures:$ERRORS"
fi
echo ""
[ "$FAIL_TS" -eq 0 ] && [ "$FAIL_WASM" -eq 0 ]
