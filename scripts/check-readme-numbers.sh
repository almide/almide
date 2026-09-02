#!/usr/bin/env bash
# check-readme-numbers.sh — no undated hand-written count in README.md.
#
# A count in the README is a claim. Claims here are either DERIVED (inside a
# `<!-- name:generated:start … -->` block some script owns) or MEASURED (the
# line carries the date it was measured, so a reader can tell a fresh number
# from a fossil). The "164-contract ledger" that sat at half the ledger's
# size, the "310 test files" at 421, the 703 B Hello, world four releases
# stale — each was a bare number nobody dated and no gate read. This gate
# reads them.
#
# Rule: outside generated blocks, a line that pairs a number with one of the
# COUNTED nouns must also carry a YYYY-MM-DD date, or match an exemption
# below — each exemption names the gate that owns the number instead.
# Generated blocks NEST: the stamped `counts:generated` block (dated in its
# start marker) sits inside the claims and stats blocks, so the skip is a
# depth, not a flag — a flag would resume scanning at the inner end marker.
#
# It also refuses the phrases that stopped being true at commissioning
# (#1599): the wasm path is two verified legs behind a router, not one.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2

fail=0
NOUNS='(contracts?|functions?|modules?|tests?|test files?|fixtures?|theorems?|lemmas?|tasks?|trials?|exercises?|goldens?|mutants?)'
# Owned by proofs/check.sh (its COUNT_DOCS list includes README.md): the
# audited-theorem count is asserted where it is computed.
EXEMPT='audited theorems'

while IFS= read -r hit; do
  echo "::error::README.md:$hit"
  fail=1
done < <(awk -v nouns="$NOUNS" -v exempt="$EXEMPT" '
  /<!-- [a-z-]+:generated:start/ { depth++ }
  /<!-- [a-z-]+:generated:end -->/ { depth--; next }
  depth > 0 { next }
  {
    line = tolower($0)
    if (match(line, "[0-9][0-9,]*[[:space:]]*-?[[:space:]]*" nouns "([^a-z]|$)")) {
      if (line ~ /20[0-9][0-9]-[0-9][0-9](-[0-9][0-9])?/) next
      if (line ~ exempt) next
      print NR ": undated hand-written count (add the measurement date, or derive it in a generated block) — " substr($0, 1, 110)
    }
  }' README.md)

# Fossils: each phrase names something that was true once and is quoted as
# if it still were. Add to the list when a phrase is retired, never remove.
FOSSILS=(
  'the sole wasm path'      # two verified legs behind a router since #1599
  'exactly one wasm path'   # same
  '164-contract'            # the ledger passed 164 long ago; the count is derived now
)
for f in README.md docs/wasm/README.md docs/project/BENCHMARKS.md; do
  [ -f "$f" ] || continue
  for p in "${FOSSILS[@]}"; do
    if grep -nF -- "$p" "$f" >/dev/null; then
      echo "::error::$f quotes '$p' — false since commissioning (#1599): two verified wasm legs behind one router"
      fail=1
    fi
  done
done

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "readme-numbers: every count in README.md is derived or dated; no fossil phrase."
