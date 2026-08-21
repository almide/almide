#!/usr/bin/env bash
# The stability-closure REPORTER (#1485): current standing per criterion of
# proofs/stability-closure.toml — PASS / FAIL / NOT-YET-MEASURABLE — printed
# on every push so the distance to the claim is visible. Deliberately
# NON-BLOCKING: this gate's job is to make the sentence "the defect curve has
# bent" measurable, not to redden CI while it has not. Always exits 0.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "── stability closure standing (proofs/stability-closure.toml) ──"

# 1. fuzz-nights (N=14): the streak meter reads the nightly verdicts.
if streak_line=$(bash scripts/fuzz-green-streak.sh 2>/dev/null | grep -oE '[0-9]+ consecutive clean day' | grep -oE '^[0-9]+'); then
  if [ "${streak_line:-0}" -ge 14 ]; then
    echo "  fuzz-nights        PASS      (streak ${streak_line} >= 14)"
  else
    echo "  fuzz-nights        NOT-YET   (streak ${streak_line:-0} of 14)"
  fi
else
  echo "  fuzz-nights        UNMEASURED (streak meter unavailable here)"
fi

# 2. conformance-weeks (N=4): weekly workflow verdicts — not derivable from
#    the checkout alone; the exclusion-list half IS.
if [ -n "$(grep -rl 'exclude' proofs/kernel-conformance 2>/dev/null | head -1)" ]; then
  echo "  conformance-weeks  UNMEASURED (weekly verdicts live in Actions; exclusion list: check conformancegen)"
else
  echo "  conformance-weeks  UNMEASURED (weekly verdicts live in Actions)"
fi

# 3. wasm-frontier (target 0): re-measured from the baseline file.
frontier=$(grep -c ':: COMPILER_FRONTIER ::' proofs/wasm-fallback-baseline.txt 2>/dev/null || echo "?")
if [ "$frontier" = "0" ]; then
  echo "  wasm-frontier      PASS      (0 rows)"
else
  echo "  wasm-frontier      NOT-YET   (${frontier} rows of 0)"
fi

# 4. wall-corpus: the ratchets themselves gate elsewhere; report the row count.
fallback_rows=$(grep -cE '^spec/' proofs/wasm-fallback-baseline.txt 2>/dev/null || echo "?")
echo "  wall-corpus        SEE-GATES (corpus-wall + walled-real ratchet gate red on regression; baseline rows: ${fallback_rows})"

# 5. blockers (target 0): needs gh; honest UNMEASURED without it.
GH="gh"
command -v timeout >/dev/null 2>&1 && GH="timeout 20 gh"
if command -v gh >/dev/null 2>&1 && $GH auth status >/dev/null 2>&1; then
  total=0
  for lbl in I-divergence I-miscompile I-unsound; do
    n=$($GH issue list --label "$lbl" --state open --json number -q 'length' 2>/dev/null || echo "")
    [ -z "$n" ] && { total="?"; break; }
    total=$((total + n))
  done
  if [ "$total" = "0" ]; then
    echo "  blockers           PASS      (0 open I-severity issues)"
  elif [ "$total" = "?" ]; then
    echo "  blockers           UNMEASURED (gh query failed)"
  else
    echo "  blockers           NOT-YET   (${total} open)"
  fi
else
  echo "  blockers           UNMEASURED (no authenticated gh here)"
fi

# 6. dialect-still: report the current epoch; the window comparison is a
#    two-point measurement only the claim itself performs.
epoch=$(grep -oE '^n = [0-9]+' proofs/dialect-epochs.toml 2>/dev/null | tail -1 | grep -oE '[0-9]+' || echo "?")
echo "  dialect-still      INFO      (current epoch: ${epoch}; must hold across the 30-day window)"

echo "──────────────────────────────────────────────────────────────"
exit 0
