#!/usr/bin/env bash
# NIGHTLY FUZZ TRACKING-ISSUE BODY (#2390)
# ========================================
#
# Renders the body the verdict job posts to the tracking issue for one class
# of finding — `correctness` (everything but `Slow__*`, the `fuzz-findings`
# label) or `slow` (perf-class, the `fuzz-perf` label, #1235).
#
# Two silent under-counts lived in the workflow YAML this replaces:
#
#   1. The body said "across ${FUZZ_SHARDS} shard(s)" unconditionally — 8,
#      whether 8 shards uploaded or 1. The count is COLLECTED from the shards
#      that came back, so the body now says from how many, names the shards
#      that did not return, and says that their findings are not in the count.
#   2. The listing was `grep ... | head -60`: three matching lines per finding,
#      so a hard cap of exactly 20 findings that never said it was a cap. Run
#      33848741367 titled 23 and listed 20, and the three that fell off read as
#      unaccounted for rather than as truncated. The cap is now counted in
#      FINDINGS, and when it bites the body says "showing 20 of 23".
#
# Usage: fuzz-night-issue-body.sh <night-findings> <correctness|slow> <run-url>
#                                 <reporting> <planned> <missing> [<cap>]
#   <night-findings>  the aggregated findings dir: one subdirectory per unique
#                     finding, each holding the fuzzer's meta.txt
#   <reporting>/<planned>/<missing>  the verdict's own qualification, as
#                     scripts/fuzz-night-verdict.sh wrote them to $GITHUB_OUTPUT
#   <cap>             findings listed inline (default 20); the rest stay in the
#                     artifact and the run log, and the body says so
#
# Tested with forged nights by tests/fuzz_night_report_test.rs.

set -euo pipefail
export LC_ALL=C

DIR="${1:?night-findings dir}"
CLASS="${2:?correctness|slow}"
RUN_URL="${3:?run url}"
REPORTING="${4:?reporting}"
PLANNED="${5:?planned}"
MISSING="${6:?missing}"
CAP="${7:-20}"

case "$CLASS" in
  correctness) mapfile -t DIRS < <(find "$DIR" -mindepth 1 -maxdepth 1 -type d ! -name 'Slow__*' | sort) ;;
  slow)        mapfile -t DIRS < <(find "$DIR" -mindepth 1 -maxdepth 1 -type d -name 'Slow__*' | sort) ;;
  *) echo "fuzz-night-issue-body.sh: class must be correctness or slow, got '$CLASS'" >&2; exit 2 ;;
esac
COUNT=${#DIRS[@]}
SHOWN=$COUNT
[ "$SHOWN" -le "$CAP" ] || SHOWN=$CAP

# A verdict that never wrote its outputs leaves `reporting` unknown: say so,
# never "of 8" — the unqualified denominator is the defect this file replaces.
if [[ "$REPORTING" =~ ^[0-9]+$ ]] && [[ "$PLANNED" =~ ^[0-9]+$ ]]; then
  FROM="**$REPORTING of $PLANNED** shard(s)"
else
  FROM="an **unknown** number of the $PLANNED planned shard(s) (the verdict step recorded no shard count)"
  REPORTING=-1
  PLANNED=0
fi

case "$CLASS" in
  correctness)
    echo "The nightly generative differential fuzzer recorded **$COUNT** unique finding(s)"
    echo "collected from $FROM."
    ;;
  slow)
    echo "The nightly fuzzer recorded **$COUNT** perf-class Slow finding(s): a leg"
    echo "outran the per-program budget but completed byte-identical at the 10x"
    echo "confirm re-run. Not correctness — the night stays green — but each is a"
    echo "real order-of-magnitude perf gap (the #1229 class). Collected from"
    echo "$FROM."
    ;;
esac

if [ "$REPORTING" -ge 0 ] && [ "$REPORTING" -lt "$PLANNED" ]; then
  UNRETURNED=$((PLANNED - REPORTING))
  case "$MISSING" in
    unknown|none|"") WHICH="" ;;
    *)               WHICH=" (shards ${MISSING//,/, })" ;;
  esac
  echo ""
  echo "**$UNRETURNED shard(s)${WHICH} did not report** — their findings, if any, are not"
  echo "in this count. A reclaimed runner uploads nothing; the shard's own job log"
  echo "still lists every \`** FINDING\` line, and its seed is \`run_id * 16 + shard\`."
fi

echo ""
echo "Artifacts (minimized repros + both-target outputs) are attached to"
echo "[run]($RUN_URL)."
echo ""
echo "Each finding's \`meta.txt\` includes a deterministic replay command:"
echo "\`xtarget-fuzz replay --seed <S> --index <I>\`."
echo ""
if [ "$SHOWN" -lt "$COUNT" ]; then
  echo "<details><summary>Finding summaries — showing $SHOWN of $COUNT; the remaining $((COUNT - SHOWN)) are in the artifact and the run log</summary>"
else
  echo "<details><summary>Finding summaries ($COUNT)</summary>"
fi
echo ""
echo '```'
for ((i = 0; i < SHOWN; i++)); do
  meta="${DIRS[$i]}/meta.txt"
  [ -f "$meta" ] || { echo "kind        = ? (no meta.txt in $(basename "${DIRS[$i]}"))"; continue; }
  grep -E '^(kind|summary|reproduce)' "$meta" || true
done
if [ "$SHOWN" -lt "$COUNT" ]; then
  echo "... $((COUNT - SHOWN)) more finding(s) not shown (showing $SHOWN of $COUNT)"
fi
echo '```'
echo ""
echo "</details>"
