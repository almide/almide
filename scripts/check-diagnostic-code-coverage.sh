#!/usr/bin/env bash
# DIAGNOSTIC-CODE COVERAGE FLOOR (mission-critical attack list A1-3).
# ===================================================================
#
# Every DOCUMENTED E-code (docs/diagnostics/E*.md) must have at least one
# fixture family under tests/diagnostics/ (dirs named eNNN-*). The rust-scale
# negative-test push (test-surface-25x tier 1) grows per-code families; this
# gate keeps the FLOOR — a documented code with zero fixtures is a diagnostic
# nobody can refactor safely.
#
# Known no-fixture codes, each with a reason the docs carry:
#   E054 — fires only on an internal formatter defect (no committed source can
#          trigger it deliberately; pinned by fmt_corpus_test instead).
#   E033 — needs a multi-module project (opaque-type external construction);
#          pinned by module-project tests, not single-file fixtures.
#   E420 — needs a cross-module call (mod/local visibility); same.
#
# Codes with fewer than 3 fixtures are REPORTED as the backlog (soft), not
# failed — the floor is 1, the tier-1 target is "every hint variant".
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# E081 fires in the BUILD path (--target wasm availability, #1423) — the
# check-harness fixture format cannot reach it; its pin is
# tests/wasm_availability_e081_test.rs (the E054 precedent).
EXEMPT="E054 E033 E420 E081"

fail=0
backlog=""
total=0
for doc in docs/diagnostics/E*.md; do
  code=$(basename "$doc" .md)
  case " $EXEMPT " in *" $code "*) continue ;; esac
  lower=$(printf '%s' "$code" | tr 'A-Z' 'a-z')
  n=$(ls -d tests/diagnostics/${lower}-*/ 2>/dev/null | wc -l | tr -d ' ')
  total=$((total + n))
  if [ "$n" -eq 0 ]; then
    echo "::error::diagnostic-code-coverage: documented $code has NO fixture family (tests/diagnostics/${lower}-*)"
    fail=1
  elif [ "$n" -lt 3 ]; then
    backlog="$backlog $code($n)"
  fi
done

echo "diagnostic-code-coverage: $total fixture dir(s) across documented codes; floor >=1 held"
if [ -n "$backlog" ]; then
  echo "  below the 3-family tier-1 bar (backlog, not a failure):$backlog"
fi
exit $fail
