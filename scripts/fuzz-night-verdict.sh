#!/usr/bin/env bash
# NIGHTLY FUZZ VERDICT RENDERER (#924)
# ====================================
#
# Renders one night's SHARDED campaign into (a) a markdown verdict for
# $GITHUB_STEP_SUMMARY on stdout and (b) the greppable per-night record line
# on stderr, so it lands in the job log even with stdout redirected:
#
#   fuzz-night: shards=4/4 minutes_planned=20 minutes_delivered=20.0
#               generated=4210 throughput=210.5prog/min findings=0
#
# WHY SHARDS. The night used to be one campaign in one job, so one reclaimed
# runner ("The runner has received a shutdown signal", exit 143 — a JOB-level
# kill no `set +e` or `timeout-minutes` can catch) cost the WHOLE night and
# reported identically to a real finding. Measured over the 12 nights after the
# build was split out (#1014): 10 completed, 2 were killed — a ~1-in-6 kill
# rate, which makes #924's "14 consecutive full-budget nights" a (5/6)^14 ≈ 8%
# proposition. The condition was unreachable by arithmetic, not by any property
# of the compiler.
#
# Sharding fixes the arithmetic: N independent jobs, N independent runner
# lifetimes. A kill now costs 1/N of the night's coverage instead of the night,
# and the night still produces a verdict from the shards that did report. It
# also multiplies coverage — N shards of M minutes deliver N*M fuzz-minutes in
# M minutes of wall clock.
#
# A shard that was killed simply has no output file: `upload-artifact` never
# ran. That absence is the signal, and it is reported as `shards=k/N` rather
# than being confused with a finding.
#
# WHAT MAKES A NIGHT RED. Correctness findings, and only those. Coverage lost
# to a reclaimed runner is reported, never fatal — otherwise the infra noise
# the sharding exists to absorb would come straight back in through the
# verdict. Perf-class `Slow` findings (#1235: a leg that outran the budget but
# completed byte-identical at the fuzzer's 10x confirm re-run) are reported on
# the record line and tracked under the perf label, but they do not fail the
# night either — the 0.57.0 release gate showed a quadratic-slow run (#1229)
# going red as a phantom "Hang".
#
# budget_completed per shard is read from the presence of the fuzzer's own
# `=== campaign summary ===` block: print_summary (tools/xtarget-fuzz) only
# runs after the campaign loop exits on its own (time or program budget), so a
# reclaimed runner cannot fake it.
#
# Lives in a file, not workflow YAML, so it can be run and tested locally.
# Division of labour: scripts/fuzz-track-record.sh scores nights ACROSS runs
# from job conclusions; this aggregates the shards WITHIN one night.
#
# Usage: fuzz-night-verdict.sh <shard-dir> <minutes-per-shard> <shards-planned> <findings> [<slow>]
#   <shard-dir> holds one subdirectory per reporting shard, each containing
#   fuzz-output.txt (the layout `actions/download-artifact` produces when
#   several artifacts are downloaded without a `name:`).
#   <slow> is the perf-class subset of <findings> (defaults to 0 so pre-#1235
#   callers keep working); the record line splits the two.

set -euo pipefail
# Byte-order collation, pinned (#1031): the shard walk below is `find | sort`,
# so an unpinned locale would order the verdict's per-shard rows differently on
# differently-configured machines — the same drift that made
# docs/roadmap/README.md churn with no content change.
export LC_ALL=C

DIR="${1:?usage: fuzz-night-verdict.sh <shard-dir> <minutes-per-shard> <shards-planned> <findings> [<slow>]}"
MINUTES="${2:?minutes-per-shard}"
PLANNED="${3:?shards-planned}"
FINDINGS="${4:?findings}"
SLOW="${5:-0}"
CORRECTNESS=$((FINDINGS - SLOW))

echo "## Nightly fuzz verdict"
echo ""

# One shard = one fuzz-output.txt anywhere under DIR. A killed shard uploaded
# nothing, so it is absent here — that is exactly what `shards=k/N` reports.
mapfile -t OUTS < <(find "$DIR" -name fuzz-output.txt -type f 2>/dev/null | sort)
REPORTING=${#OUTS[@]}

if [ "$REPORTING" -eq 0 ]; then
  LINE="fuzz-night: shards=0/$PLANNED minutes_planned=$((MINUTES * PLANNED)) minutes_delivered=0 generated=0 findings=$FINDINGS correctness=$CORRECTNESS slow=$SLOW"
  echo "$LINE" >&2
  echo '```'; echo "$LINE"; echo '```'
  echo ""
  echo "No shard reported: every runner was reclaimed before its campaign could upload."
  exit 0
fi

COMPLETED=0
GENERATED=0
ELAPSED=0
SEEDS=""
ROWS=""
for out in "${OUTS[@]}"; do
  seed=$(grep -oE "seed += +[0-9]+" "$out" | tr -s ' ' | cut -d' ' -f3 | head -1 || true)
  if grep -q "^=== campaign summary ===" "$out"; then
    COMPLETED=$((COMPLETED + 1))
    g=$(awk '/^  generated /{print $3; exit}' "$out")
    e=$(awk '/^  elapsed /{gsub(/s$/,"",$3); print $3; exit}' "$out")
    GENERATED=$((GENERATED + ${g:-0}))
    ELAPSED=$(awk -v a="$ELAPSED" -v b="${e:-0}" 'BEGIN{printf "%.1f", a+b}')
    ROWS="${ROWS}| ${seed:-?} | complete | ${g:-?} | ${e:-?}s |"$'\n'
  else
    # Truncated: credit what its last progress line saw, so a reclaimed shard
    # still contributes its real coverage instead of being counted as zero.
    last=$(grep -E "^ *\[ *[0-9]+s\]" "$out" | tail -1 || true)
    g=$(echo "$last" | grep -oE 'generated=[0-9]+' | cut -d= -f2 || true)
    e=$(echo "$last" | grep -oE '\[ *[0-9]+s\]' | grep -oE '[0-9]+' || true)
    GENERATED=$((GENERATED + ${g:-0}))
    ELAPSED=$(awk -v a="$ELAPSED" -v b="${e:-0}" 'BEGIN{printf "%.1f", a+b}')
    ROWS="${ROWS}| ${seed:-?} | truncated | ${g:-?} | ${e:-?}s |"$'\n'
  fi
  SEEDS="${SEEDS}${seed:-?} "
done

DELIVERED=$(awk -v e="$ELAPSED" 'BEGIN{printf "%.1f", e/60}')
THROUGHPUT=$(awk -v g="$GENERATED" -v e="$ELAPSED" 'BEGIN{printf "%.1f", (e>0)? g*60/e : 0}')
LINE="fuzz-night: shards=$COMPLETED/$PLANNED reporting=$REPORTING minutes_planned=$((MINUTES * PLANNED)) minutes_delivered=$DELIVERED generated=$GENERATED throughput=${THROUGHPUT}prog/min findings=$FINDINGS correctness=$CORRECTNESS slow=$SLOW"

echo "$LINE" >&2
echo '```'
echo "$LINE"
echo '```'
echo ""
if [ "$COMPLETED" -lt "$PLANNED" ]; then
  echo "**$((PLANNED - COMPLETED))** of **$PLANNED** shard(s) did not finish their budget"
  echo "(runner reclaimed). The night still has a verdict — coverage is reduced,"
  echo "not absent. This is reported, never fatal: only findings fail the night."
  echo ""
fi
if [ "$SLOW" -gt 0 ] && [ "$CORRECTNESS" -eq 0 ]; then
  echo "**$SLOW** perf-class Slow finding(s) (#1235: over budget but completed"
  echo "byte-identical at 10x) — tracked under the perf label, night stays green."
  echo ""
fi
echo "| seed | budget | programs | elapsed |"
echo "|------|--------|---------:|--------:|"
printf '%s' "$ROWS"
echo ""
echo "Replay any finding with \`xtarget-fuzz replay --seed S --index I\` (seeds above)."
