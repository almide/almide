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
# The number printed is the count of DISTINCT open issues (#2400). One issue
# may carry several classes (the taxonomy allows `I-divergence` + `regression`),
# and summing the per-label rows counted such an issue once per label: four
# open blockers read as 7. The per-label breakdown is still printed so the
# overlap is visible; it is no longer what the total is made of.
#
# Usage:
#   count-release-blockers.sh                # report; exit 0 always
#   count-release-blockers.sh --gate         # exit 1 when any blocker is open
#   count-release-blockers.sh --from-stdin   # take the listing from stdin
#                                            # instead of `gh` (test seam)
#
# Exit 2 in either mode when `gh` is missing or its listing fails: no
# measurement, no verdict — never a printed 0.
#
# The stdin listing is one line per (label, issue) membership, tab-separated:
#   <label>\t<number>\t<title>
# — the exact shape `gh` is asked for, so the counting core runs the same code
# on a forged overlap as on the live tracker (tests/release_blocker_count_test.rs).
#
# Needs `gh` with repo read access (present in Actions via GITHUB_TOKEN).
set -euo pipefail
export LC_ALL=C

REPO="${ALMIDE_REPO:-almide/almide}"
BLOCKING_LABELS=(I-unsound I-miscompile I-divergence regression)

gate=0
from_stdin=0
for arg in "$@"; do
  case "$arg" in
    --gate) gate=1 ;;
    --from-stdin) from_stdin=1 ;;
    *)
      echo "count-release-blockers: unknown argument '$arg' (--gate, --from-stdin)" >&2
      exit 2
      ;;
  esac
done

# One line per (label, issue) membership: label \t number \t title.
#
# A `gh` failure (token, network, rate limit) is NOT an empty tracker: a
# measurement that never ran must not print as zero, because `release-blockers: 0`
# is what lets a final tag through. Fail loudly, in report mode too.
fetch_listing() {
  local label out rc
  for label in "${BLOCKING_LABELS[@]}"; do
    out=$(gh issue list --repo "$REPO" --state open --label "$label" \
      --json number,title \
      --jq ".[] | [\"$label\", (.number|tostring), .title] | @tsv") && rc=0 || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "::error::count-release-blockers: gh listing failed (exit $rc) — no verdict" >&2
      return 2
    fi
    printf '%s\n' "$out"
  done
}

if [ "$from_stdin" -eq 1 ]; then
  listing=$(cat)
else
  if ! command -v gh >/dev/null; then
    echo "::error::count-release-blockers: gh not found — cannot measure; refusing to answer 0" >&2
    exit 2
  fi
  listing=$(fetch_listing) || exit 2
fi

# Per-label breakdown: every membership, grouped under its label.
for label in "${BLOCKING_LABELS[@]}"; do
  rows=$(printf '%s\n' "$listing" \
    | awk -F'\t' -v l="$label" '$1 == l { printf "  #%s  %s\n", $2, $3 }')
  if [ -n "$rows" ]; then
    count=$(printf '%s\n' "$rows" | wc -l | tr -d ' ')
    echo "$label ($count):"
    printf '%s\n' "$rows"
  fi
done

# The total is the size of the SET of issue numbers, not the sum of the rows.
numbers=$(printf '%s\n' "$listing" | awk -F'\t' 'NF >= 2 { print $2 }')
memberships=$(printf '%s\n' "$numbers" | grep -c . || true)
distinct=$(printf '%s\n' "$numbers" | sort -u | grep -c . || true)
multi=$(printf '%s\n' "$numbers" | sort | uniq -c | awk '$1 > 1' | grep -c . || true)

echo "label-memberships: $memberships across $distinct issues ($multi carry more than one blocker label)"
echo "release-blockers: $distinct"
if [ "$gate" -eq 1 ] && [ "$distinct" -gt 0 ]; then
  echo "release-blockers: a FINAL tag must not ship over an open blocker —" >&2
  echo "fix it, or demote its label through the taxonomy's amendment path." >&2
  exit 1
fi
