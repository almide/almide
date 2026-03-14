#!/usr/bin/env bash
# Cross-target test runner: runs .almd files on both Rust and TS targets, compares results.
# Usage: tools/cross-target-test.sh [dir-or-file...]
# If no args, runs exercises/ and spec/

set -euo pipefail

ALMIDE="${ALMIDE:-./target/release/almide}"
DENO="${DENO:-deno}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

rust_pass=0; rust_fail=0; rust_skip=0
ts_pass=0; ts_fail=0; ts_skip=0
mismatch=0
files_tested=0

run_rust_exercise() {
    local f="$1"
    $ALMIDE run "$f" > /tmp/cross-rust-out.txt 2>&1
    return $?
}

run_ts_exercise() {
    local f="$1"
    $ALMIDE "$f" --target ts > /tmp/cross-ts-src.ts 2>/dev/null || return 2
    $DENO run --allow-all --no-check /tmp/cross-ts-src.ts > /tmp/cross-ts-out.txt 2>&1
    return $?
}

run_rust_test() {
    local f="$1"
    $ALMIDE test "$f" > /tmp/cross-rust-out.txt 2>&1
    return $?
}

run_ts_test() {
    local f="$1"
    $ALMIDE "$f" --target ts > /tmp/cross-ts-src.ts 2>/dev/null || return 2
    $DENO test --allow-all --no-check /tmp/cross-ts-src.ts > /tmp/cross-ts-out.txt 2>&1
    return $?
}

compare_file() {
    local f="$1"
    local mode="$2"  # "exercise" or "test"

    files_tested=$((files_tested + 1))

    # Run Rust
    local rust_rc=0
    if [ "$mode" = "test" ]; then
        run_rust_test "$f" || rust_rc=$?
    else
        run_rust_exercise "$f" || rust_rc=$?
    fi

    # Run TS
    local ts_rc=0
    if [ "$mode" = "test" ]; then
        run_ts_test "$f" || ts_rc=$?
    else
        run_ts_exercise "$f" || ts_rc=$?
    fi

    # TS emit failed (codegen issue, not a test failure)
    if [ $ts_rc -eq 2 ]; then
        ts_skip=$((ts_skip + 1))
        if [ $rust_rc -eq 0 ]; then
            rust_pass=$((rust_pass + 1))
        else
            rust_fail=$((rust_fail + 1))
        fi
        printf "  ${YELLOW}SKIP-TS${NC}  %s (TS emit failed)\n" "$f"
        return
    fi

    # Count results
    [ $rust_rc -eq 0 ] && rust_pass=$((rust_pass + 1)) || rust_fail=$((rust_fail + 1))
    [ $ts_rc -eq 0 ] && ts_pass=$((ts_pass + 1)) || ts_fail=$((ts_fail + 1))

    # Compare
    if [ $rust_rc -eq 0 ] && [ $ts_rc -eq 0 ]; then
        printf "  ${GREEN}OK${NC}       %s\n" "$f"
    elif [ $rust_rc -ne 0 ] && [ $ts_rc -ne 0 ]; then
        printf "  ${YELLOW}BOTH-FAIL${NC} %s\n" "$f"
    else
        mismatch=$((mismatch + 1))
        if [ $rust_rc -eq 0 ]; then
            printf "  ${RED}MISMATCH${NC} %s (Rust OK, TS FAIL)\n" "$f"
        else
            printf "  ${RED}MISMATCH${NC} %s (Rust FAIL, TS OK)\n" "$f"
        fi
    fi
}

echo "Cross-target test runner"
echo "========================"
echo ""

# Determine targets
targets=("$@")
if [ ${#targets[@]} -eq 0 ]; then
    targets=("exercises" "spec")
fi

for target in "${targets[@]}"; do
    if [ -f "$target" ]; then
        # Single file
        if [[ "$target" == *_test.almd ]] || [[ "$target" == spec/* ]]; then
            compare_file "$target" "test"
        else
            compare_file "$target" "exercise"
        fi
    elif [ -d "$target" ]; then
        echo "--- $target ---"
        # Find .almd files
        while IFS= read -r f; do
            # Skip error test files
            case "$f" in *_error_test.almd) continue;; esac

            if [[ "$f" == spec/* ]] || [[ "$f" == *_test.almd ]]; then
                compare_file "$f" "test"
            else
                compare_file "$f" "exercise"
            fi
        done < <(find "$target" -name '*.almd' -type f | sort)
    fi
done

echo ""
echo "========================"
echo "Results:"
echo "  Files tested:  $files_tested"
echo "  Rust:          $rust_pass passed, $rust_fail failed, $rust_skip skipped"
echo "  TS:            $ts_pass passed, $ts_fail failed, $ts_skip skipped"
echo "  Mismatches:    $mismatch"
echo ""

if [ $mismatch -gt 0 ]; then
    echo -e "${RED}CROSS-TARGET MISMATCH DETECTED${NC}"
    exit 1
else
    echo -e "${GREEN}All cross-target results consistent${NC}"
    exit 0
fi
