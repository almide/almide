#!/usr/bin/env bash
# ENQUEUE WHEN THE REQUIRED CHECKS ARE GREEN — not when everything is.
#
# The merge queue for develop requires a fixed set of status checks (the branch
# protection's `required_status_checks.contexts`, 17 of them on 2026-09-21).
# The commissioned mutation gate is NOT one of them: it runs only on
# pull_request events, and the full sweep (`mutation-sweep.yml`) judges every
# develop push after landing. Waiting for "all green" therefore waits for the
# one job the queue never asks about — measured on #2399's run it was the
# critical path at 56 min against 27 min for the slowest required shard.
#
# This script asks for the required set, compares the PR's rollup against
# exactly that set, and enqueues through the GraphQL mutation when every
# required context is SUCCESS (or SKIPPED, which the queue counts as passing).
# An auto-merge armed with `gh pr merge --auto` also enters the queue by
# itself once the LAST check goes green (its method reads MERGE against a
# REBASE queue — GitHub normalises the field, and that is harmless; measured
# on #2404, 2026-09-21). What the arm waits for is every check, this waits
# for the required ones; "already in the queue" is therefore a success here.
#
# Usage: scripts/enqueue-when-required-green.sh <pr-number> [--wait]
#   --wait   poll every 60 s until the required set is green (or one fails),
#            then enqueue; without it, report and enqueue only if green now.
set -euo pipefail
export LC_ALL=C # sort order must not depend on the machine (#1031)
PR="${1:?pr number}"
WAIT="${2:-}"
REPO="${GH_REPO:-almide/almide}"
BASE="$(gh pr view "$PR" --repo "$REPO" --json baseRefName --jq .baseRefName)"

required="$(gh api "repos/$REPO/branches/$BASE/protection" --jq '.required_status_checks.contexts[]' | sort -u)"
[ -n "$required" ] || { echo "::error::no required checks found for $BASE — refusing to guess"; exit 2; }

verdict() {
  # prints: pending=<n> failed=<names> missing=<names>
  gh pr view "$PR" --repo "$REPO" --json statusCheckRollup --jq '.statusCheckRollup[]|"\(.name // .context)\t\(.conclusion // .status // "?")"' \
    | REQUIRED="$required" python3 -c '
import os, sys
want = set(l for l in os.environ["REQUIRED"].split("\n") if l)
seen, bad, pend = set(), [], 0
for line in sys.stdin:
    name, _, state = line.rstrip("\n").partition("\t")
    if name not in want:
        continue
    seen.add(name); s = state.upper()
    if s in ("SUCCESS", "SKIPPED", "NEUTRAL"):
        pass
    elif s in ("FAILURE", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED"):
        bad.append(name)
    else:
        pend += 1
miss = sorted(want - seen)
print("pending=%d" % pend); print("failed=%s" % ", ".join(bad)); print("missing=%s" % ", ".join(miss))'
}

enqueue() {
  local id out; id="$(gh pr view "$PR" --repo "$REPO" --json id --jq .id)"
  if out="$(gh api graphql -f query="mutation{enqueuePullRequest(input:{pullRequestId:\"$id\"}){clientMutationId}}" 2>&1)"; then
    echo "enqueued #$PR — required set green ($(echo "$required" | wc -l | tr -d ' ') contexts); the mutation sweep judges develop after landing"
  elif echo "$out" | grep -qi "already in the queue"; then
    echo "#$PR is already in the queue (an armed auto-merge entered it) — nothing to do"
  else
    echo "::error::enqueue of #$PR failed: $out"; return 1
  fi
}

while :; do
  v="$(verdict)"
  pend="$(echo "$v" | sed -n 's/^pending=//p')"
  failed="$(echo "$v" | sed -n 's/^failed=//p')"
  missing="$(echo "$v" | sed -n 's/^missing=//p')"
  if [ -n "$failed" ]; then echo "::error::#$PR has a FAILED required check: $failed — not enqueuing"; exit 1; fi
  if [ "$pend" = "0" ] && [ -z "$missing" ]; then enqueue; exit 0; fi
  echo "#$PR: required pending=$pend missing=[$missing] ($(date -u +%H:%M:%SZ))"
  [ "$WAIT" = "--wait" ] || exit 3
  sleep 60
done
