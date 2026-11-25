#!/usr/bin/env bash
# NATIVE WALLS, corpus-wide (#4006, decision row witness.native-verified-fallback).
#
# Every program in the corpus (each .almd with a `main` under spec/,
# research/benchmark/perf/ and research/benchmark/exercises/) goes through
# `almide verify <f> --mir`, which takes the same frontend and the same v1
# native render `almide build` takes (render_v1_native_certified) and has the
# independent almide-verify judge the certificates of the MIR that render
# came from. Each program lands in one class:
#
#   ok      the v1 render covers it and almide-verify accepts every witness
#           (CERTIFIED, or INCOMPLETE: nothing contradicted)
#   wall    the v1 render refuses it (`the native route walls it: <reason>`)
#   reject  the v1 render covers it but almide-verify REJECTS a witness
#   front   the frontend refused it (deliberate error fixtures); not judged
#   hang    no verdict within the per-program limit: always a failure
#
# proofs/native-walls.txt is the expected set of `wall` and `reject` rows. It
# is SHRINK-ONLY: a program that walls or is rejected without a row fails this
# gate (a new wall is a regression, never a new row), and a row whose program
# now renders and is accepted fails it too, so the row is deleted in the same
# change that fixed it. The rows only ever go down, to zero.
#
# Usage: scripts/check-native-walls.sh [--write]   (from anywhere)
#   ALMIDE_BIN  the almide binary (default target/release/almide); the
#               almide-verify binary must sit next to it or on PATH.
#   JOBS        parallel programs (default: nproc)
#   --write     print the measured rows in the file's format instead of
#               judging (for a SHRINK: the gate still refuses a new row).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

BIN="${ALMIDE_BIN:-target/release/almide}"
case "$BIN" in /*) ;; *) BIN="$PWD/$BIN" ;; esac
EXPECTED=proofs/native-walls.txt
LIMIT="${NATIVE_WALLS_LIMIT:-600}"
JOBS="${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}"
WRITE=0; [ "${1:-}" = "--write" ] && WRITE=1

"$BIN" --version >/dev/null || { echo "::error::native-walls: $BIN does not run"; exit 2; }
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

# The verifier must be reachable: a missing one exits 127 for every program,
# which must never read as "nothing rejected".
probe="$WORK/probe.almd"; printf 'fn main() -> Unit = println("ok")\n' > "$probe"
"$BIN" verify "$probe" --mir > "$WORK/probe.out" 2>&1
case $? in
  0|3) ;;
  *) echo "::error::native-walls: \`almide verify --mir\` cannot judge a one-line program (is almide-verify next to $BIN?)"; sed 's/^/    /' "$WORK/probe.out"; exit 2 ;;
esac

find spec research/benchmark/perf research/benchmark/exercises -name '*.almd' -type f | sort > "$WORK/all.txt"
: > "$WORK/main.txt"
while IFS= read -r f; do
  grep -qE '^(effect )?fn main\b' "$f" && printf '%s\n' "$f" >> "$WORK/main.txt"
done < "$WORK/all.txt"

judge_one() {
  f="$1"; b="$(mktemp "$WORK/bundle.XXXXXX")"; : > "$b"
  err=$(timeout "$LIMIT" "$BIN" verify "$f" --mir --emit "$b" 2>&1 >/dev/null); rc=$?
  if [ $rc -eq 124 ]; then printf 'hang\t%s\t%ss\n' "$f" "$LIMIT"
  elif [ $rc -eq 0 ] || [ $rc -eq 3 ]; then printf 'ok\t%s\t\n' "$f"
  elif grep -q 'the native route walls it' <<<"$err"; then
    printf 'wall\t%s\t%s\n' "$f" "$(grep -m1 'the native route walls it' <<<"$err" | sed 's/.*the native route walls it: //')"
  elif [ -s "$b" ] && grep -q '^witness ' "$b"; then printf 'reject\t%s\t\n' "$f"
  else printf 'front\t%s\t\n' "$f"
  fi
  rm -f "$b"
}
export -f judge_one; export BIN WORK LIMIT
xargs -P "$JOBS" -I{} bash -c 'judge_one "$@"' _ {} < "$WORK/main.txt" | sort -t "$(printf '\t')" -k2,2 > "$WORK/rows.tsv"

total=$(wc -l < "$WORK/main.txt" | tr -d ' ')
count() { awk -F'\t' -v c="$1" '$1 == c' "$WORK/rows.tsv" | wc -l | tr -d ' '; }
echo "native-walls: $total programs — ok $(count ok), wall $(count wall), reject $(count reject), front $(count front), hang $(count hang)"

if [ "$WRITE" -eq 1 ]; then
  awk -F'\t' '$1 == "wall" || $1 == "reject" { print $1 "\t" $2 }' "$WORK/rows.tsv"
  exit 0
fi

fail=0
if [ "$(count hang)" -ne 0 ]; then
  awk -F'\t' '$1 == "hang" { print "FAIL hang (no verdict in " $3 "): " $2 }' "$WORK/rows.tsv"; fail=1
fi
grep -vE '^(#|$)' "$EXPECTED" | cut -f1,2 | sort -t "$(printf '\t')" -k2,2 > "$WORK/expected.tsv"
awk -F'\t' '$1 == "wall" || $1 == "reject" { print $1 "\t" $2 }' "$WORK/rows.tsv" > "$WORK/measured.tsv"
# New: measured but not expected — a regression, never a new row.
while IFS=$'\t' read -r cls f; do
  reason="$(awk -F'\t' -v p="$f" '$2 == p { print $3 }' "$WORK/rows.tsv")"
  echo "FAIL new $cls: $f${reason:+ — $reason}"; fail=1
done < <(comm -13 "$WORK/expected.tsv" "$WORK/measured.tsv")
# Gone: expected but no longer measured — shrink the file in this change.
while IFS=$'\t' read -r cls f; do
  now="$(awk -F'\t' -v p="$f" '$2 == p { print $1 }' "$WORK/rows.tsv")"
  echo "FAIL stale row: $cls $f is now '${now:-absent}' — delete its row from $EXPECTED"; fail=1
done < <(comm -23 "$WORK/expected.tsv" "$WORK/measured.tsv")

if [ "$fail" -ne 0 ]; then echo "native-walls: FAILED"; exit 1; fi
echo "native-walls OK: $(count wall) wall(s) and $(count reject) rejected certificate(s), exactly the rows of $EXPECTED"
