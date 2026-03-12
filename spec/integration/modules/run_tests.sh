#!/usr/bin/env bash
# ============================================================
# Module System v2 — Test Runner
# ============================================================
# Runs all module system specification tests:
#   - Happy-path tests (must compile + pass)
#   - Error tests (must fail to compile with expected message)
#
# Usage:
#   cd exercises/mod-test && bash run_tests.sh
#   # or from project root:
#   bash exercises/mod-test/run_tests.sh

set -euo pipefail

ALMIDE="${ALMIDE:-almide}"
DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
ERRORS=""

green()  { printf "\033[32m%s\033[0m" "$1"; }
red()    { printf "\033[31m%s\033[0m" "$1"; }
yellow() { printf "\033[33m%s\033[0m" "$1"; }

# --- Happy-path tests (must succeed) ---
HAPPY_TESTS=(
  "mod_system_test.almd"      # Comprehensive spec: 25 tests
  "vis_effect_test.almd"      # Effect fn across modules
)

echo "=== Module System v2 — Specification Tests ==="
echo ""

for f in "${HAPPY_TESTS[@]}"; do
  printf "  %-40s " "$f"
  if output=$("$ALMIDE" run "$DIR/$f" 2>&1); then
    # Count tests from output
    count=$(echo "$output" | grep -c ' ok$' || true)
    green "PASS"; echo " ($count tests)"
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
  if output=$("$ALMIDE" run "$DIR/$file" 2>&1); then
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

check_error_test "vis_mod_error_test.almd"   "is not accessible"
check_error_test "vis_local_error_test.almd"  "is not accessible"

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
