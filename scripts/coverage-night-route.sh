#!/usr/bin/env bash
# NIGHTLY COVERAGE GATES → A LABELLED ISSUE (#3195)
# ==================================================
#
# The three #566 coverage jobs in fuzz-nightly.yml (the line-mode ratchet, the
# per-condition ratchet and the MC/DC mutation gate) used to report only as a
# red job inside the night's run. Nobody reads that page every day: the
# per-condition ratchet was red on every night of the v0.66.0 window (#3195)
# and twice before (#2707, #2941) before anyone acted. The fuzz findings
# learned the same lesson in #2379 and route to a `fuzz-findings` issue; this
# is that shape for the coverage gates.
#
# When any #566 job of the run did not succeed, this opens an issue labelled
# `coverage-ratchet`, or comments on the open one, with each red job's verdict
# lines (TOTAL vs floor, safety-file floors, surviving mutants) and a link.
# A green night does nothing.
#
# Usage: coverage-night-route.sh <run-id> [--dry-run]
#   --dry-run  read the run and its logs, print the issue title / body and the
#              action it WOULD take (create, or comment on #N); write nothing
# Env: GITHUB_REPOSITORY (default almide/almide), GH_TOKEN / GITHUB_TOKEN.

set -euo pipefail
export LC_ALL=C

RUN="${1:?run id}"
DRY=0
[ "${2:-}" = "--dry-run" ] && DRY=1
REPO="${GITHUB_REPOSITORY:-almide/almide}"
LABEL="coverage-ratchet"

run_meta="$(gh api "repos/$REPO/actions/runs/$RUN" --jq '[.head_branch, .head_sha[0:9], .event, .html_url] | @tsv')"
IFS=$'\t' read -r ref sha event run_url <<< "$run_meta"

# Every #566 job of this run with its conclusion. Matched by the `(#566)`
# suffix all three job names carry, so a renamed job drops out loudly below
# (zero jobs is an error, never a quiet green).
jobs="$(gh api --paginate "repos/$REPO/actions/runs/$RUN/jobs?per_page=100" \
  --jq '.jobs[] | select(.name | endswith("(#566)")) | [.id, (.conclusion // "unfinished"), .name, .html_url] | @tsv')"
if [ -z "$jobs" ]; then
    echo "::error::coverage-night-route: run $RUN has no '(#566)' job — renamed or removed; the routing went blind"
    exit 1
fi

red=""
while IFS=$'\t' read -r id concl name url; do
    case "$concl" in success|skipped) continue ;; esac
    red="$red$id"$'\t'"$concl"$'\t'"$name"$'\t'"$url"$'\n'
done <<< "$jobs"

if [ -z "$red" ]; then
    echo "coverage-night-route: every #566 job of run $RUN succeeded — nothing to route"
    exit 0
fi

body="$(mktemp)"
trap 'rm -f "$body"' EXIT
{
    echo "The nightly coverage gates went red on \`$ref\` ($sha, $event): [run]($run_url)."
    echo
    while IFS=$'\t' read -r id concl name url; do
        [ -n "$id" ] || continue
        echo "### [$name]($url): $concl"
        echo
        echo '```'
        # The verdict lines each gate prints; the timestamp prefix and ANSI
        # colour are stripped. A job that died before its verdict (a timeout,
        # a build failure) has none, and its log tail stands in for them.
        log="$(gh api "repos/$REPO/actions/jobs/$id/logs" 2>/dev/null | sed -E 's/^[0-9T:.Z-]+ //; s/\x1b\[[0-9;]*m//g' || true)"
        hits="$(printf '%s\n' "$log" | grep -E 'COVERAGE RATCHET FAIL|SAFETY COVERAGE FAIL|TOTAL line coverage|^coverage: |^::error::|##\[error\]|mutant|SURVIV' | head -25 || true)"
        if [ -n "$hits" ]; then printf '%s\n' "$hits"; else printf '%s\n' "$log" | tail -15; fi
        echo '```'
        echo
    done <<< "$red"
    echo "The per-file table is in each job's log under \"per-file coverage\". Restore the coverage with tests or a workload; lower a floor only for deleted code, per file, in its own commit (proofs/COVERAGE.md)."
} > "$body"

title="Nightly coverage gate red on $ref"
existing="$(gh issue list -R "$REPO" --label "$LABEL" --state open --json number --jq '.[0].number // empty' 2>/dev/null || true)"

if [ "$DRY" = "1" ]; then
    if [ -n "$existing" ]; then echo "DRY RUN: would comment on #$existing (label $LABEL)"
    else echo "DRY RUN: would create \"$title\" with label $LABEL"; fi
    echo "----- body -----"
    cat "$body"
    exit 0
fi

if [ -n "$existing" ]; then
    gh issue comment -R "$REPO" "$existing" --body-file "$body"
else
    gh label create -R "$REPO" "$LABEL" --color B60205 --description "A nightly #566 coverage gate is red (line ratchet, per-condition ratchet, MC/DC mutants)" 2>/dev/null || true
    gh issue create -R "$REPO" --title "$title" --body-file "$body" --label "$LABEL"
fi
