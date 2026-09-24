#!/usr/bin/env bash
# NIGHTLY FUZZ VERDICT RENDERER (#924)
# ====================================
#
# Renders one night's SHARDED campaign into (a) a markdown verdict for
# $GITHUB_STEP_SUMMARY on stdout and (b) the greppable per-night record line
# on stderr, so it lands in the job log even with stdout redirected:
#
#   fuzz-night: shards=4/4 reporting=4 recovered=none missing=none
#               minutes_planned=20 minutes_delivered=20.0 delivered_pct=100
#               budget=full generated=4210 throughput=210.5prog/min findings=0
#               findings_recovered=0 correctness=0 slow=0
#
# The field vocabulary is documented once, in scripts/lib/fuzz-night-line.sh,
# which is also the reader the streak scripts use.
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
# BUT THE ABSENCE IS NOT THE DATA (#2513). "A reclaimed runner uploads nothing"
# is true and says nothing about whether its run was recorded: the job LOG is
# not an upload, it survives the kill, and its last progress line
# (`[ 295s] generated=1108 ... findings=0`) carries the seconds fuzzed, the
# programs generated and the findings up to the kill. Run 35702279584 lost four
# shards at 100s/180s/275s/295s and the verdict scored all four as zero minutes:
# 4/8 = 50% instead of the 85% those shards actually delivered, dropping a night
# that WAS over #924's 75% line below it. So every shard with no artifact now
# has its job log read (`repos/<repo>/actions/jobs/<id>/logs`) and, when that
# log holds a progress line, its minutes, programs and findings are folded in
# under `recovered=`. `missing=` narrows to the shards that yielded nothing
# EITHER WAY — a log fetch that fails (network, retention, a job that never
# started) scores as UNKNOWN, never as zero minutes and never as a clean shard,
# and says "could not read shard N's log" so the reason is in the verdict text.
# A recovered `findings=F` is F UP TO that second, never F for the whole budget,
# and the fuzzer's counter does not split correctness from perf-class Slow, so
# `findings_recovered` > 0 makes the night unclassified rather than green.
#
# The log source is a seam: with $FUZZ_SHARD_LOG_DIR set, shard N's log is read
# from `$FUZZ_SHARD_LOG_DIR/shard-N.log` instead of the API, which is how the
# tests forge killed shards without a network (tests/fuzz_night_report_test.rs).
# Without it, recovery needs $GITHUB_REPOSITORY and $GITHUB_RUN_ID (the workflow
# sets both) and a `gh` that can read sibling jobs — the verdict job therefore
# carries `permissions: actions: read`.
#
# THE COUNT IS CONDITIONAL ON WHO CAME BACK (#2390). `findings=F` is the number
# of findings COLLECTED from the shards that uploaded, not the number the night
# found: a shard reclaimed after recording a finding takes it along (run
# 33848741367: 6 of 8 uploaded, 8+4+4+3+2+2 = 23, shard 8's 24th was in its log
# only). The record line used to carry `shards=k/N` next to an unqualified
# `findings=F`, and only the latter reached the issue title. So the heading
# now says `findings=F of r/N shards`, names the shards that did not return
# (`missing=3,8` — their seeds are `run_id * 16 + shard`, replayable from the
# run alone), and states the delivered fuzz-minutes as a percentage of the
# plan (`delivered_pct`, `budget=full|partial` at the 75% line #924 ratified).
# The shard number is read from the artifact directory name
# (`fuzz-shard-<run_id>-<shard>`, the layout `download-artifact` produces).
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
# Lives in a file, not workflow YAML, so it can be run and tested locally
# (tests/fuzz_night_report_test.rs forges a preempted night and reads this
# output back). Division of labour: scripts/fuzz-track-record.sh scores nights
# ACROSS runs; this aggregates the shards WITHIN one night.
#
# When $GITHUB_OUTPUT is set, the qualifying fields are also written there
# (reporting, missing, shards_planned, minutes_planned, minutes_delivered,
# delivered_pct, budget) so the issue-body step can carry them into the issue.
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

# shellcheck source=scripts/lib/fuzz-night-line.sh
. "$(cd "$(dirname "$0")" && pwd)/lib/fuzz-night-line.sh"

DIR="${1:?usage: fuzz-night-verdict.sh <shard-dir> <minutes-per-shard> <shards-planned> <findings> [<slow>]}"
MINUTES="${2:?minutes-per-shard}"
PLANNED="${3:?shards-planned}"
FINDINGS="${4:?findings}"
SLOW="${5:-0}"
CORRECTNESS=$((FINDINGS - SLOW))
MINUTES_PLANNED=$((MINUTES * PLANNED))

emit_outputs() {
  # $1 reporting $2 missing $3 minutes_delivered $4 delivered_pct $5 budget
  # $6 recovered $7 findings_recovered
  [ -n "${GITHUB_OUTPUT:-}" ] || return 0
  {
    echo "reporting=$1"
    echo "missing=$2"
    echo "shards_planned=$PLANNED"
    echo "minutes_planned=$MINUTES_PLANNED"
    echo "minutes_delivered=$3"
    echo "delivered_pct=$4"
    echo "budget=$5"
    echo "recovered=${6:-none}"
    echo "findings_recovered=${7:-0}"
  } >> "$GITHUB_OUTPUT"
}

# This run's job list, fetched once: `fuzz_shard_log` (scripts/lib) turns a
# shard NUMBER into a job id through it. Per-job logs are readable while the run
# is still going, which is what makes this usable from the verdict job of the
# same run. Failing to fetch it is not an error here — every shard then reads as
# unreadable and is NAMED as such, which is the honest outcome.
JOBS_JSON=""
shard_job_log() {
  local repo="${GITHUB_REPOSITORY:-}" run="${GITHUB_RUN_ID:-}"
  if [ -z "${FUZZ_SHARD_LOG_DIR:-}" ] && [ -z "$JOBS_JSON" ] && [ -n "$repo" ] && [ -n "$run" ]; then
    JOBS_JSON=$(gh api "repos/$repo/actions/runs/$run/jobs?per_page=100" 2>/dev/null || true)
  fi
  fuzz_shard_log "$repo" "$JOBS_JSON" "$1"
}

# One shard = one fuzz-output.txt anywhere under DIR. A killed shard uploaded
# nothing, so it is absent here — that is exactly what `shards=k/N` reports.
mapfile -t OUTS < <(find "$DIR" -name fuzz-output.txt -type f 2>/dev/null | sort)
REPORTING=${#OUTS[@]}

# Which shard NUMBERS came back: the trailing `-<n>` of the artifact directory.
# A layout that does not carry the number (a hand-built local exercise) leaves
# the missing list `unknown` rather than guessing.
SEEN=" "
UNNUMBERED=0
for out in "${OUTS[@]}"; do
  d=$(basename "$(dirname "$out")")
  n="${d##*-}"
  if [[ "$n" =~ ^[0-9]+$ ]]; then SEEN="$SEEN$n "; else UNNUMBERED=1; fi
done
MISSING=""
if [ "$UNNUMBERED" -eq 1 ]; then
  MISSING="unknown"
else
  for ((i = 1; i <= PLANNED; i++)); do
    case "$SEEN" in *" $i "*) ;; *) MISSING="${MISSING:+$MISSING,}$i" ;; esac
  done
  [ -n "$MISSING" ] || MISSING="none"
fi

# No shard uploaded ANYTHING. Recovery is not attempted here on purpose: the
# workflow's own "Fail the night when no shard reported" step has already failed
# the night as INFRA by then (the #976 vacuous-pass rule), and a log-recovered
# percentage must not be printed next to a `budget=` the streak readers would
# score against a failed verdict job. The shards' logs still hold their lines;
# what is missing is a ruling on whether a night nobody uploaded can be scored
# at all, and that is not this script's to make.
if [ "$REPORTING" -eq 0 ]; then
  LINE="fuzz-night: shards=0/$PLANNED reporting=0 recovered=none missing=$MISSING minutes_planned=$MINUTES_PLANNED minutes_delivered=0 delivered_pct=0 budget=partial generated=0 findings=$FINDINGS findings_recovered=0 correctness=$CORRECTNESS slow=$SLOW"
  echo "$LINE" >&2
  emit_outputs 0 "$MISSING" 0 0 partial none 0
  echo "## Nightly fuzz verdict — findings=$FINDINGS of 0/$PLANNED shards"
  echo ""
  echo '```'; echo "$LINE"; echo '```'
  echo ""
  echo "No shard reported: every runner was reclaimed before its campaign could"
  echo "upload. A night with zero evidence cannot be green — that would be the"
  echo "vacuous pass (#976 class) — so this is an INFRA failure, distinct from"
  echo "a finding."
  exit 1
fi

COMPLETED=0
GENERATED=0
ELAPSED=0
SEEDS=""
ROWS=""
for out in "${OUTS[@]}"; do
  d=$(basename "$(dirname "$out")")
  n="${d##*-}"
  [[ "$n" =~ ^[0-9]+$ ]] || n="?"
  seed=$(grep -oE "seed += +[0-9]+" "$out" | tr -s ' ' | cut -d' ' -f3 | head -1 || true)
  if grep -q "^=== campaign summary ===" "$out"; then
    COMPLETED=$((COMPLETED + 1))
    g=$(awk '/^  generated /{print $3; exit}' "$out")
    e=$(awk '/^  elapsed /{gsub(/s$/,"",$3); print $3; exit}' "$out")
    GENERATED=$((GENERATED + ${g:-0}))
    ELAPSED=$(awk -v a="$ELAPSED" -v b="${e:-0}" 'BEGIN{printf "%.1f", a+b}')
    ROWS="${ROWS}| $n | ${seed:-?} | complete | ${g:-?} | ${e:-?}s |"$'\n'
  else
    # Truncated: credit what its last progress line saw, so a reclaimed shard
    # still contributes its real coverage instead of being counted as zero.
    last=$(grep -E "^ *\[ *[0-9]+s\]" "$out" | tail -1 || true)
    g=$(echo "$last" | grep -oE 'generated=[0-9]+' | cut -d= -f2 || true)
    e=$(echo "$last" | grep -oE '\[ *[0-9]+s\]' | grep -oE '[0-9]+' || true)
    GENERATED=$((GENERATED + ${g:-0}))
    ELAPSED=$(awk -v a="$ELAPSED" -v b="${e:-0}" 'BEGIN{printf "%.1f", a+b}')
    ROWS="${ROWS}| $n | ${seed:-?} | truncated | ${g:-?} | ${e:-?}s |"$'\n'
  fi
  SEEDS="${SEEDS}${seed:-?} "
done

# ── RECOVERY (#2513) ──────────────────────────────────────────────────────
# Every shard with no artifact, read back out of its job log. What the log
# yields is folded into the night's totals exactly as a truncated artifact is;
# what it does not yield stays in `missing=` and is said out loud.
RECOVERED=""
RECOVERED_FINDINGS=0
RECOVERED_SECONDS=0
UNREADABLE=""
if [ "$MISSING" != "none" ] && [ "$MISSING" != "unknown" ]; then
  for n in ${MISSING//,/ }; do
    log=$(shard_job_log "$n" || true)
    read -r secs gen finds <<<"$(printf '%s\n' "$log" | fuzz_last_progress)" || true
    if [ -z "${secs:-}" ]; then
      UNREADABLE="${UNREADABLE:+$UNREADABLE,}$n"
      echo "could not read shard $n's log — its minutes and findings stay UNKNOWN, not zero" >&2
      continue
    fi
    seed=$(printf '%s\n' "$log" | grep -oE "seed += +[0-9]+" | tr -s ' ' | cut -d' ' -f3 | head -1 || true)
    RECOVERED="${RECOVERED:+$RECOVERED,}$n@${secs}s"
    RECOVERED_SECONDS=$((RECOVERED_SECONDS + secs))
    RECOVERED_FINDINGS=$((RECOVERED_FINDINGS + finds))
    GENERATED=$((GENERATED + gen))
    ELAPSED=$(awk -v a="$ELAPSED" -v b="$secs" 'BEGIN{printf "%.1f", a+b}')
    ROWS="${ROWS}| $n | ${seed:-?} | recovered | ${gen} | ${secs}s |"$'\n'
    SEEDS="${SEEDS}${seed:-?} "
  done
fi
[ -n "$RECOVERED" ] || RECOVERED="none"
# `missing=` is now only what nothing could be read for, either way.
case "$MISSING" in
  unknown) ;;
  *) MISSING="${UNREADABLE:-none}" ;;
esac
# The night's findings are the uploaded ones PLUS what the recovered logs had
# counted by the second they were killed at. Recovered findings can overlap a
# name the aggregate already deduplicated, so the total is an upper bound —
# which is the safe direction: it can only make a night look less clean.
FINDINGS_TOTAL=$((FINDINGS + RECOVERED_FINDINGS))

DELIVERED=$(awk -v e="$ELAPSED" 'BEGIN{printf "%.1f", e/60}')
THROUGHPUT=$(awk -v g="$GENERATED" -v e="$ELAPSED" 'BEGIN{printf "%.1f", (e>0)? g*60/e : 0}')
PCT=$(awk -v p="$MINUTES_PLANNED" -v d="$DELIVERED" 'BEGIN{printf "%d", (p>0)? (100*d)/p : 0}')
if [ "$PCT" -ge "$FUZZ_NIGHT_BUDGET_PCT" ]; then BUDGET=full; else BUDGET=partial; fi
LINE="fuzz-night: shards=$COMPLETED/$PLANNED reporting=$REPORTING recovered=$RECOVERED missing=$MISSING minutes_planned=$MINUTES_PLANNED minutes_delivered=$DELIVERED delivered_pct=$PCT budget=$BUDGET generated=$GENERATED throughput=${THROUGHPUT}prog/min findings=$FINDINGS_TOTAL findings_recovered=$RECOVERED_FINDINGS correctness=$CORRECTNESS slow=$SLOW"

echo "$LINE" >&2
emit_outputs "$REPORTING" "$MISSING" "$DELIVERED" "$PCT" "$BUDGET" "$RECOVERED" "$RECOVERED_FINDINGS"

# The heading carries the condition: a count is only as good as the shards it
# was collected from, and a reader who sees the number sees its denominator.
# A recovered shard is counted apart from the ones that uploaded: its numbers
# are real, and they stop where the runner stopped it.
RECOVERED_N=0
case "$RECOVERED" in none) ;; *) RECOVERED_N=$(printf '%s' "$RECOVERED" | tr ',' '\n' | grep -c .) ;; esac
if [ "$RECOVERED_N" -gt 0 ]; then
  echo "## Nightly fuzz verdict — findings=$FINDINGS_TOTAL of $REPORTING/$PLANNED shards (+$RECOVERED_N recovered from job logs)"
else
  echo "## Nightly fuzz verdict — findings=$FINDINGS_TOTAL of $REPORTING/$PLANNED shards"
fi
echo ""
echo '```'
echo "$LINE"
echo '```'
echo ""
if [ "$RECOVERED_N" -gt 0 ]; then
  echo "**$RECOVERED_N** shard(s) uploaded nothing and were RECOVERED FROM THEIR JOB LOGS"
  echo "(${RECOVERED//,/, }): a reclaimed runner uploads nothing, but its log is not an"
  echo "upload — it outlives the job and still carries the campaign's last progress"
  echo "line. Their seconds, programs and findings are in the numbers above."
  echo ""
  echo "Each recovered count is **only up to the second it is tagged with**, never for"
  echo "that shard's whole budget: the shard fuzzed no further, so nothing after that"
  echo "second was examined at all."
  if [ "$RECOVERED_FINDINGS" -gt 0 ]; then
    echo ""
    echo "**$RECOVERED_FINDINGS finding(s) came back with them** (\`findings_recovered\`), and the"
    echo "fuzzer's progress counter does not say which class they are — so they are"
    echo "unclassified until someone reads those logs, and this night is NOT green."
    echo "The repro is the derived seed: \`xtarget-fuzz replay --seed <run_id * 16 + shard>\`."
  else
    echo "No finding had been recorded in any of those logs by the second it stops at;"
    echo "what happened afterwards was not run, so it is not a claim about it."
  fi
  echo ""
  echo "::notice::$RECOVERED_N of $PLANNED fuzz shard(s) uploaded nothing and were recovered from their job logs ($RECOVERED); findings_recovered=$RECOVERED_FINDINGS, counted only to the second named" >&2
fi
MISSING_N=0
case "$MISSING" in
  none) ;;
  unknown) MISSING_N=$((PLANNED - REPORTING)) ;;
  *) MISSING_N=$(printf '%s' "$MISSING" | tr ',' '\n' | grep -c .) ;;
esac
if [ "$MISSING_N" -gt 0 ]; then
  echo "**$FINDINGS_TOTAL** finding(s) collected from **$REPORTING of $PLANNED** shards plus"
  case "$MISSING" in
    unknown)
      WHICH=""
      echo "**$RECOVERED_N** recovered log(s) — **$MISSING_N** shard(s) returned nothing, and this"
      echo "shard layout carries no shard numbers, so no job log could be looked up for"
      echo "them. Their findings, if any, are NOT in this count and their minutes are"
      echo "UNKNOWN, not zero."
      ;;
    *)
      WHICH=" (shards ${MISSING//,/, })"
      echo "**$RECOVERED_N** recovered log(s) — **$MISSING_N** shard(s)${WHICH} returned nothing AND"
      echo "their job log could not be read either, so their findings, if any, are NOT in"
      echo "this count and their minutes are UNKNOWN, not zero. Those logs, if they come"
      echo "back, still hold every \`** FINDING\` line, and the seed is derived"
      echo "(\`run_id * 16 + shard\`), so the missing evidence is replayable from the run alone."
      ;;
  esac
  echo ""
  echo "::warning::$MISSING_N of $PLANNED fuzz shard(s) yielded nothing at all${WHICH} — neither an artifact nor a readable job log; findings=$FINDINGS_TOTAL does not include them" >&2
fi
echo "Delivered **$DELIVERED** of **$MINUTES_PLANNED** planned fuzz-minutes (**$PCT%**, budget=$BUDGET"
echo "at the ${FUZZ_NIGHT_BUDGET_PCT}% line #924 counts a streak night by)."
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
echo "| shard | seed | budget | programs | elapsed |"
echo "|------:|------|--------|---------:|--------:|"
printf '%s' "$ROWS"
echo ""
echo "Replay any finding with \`xtarget-fuzz replay --seed S --index I\` (seeds above)."
