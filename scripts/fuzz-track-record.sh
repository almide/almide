#!/usr/bin/env bash
# NIGHTLY FUZZ TRACK RECORD (#924 / #796)
# =======================================
#
# Scores the last N *scheduled* fuzz-nightly runs into one verdict per night,
# read from the same job/step conclusions the workflow itself produces:
#
#   GREEN       the night delivered a verdict and found nothing
#   FINDINGS    the night delivered a verdict and the fuzzer recorded
#               finding(s): the instrument working, not an infra failure
#   NO VERDICT  the verdict job itself never concluded (a whole-run failure)
#
# SHARDED NIGHTS. The campaign runs as N parallel shards, each its own job and
# so its own runner lifetime, aggregated by a `Night verdict` job that runs
# `if: always()`. A reclaimed shard costs 1/N of the night's coverage and
# nothing else — the verdict is still produced, and the shard count it reports
# (`shards=k/N` on the `fuzz-night:` line) is the coverage metric. That is why
# the verdict is scored from the VERDICT job here, not from any one shard: a
# night is only unscored if the aggregation itself failed to run.
#
# COVERAGE, not completion, is the honest streak metric. Before sharding, one
# reclaimed runner zeroed the night, and a ~1-in-6 kill rate made #924's "14
# consecutive full-budget nights" a (5/6)^14 ≈ 8% proposition — unreachable by
# arithmetic. The `coverage` column below reads `shards=k/N` from each night's
# record line so the streak can be stated against delivered coverage instead.
#
# Closure conditions this makes checkable with one command:
#   #924 — 14 consecutive nights with a verdict (see the coverage column)
#   #796 —  2 consecutive green nights
#
# Reporting tool, NOT a CI gate: needs `gh` auth and network.
#
# Usage: scripts/fuzz-track-record.sh [N]    (default: 20 nights)

set -euo pipefail

REPO="${FUZZ_REPO:-almide/almide}"
N="${1:-20}"

VERDICT_JOB="Night verdict"
# Pre-2026-08-08 nights ran a single unsharded campaign job; keep reading them
# so the history above the sharding change stays scoreable.
LEGACY_JOB="Generative differential fuzz"
LEGACY_STEP="Run fuzz campaign"

runs=$(gh api "repos/$REPO/actions/workflows/fuzz-nightly.yml/runs?event=schedule&per_page=$N" \
  --jq '.workflow_runs[] | [.id, (.conclusion // .status), .created_at] | @tsv')

full_streak=0
green_streak=0
full_done=0
green_done=0

printf '%-12s %-13s %-11s %-9s %s\n' "date" "run" "conclusion" "shards" "verdict"

while IFS=$'\t' read -r id conclusion created; do
  date="${created%%T*}"

  if [ "$conclusion" = "in_progress" ] || [ "$conclusion" = "queued" ]; then
    printf '%-12s %-13s %-11s %-9s %s\n' "$date" "$id" "$conclusion" "-" "IN PROGRESS (not scored)"
    continue
  fi

  jobs=$(gh api "repos/$REPO/actions/runs/$id/jobs")
  vjob=$(jq -r "[.jobs[] | select(.name == \"$VERDICT_JOB\") | .conclusion][0] // \"absent\"" <<<"$jobs")

  if [ "$vjob" = "absent" ]; then
    # Legacy (unsharded) night: score it the old way, from the campaign step.
    step=$(jq -r "[.jobs[] | select(.name == \"$LEGACY_JOB\") | .steps[] \
      | select(.name == \"$LEGACY_STEP\") | .conclusion][0] // \"absent\"" <<<"$jobs")
    if [ "$conclusion" = "success" ]; then verdict="GREEN"
    elif [ "$step" = "success" ]; then verdict="FINDINGS (full budget, red on findings)"
    else verdict="TRUNCATED (campaign step: $step)"; fi
    coverage="1/1"
  else
    # Sharded night: k of N shards reported a completed budget. `shards=k/N`
    # is written by scripts/fuzz-night-verdict.sh into the verdict job's log.
    coverage=$(jq -r "[.jobs[] | select(.name | startswith(\"Generative differential fuzz\")) \
      | .conclusion] | \"\(map(select(. == \"success\")) | length)/\(length)\"" <<<"$jobs")
    case "$vjob" in
      success) verdict="GREEN" ;;
      failure) verdict="FINDINGS (verdict delivered, red on findings)" ;;
      *)       verdict="NO VERDICT (verdict job: $vjob)" ;;
    esac
  fi

  case "$verdict" in
    GREEN)
      if [ "$green_done" -eq 0 ]; then green_streak=$((green_streak + 1)); fi
      if [ "$full_done" -eq 0 ]; then full_streak=$((full_streak + 1)); fi
      ;;
    FINDINGS*)
      green_done=1
      if [ "$full_done" -eq 0 ]; then full_streak=$((full_streak + 1)); fi
      ;;
    *)
      green_done=1
      full_done=1
      ;;
  esac

  printf '%-12s %-13s %-11s %-9s %s\n' "$date" "$id" "$conclusion" "${coverage:-?}" "$verdict"
done <<<"$runs"

echo
echo "verdict streak:     $full_streak/14  (#924 closes at 14)"
echo "green streak:       $green_streak/2   (#796 needs 2 consecutive)"
