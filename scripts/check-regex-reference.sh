#!/usr/bin/env bash
# REGEX REFERENCE GATE (#2129).
#
# C-032 fuzzes the regex engine native-vs-wasm, but its oracle is our OWN native
# engine: it proves the two legs agree and is blind to a rule both read the same
# way and both read wrong. `regex` is the module this repo's own gate authors
# stopped using for exactly that reason — a silent mis-read looks like a correct
# answer, which is the failure shape that destroys modification survival.
#
# This gate asks both legs the questions an engine OUTSIDE this project has
# already answered (proofs/regex/reference-answers.tsv), over a subset chosen so
# the reference engines agree with each other — see proofs/regex/README.md for
# what is in it, what is deliberately out, and why the table is committed rather
# than linked.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
TABLE="proofs/regex/reference-answers.tsv"
PROBE="proofs/regex/probe.almd"
[ -x "$BIN" ] || { echo "::error::regex-reference: no compiler at $BIN (build it first)"; exit 1; }
[ -s "$TABLE" ] || { echo "::error::regex-reference: $TABLE is missing or empty"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cut -f1-3 "$TABLE" > "$WORK/cases.tsv"
rows=$(wc -l < "$WORK/cases.tsv" | tr -d ' ')
# A table that shrank is a gate that stopped looking: the subset is grow-only.
MIN_ROWS=6000
if [ "$rows" -lt "$MIN_ROWS" ]; then
  echo "::error::regex-reference: the table has $rows rows (floor $MIN_ROWS) — the subset may only grow"
  exit 1
fi

fail=0
for leg in native wasm; do
  if [ "$leg" = native ]; then
    "$BIN" run "$PROBE" < "$WORK/cases.tsv" > "$WORK/$leg.out" 2>"$WORK/$leg.err"
  else
    "$BIN" run "$PROBE" --target wasm < "$WORK/cases.tsv" > "$WORK/$leg.out" 2>"$WORK/$leg.err"
  fi
  if [ $? -ne 0 ]; then
    echo "::error::regex-reference: the $leg leg could not answer the probe"
    head -5 "$WORK/$leg.err"
    exit 1
  fi
  paste -d'\t' "$TABLE" "$WORK/$leg.out" | awk -F'\t' '$4 != $5' > "$WORK/$leg.div"
  n=$(wc -l < "$WORK/$leg.div" | tr -d ' ')
  if [ "$n" -ne 0 ]; then
    echo "::error::regex-reference: the $leg leg differs from the reference on $n of $rows cases"
    echo "  op / pattern / subject / reference / $leg"
    head -10 "$WORK/$leg.div" | sed 's/^/  /'
    fail=1
  fi
done

[ "$fail" -eq 0 ] || exit 1
echo "regex-reference: $rows cases, both legs answer what the reference engine answers"
