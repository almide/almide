#!/usr/bin/env bash
# ============================================================
# Module System — Test Runner
# ============================================================
# Runs all module system specification tests:
#   - Happy-path tests (must compile + pass)
#   - Error tests (must fail to compile with expected message)
#
# Usage:
#   bash spec/integration/modules/run_tests.sh

set -euo pipefail

ALMIDE="${ALMIDE:-almide}"
DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
ERRORS=""

green()  { printf "\033[32m%s\033[0m" "$1"; }
red()    { printf "\033[31m%s\033[0m" "$1"; }

# --- Happy-path tests (must succeed) ---
HAPPY_TESTS=(
  "diamond_test.almd"
  "alias_test.almd"
  "submodule_call_test.almd"
  "vis_effect_test.almd"
)

echo "=== Module System — Specification Tests ==="
echo ""

for f in "${HAPPY_TESTS[@]}"; do
  printf "  %-40s " "$f"
  if output=$("$ALMIDE" test "$DIR/$f" 2>&1); then
    green "PASS"; echo ""
    PASS=$((PASS + 1))
  else
    red "FAIL"; echo ""
    ERRORS="${ERRORS}\n--- $f ---\n${output}\n"
    FAIL=$((FAIL + 1))
  fi
done

# --- Error tests (must fail to compile with expected error message) ---
echo ""
echo "--- Error tests (must fail with expected message) ---"
echo ""

check_error_test() {
  local file="$1"
  local expected_msg="$2"
  printf "  %-40s " "$file"
  if output=$("$ALMIDE" check "$DIR/$file" 2>&1); then
    red "FAIL"; echo " (should have failed but succeeded)"
    ERRORS="${ERRORS}\n--- $file ---\nExpected compile error but succeeded\n"
    FAIL=$((FAIL + 1))
  elif echo "$output" | grep -q "$expected_msg"; then
    green "PASS"; echo " (correctly rejected)"
    PASS=$((PASS + 1))
  else
    red "FAIL"; echo " (failed but wrong error)"
    ERRORS="${ERRORS}\n--- $file ---\nExpected: $expected_msg\nGot: ${output}\n"
    FAIL=$((FAIL + 1))
  fi
}

check_error_test "vis_mod_error_test.almd"       "is not accessible"
check_error_test "vis_local_error_test.almd"      "is not accessible"
check_error_test "phantom_dep_error_test.almd"    "undefined variable"
check_error_test "deep_phantom_test.almd"         "undefined variable"

# --- Summary ---
echo ""
TOTAL=$((PASS + FAIL))
echo "=== Results: $PASS/$TOTAL passed ==="

if [ "$FAIL" -gt 0 ]; then
  echo ""
  red "FAILURES:"; echo ""
  printf "%b" "$ERRORS"
  exit 1
fi

green "All module system tests passed."; echo ""
