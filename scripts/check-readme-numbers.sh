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
#
# And it carries the MSR row (#2146 item 4): the LLM-writability scorecard —
# the number the page's strongest claim rests on — must be dated, traceable to
# the run that produced it, and younger than 90 days. A stale number is a red
# gate, not a footnote.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/.." || exit 2

# shellcheck source=lib/readme-freshness.sh
. "$here/lib/readme-freshness.sh"

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

# ── The MSR row (#2146 item 4, #1617) ──
#
# The rest of this gate asks a measured number to carry a date. That is not
# enough for the one number the tagline rests on: a date makes the reader do
# the arithmetic, and for five months nobody did (the scorecard was measured
# 2026-04-12 and read as current until #1617 was filed). Here the gate does
# the arithmetic, and asks for the two things that make the number checkable
# by someone who is not us:
#
#   * a citation of the almide-dojo commit that holds the run, so the table
#     can be recomputed from the committed `summary.md` / `table.md` rather
#     than believed, and
#   * that run's YYYY-MM-DD on the SAME line as the citation, no older than
#     the 90-day line. Same line, because the section also carries the
#     MiniGit bench's date, and a section-wide search would happily age the
#     MSR claim against a different measurement's date.
#
# No key and no network: the date and the sha are in README.md. The line and
# the section list come from scripts/lib/readme-freshness.sh, which the
# nightly delivery gate reads too.
#
# When this goes red the fix is the MEASUREMENT, not the date: almide-dojo
# owns it (`make msr` / the Cross-language MSR Run workflow), and no Anthropic
# key is needed for either — by ruling, the scheduled lanes run on the
# Cloudflare models.
readme_msr_report || fail=1
# ...and the section rule too, so a scorecard that loses its prose date
# entirely is caught even before the citation rule can read one.
readme_freshness_report || fail=1

if [ "$fail" -ne 0 ]; then exit 1; fi
echo "readme-numbers: every count in README.md is derived or dated; the MSR scorecard is within $README_FRESHNESS_MAX_DAYS days and cites its run; no fossil phrase."
