#!/usr/bin/env bash
# NIGHTLY FUZZ TRACK RECORD (#924 / #796)
# =======================================
#
# Scores the last N *scheduled* fuzz-nightly runs into one verdict per night,
# read from the same job/step conclusions the workflow itself produces:
#
#   GREEN       run concluded success — full budget, zero findings
#   FINDINGS    the campaign completed its budget and the fuzzer recorded
#               finding(s): the instrument working, not an infra failure
#   TRUNCATED   the campaign step never concluded success (runner reclaimed,
#               build failure, or a pre-split night with a different job shape)
#
# "Completed its budget" is read from the "Run fuzz campaign" step conclusion:
# that step's script only reaches its exit after the campaign returns, so a
# reclaimed runner cannot leave it `success`. The per-night numbers (programs,
# throughput) live on the `fuzz-night:` line in each run's step summary/log.
#
# Closure conditions this makes checkable with one command:
#   #924 — 14 consecutive full-budget nights (GREEN or FINDINGS)
#   #796 —  2 consecutive green nights       (GREEN)
#
# Reporting tool, NOT a CI gate: needs `gh` auth and network.
#
# Usage: scripts/fuzz-track-record.sh [N]    (default: 20 nights)

set -euo pipefail

REPO="${FUZZ_REPO:-almide/almide}"
N="${1:-20}"

FUZZ_JOB="Generative differential fuzz"
CAMPAIGN_STEP="Run fuzz campaign"

runs=$(gh api "repos/$REPO/actions/workflows/fuzz-nightly.yml/runs?event=schedule&per_page=$N" \
  --jq '.workflow_runs[] | [.id, (.conclusion // .status), .created_at] | @tsv')

full_streak=0
green_streak=0
full_done=0
green_done=0

printf '%-12s %-13s %-11s %s\n' "date" "run" "conclusion" "verdict"

while IFS=$'\t' read -r id conclusion created; do
  date="${created%%T*}"

  if [ "$conclusion" = "in_progress" ] || [ "$conclusion" = "queued" ]; then
    printf '%-12s %-13s %-11s %s\n' "$date" "$id" "$conclusion" "IN PROGRESS (not scored)"
    continue
  fi

  step=$(gh api "repos/$REPO/actions/runs/$id/jobs" --jq \
    "[.jobs[] | select(.name == \"$FUZZ_JOB\") | .steps[] \
      | select(.name == \"$CAMPAIGN_STEP\") | .conclusion][0] // \"absent\"")

  if [ "$conclusion" = "success" ]; then
    verdict="GREEN"
  elif [ "$step" = "success" ]; then
    verdict="FINDINGS (full budget, red on findings)"
  else
    verdict="TRUNCATED (campaign step: $step)"
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

  printf '%-12s %-13s %-11s %s\n' "$date" "$id" "$conclusion" "$verdict"
done <<<"$runs"

echo
echo "full-budget streak: $full_streak/14  (#924 closes at 14)"
echo "green streak:       $green_streak/2   (#796 needs 2 consecutive)"
