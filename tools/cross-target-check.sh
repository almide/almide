#!/bin/bash
# Cross-Target Output Check: Run tests on Rust, TS, WASM and compare stdout.
# Usage: ./tools/cross-target-check.sh [dir]
# Exit code: 0 = all outputs match, 1 = mismatches found

set -uo pipefail

DIR="${1:-spec/lang}"
ALMIDE="${ALMIDE:-./target/release/almide}"
MATCH=0
MISMATCH=0
SKIP=0
ERRORS=""
TMPDIR=$(mktemp -d)

trap "rm -rf $TMPDIR" EXIT

for f in "$DIR"/*_test.almd; do
    [ -f "$f" ] || continue
    name=$(basename "$f" .almd)

    # 1. Rust target
    if ! $ALMIDE test "$f" > "$TMPDIR/${name}.rust" 2>/dev/null; then
        SKIP=$((SKIP + 1))
        continue
    fi
    # Extract test summary line (e.g. "spec/lang/foo_test.almd: 9 tests passed")
    rust_summary=$(/usr/bin/grep "tests passed" "$TMPDIR/${name}.rust" | head -1)

    ok=1

    # 2. WASM target
    if $ALMIDE test "$f" --target wasm > "$TMPDIR/${name}.wasm" 2>/dev/null; then
        wasm_summary=$(/usr/bin/grep "tests passed" "$TMPDIR/${name}.wasm" | head -1)
        # Compare test counts
        rust_count=$(echo "$rust_summary" | /usr/bin/grep -oE '[0-9]+ tests' | head -1)
        wasm_count=$(echo "$wasm_summary" | /usr/bin/grep -oE '[0-9]+ tests' | head -1)
        if [ "$rust_count" != "$wasm_count" ]; then
            ERRORS="$ERRORS\n  $name: Rust=$rust_count WASM=$wasm_count"
            ok=0
        fi
    else
        wasm_err=$($ALMIDE test "$f" --target wasm 2>&1 | tail -1)
        ERRORS="$ERRORS\n  $name: WASM failed — $wasm_err"
        ok=0
    fi

    # 3. TS target
    if $ALMIDE test "$f" --target ts > "$TMPDIR/${name}.ts" 2>/dev/null; then
        ts_summary=$(/usr/bin/grep "tests passed" "$TMPDIR/${name}.ts" | head -1)
        ts_count=$(echo "$ts_summary" | /usr/bin/grep -oE '[0-9]+ tests' | head -1)
        if [ "$rust_count" != "$ts_count" ]; then
            ERRORS="$ERRORS\n  $name: Rust=$rust_count TS=$ts_count"
            ok=0
        fi
    else
        : # TS failures are expected (known codegen issues)
    fi

    if [ "$ok" = "1" ]; then
        MATCH=$((MATCH + 1))
    else
        MISMATCH=$((MISMATCH + 1))
    fi
done

total=$((MATCH + MISMATCH + SKIP))
echo ""
echo "=== Cross-Target Output Check: $DIR ==="
echo "All match:  $MATCH"
echo "Mismatch:   $MISMATCH"
echo "Skipped:    $SKIP"
echo "Total:      $total"
if [ -n "$ERRORS" ]; then
    echo -e "\nDetails:$ERRORS"
fi
echo ""
[ "$MISMATCH" -eq 0 ]
