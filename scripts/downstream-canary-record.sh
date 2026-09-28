#!/usr/bin/env bash
# Advance the downstream canary's known-good ledger from one canary run (#2839).
#
#   scripts/downstream-canary-record.sh <run-id> [--baseline-tag vX.Y.Z] [--unclean]
#
# The DELIBERATE step that moves proofs/downstream-canary-known-good.toml: the
# Release Procedure (CLAUDE.md) runs it after a clean canary, and the result is
# committed like any other ledger. A canary run never writes the ledger itself —
# otherwise a run could record its own failure as the new normal.
#
# What it records: every (project, command) that PASSED on the run's BASELINE
# (always a final release), advancing an entry only to a newer release. A
# baseline failure never removes an entry: "passed on an earlier release" is
# exactly what lets the canary call a fail-on-both command a regression.
#
# It refuses a run that did not conclude `success` (a REGRESSION or an infra
# row). --unclean overrides that for seeding from a historical run whose
# BASELINE facts are still true even though its candidate regressed (e.g. the
# v0.64.0-baseline run that demonstrated #2839). Runs from before the baseline
# tag was stamped into the results need --baseline-tag.
set -euo pipefail

run=${1:?usage: downstream-canary-record.sh <run-id> [--baseline-tag vX.Y.Z] [--unclean]}
shift
tag_args=() unclean=""
while [ $# -gt 0 ]; do
  case $1 in
    --baseline-tag) tag_args=(--baseline-tag "$2"); shift 2 ;;
    --unclean) unclean=yes; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
repo=${GH_REPO:-almide/almide}
root=$(cd "$(dirname "$0")/.." && pwd)

read -r name status conclusion < <(gh run view "$run" -R "$repo" --json workflowName,status,conclusion \
  -q '[(.workflowName|gsub(" ";"_")), .status, .conclusion] | join(" ")')
[ "$name" = Downstream_Canary ] || { echo "run $run is '$name', not a Downstream Canary run" >&2; exit 1; }
[ "$status" = completed ] || { echo "run $run is still $status" >&2; exit 1; }
if [ "$conclusion" != success ] && [ -z "$unclean" ]; then
  echo "run $run concluded '$conclusion' — record only after a clean canary (or pass --unclean to seed from its baseline side)" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
gh run download "$run" -R "$repo" -p 'canary-*' -D "$tmp/dl" >/dev/null
mkdir -p "$tmp/results"
# One artifact per leg: canary-<slug>/<slug>/… → results/<slug>/…
for d in "$tmp"/dl/canary-*/*; do cp -R "$d" "$tmp/results/"; done
python3 "$root/scripts/downstream-canary.py" record "$tmp/results" "${tag_args[@]}"
