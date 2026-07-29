#!/usr/bin/env bash
# NIGHTLY FUZZ VERDICT RENDERER (#924)
# ====================================
#
# Renders one night's campaign output into (a) a markdown verdict for
# $GITHUB_STEP_SUMMARY on stdout and (b) the greppable per-night record line
# on stderr, so it lands in the job log even with stdout redirected:
#
#   fuzz-night: budget_completed=true minutes=5 elapsed=300.1s generated=1052
#               throughput=210.3prog/min findings=0
#
# budget_completed is read from the presence of the fuzzer's own
# `=== campaign summary ===` block: print_summary (tools/xtarget-fuzz) only
# runs after the campaign loop exits on its own (time or program budget), so
# a reclaimed runner cannot fake it. A truncated night reports its last
# progress line instead.
#
# Lives in a file, not workflow YAML, so it can be run and tested locally.
# Division of labour: scripts/fuzz-track-record.sh scores nights ACROSS runs
# from step conclusions; this renders the numbers WITHIN one night.
#
# Usage: fuzz-night-verdict.sh <fuzz-output.txt> <minutes> <findings>

set -euo pipefail

OUT="${1:?usage: fuzz-night-verdict.sh <fuzz-output.txt> <minutes> <findings>}"
MINUTES="${2:?minutes}"
FINDINGS="${3:?findings}"

echo "## Nightly fuzz verdict"
echo ""

if [ ! -s "$OUT" ]; then
  echo "The campaign produced no output — it was killed before it started."
  exit 0
fi

# The campaign header pads its columns ("seed     = N"), so the separator is
# `space*=space*`, not a single "= ".
SEED=$(grep -oE "seed += +[0-9]+" "$OUT" | tr -s ' ' | head -1 || true)

if grep -q "^=== campaign summary ===" "$OUT"; then
  ELAPSED=$(awk '/^  elapsed /{print $3; exit}' "$OUT")
  GENERATED=$(awk '/^  generated /{print $3; exit}' "$OUT")
  THROUGHPUT=$(awk '/^  throughput /{print $3; exit}' "$OUT")
  LINE="fuzz-night: budget_completed=true minutes=$MINUTES elapsed=$ELAPSED generated=$GENERATED throughput=${THROUGHPUT}prog/min findings=$FINDINGS"
else
  LAST=$(grep -E "^ *\[ *[0-9]+s\]" "$OUT" | tail -1 || true)
  LINE="fuzz-night: budget_completed=false minutes=$MINUTES last_progress=\"${LAST:-<none>}\" findings=$FINDINGS"
fi

echo "$LINE" >&2

echo '```'
echo "$LINE"
echo '```'
echo ""
echo "Campaign ${SEED:-seed unknown} — replay a finding with"
echo '`xtarget-fuzz replay --seed S --index I`'

if grep -q "^=== campaign summary ===" "$OUT"; then
  echo ""
  echo '```'
  sed -n '/^=== campaign summary ===/,$p' "$OUT" | head -25
  echo '```'
fi
