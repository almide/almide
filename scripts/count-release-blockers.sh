#!/usr/bin/env bash
# RELEASE-BLOCKER COUNT (#1482): how many OPEN issues carry a severity class
# that blocks a FINAL release tag. The classes are the closed set defined in
# docs/project/ISSUE-TAXONOMY.md:
#
#   I-unsound     the guarantee spine is violated (cert hole, UAF/double-free)
#   I-miscompile  an accepted program computes a wrong value on some target
#   I-divergence  native/wasm/interp disagree on observable output
#   regression    worked in a released version, broken at HEAD
#
# Priority labels (P-*) never block by themselves. Which classes block is a
# ratified default, amendable only through a mob decision recorded in the
# taxonomy doc — not per release, not in this script.
#
# Usage:
#   count-release-blockers.sh          # report; exit 0 always
#   count-release-blockers.sh --gate   # exit 1 when any blocker is open
#
# Needs `gh` with repo read access (present in Actions via GITHUB_TOKEN).
set -euo pipefail

REPO="${ALMIDE_REPO:-almide/almide}"
BLOCKING_LABELS=(I-unsound I-miscompile I-divergence regression)

if ! command -v gh >/dev/null; then
  echo "count-release-blockers: gh not found — cannot measure; refusing to answer 0" >&2
  exit 2
fi

total=0
for label in "${BLOCKING_LABELS[@]}"; do
  rows=$(gh issue list --repo "$REPO" --state open --label "$label" \
    --json number,title --jq '.[] | "  #\(.number)  \(.title)"' 2>/dev/null || true)
  if [ -n "$rows" ]; then
    count=$(printf '%s\n' "$rows" | wc -l | tr -d ' ')
    total=$((total + count))
    echo "$label ($count):"
    printf '%s\n' "$rows"
  fi
done

echo "release-blockers: $total"
if [ "${1:-}" = "--gate" ] && [ "$total" -gt 0 ]; then
  echo "release-blockers: a FINAL tag must not ship over an open blocker —" >&2
  echo "fix it, or demote its label through the taxonomy's amendment path." >&2
  exit 1
fi
