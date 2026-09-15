#!/usr/bin/env bash
# OWNERSHIP CERTIFIER CORPUS GATE (#2231).
#
# `ALMIDE_CERTIFY_OWNERSHIP=report` makes the native pipeline re-derive what
# every variable occurrence does from the FINAL IR and check it against the
# ownership passes' verdicts (crates/almide-codegen/src/certify_ownership.rs):
# a borrowed param that is consumed (C1), an owned value cloned at its last
# use (C3), an owned param no occurrence ever needs owned (C4). rustc sees the
# first; the other two build, print the right lines on both legs, and only
# allocate more than they should — the class no other gate holds.
#
# The corpus (every spec/lang and spec/wasm_cross program, or `$CORPUS`) is
# compiled under `report` and every violation line is collected. The
# committed ledger proofs/ownership-certifier-baseline.txt holds the sorted
# set at the last anchoring: a line NOT in the ledger is a new defect (red,
# named); a ledger line that no longer appears is a fix that must re-anchor
# in the same change (`--update` rewrites the ledger). The set may only
# SHRINK: the goal state is an empty ledger and `fail` as the debug default.
# A panic under `report` is red in its own right.
#
# Negative controls: scripts/check-ownership-certifier-negative.sh.
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE:-${ALMIDE_BIN:-almide}}"
LEDGER="${LEDGER:-proofs/ownership-certifier-baseline.txt}"
JOBS="${JOBS:-8}"
update=0
[ "${1:-}" = "--update" ] && update=1

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

if [ -n "${CORPUS:-}" ]; then
  cp "$CORPUS" "$tmp/corpus"
  floor=1
else
  find spec/lang spec/wasm_cross -name '*.almd' | sort > "$tmp/corpus"
  floor=100
fi
count=$(wc -l < "$tmp/corpus" | tr -d ' ')
[ "$count" -ge "$floor" ] || { echo "::error::ownership-certifier: only $count corpus file(s) found — the corpus moved and the gate went blind"; exit 1; }

mkdir -p "$tmp/err"
export ALMIDE tmp
# shellcheck disable=SC2016
xargs -P "$JOBS" -I{} bash -c '
  f="$1"; key="$(printf "%s" "$f" | tr "/" "_")"
  ALMIDE_CERTIFY_OWNERSHIP=report "$ALMIDE" "$f" --target rust > /dev/null 2> "$tmp/err/$key" < /dev/null
' _ {} < "$tmp/corpus"

panics=$(grep -l "panicked" "$tmp/err"/* 2>/dev/null | wc -l | tr -d ' ')
if [ "$panics" != 0 ]; then
  echo "::error::ownership-certifier: $panics file(s) panicked under report mode:"
  grep -l "panicked" "$tmp/err"/* | head -5 | while read -r e; do echo "    $(basename "$e"): $(grep -m1 panicked "$e")"; done
  exit 1
fi

cat "$tmp/err"/* | grep '^\[CERTIFY OWNERSHIP\] ' | sed 's/^\[CERTIFY OWNERSHIP\] //' | sort -u > "$tmp/now"
n_now=$(wc -l < "$tmp/now" | tr -d ' ')

if [ "$update" = 1 ]; then
  {
    echo "# ownership certifier ledger (#2231): every violation ALMIDE_CERTIFY_OWNERSHIP=report raises over"
    echo "# spec/lang + spec/wasm_cross at the last anchoring, sorted. Shrink-only: a new line is a defect,"
    echo "# a vanished line re-anchors with \`scripts/check-ownership-certifier.sh --update\` in the same change."
    cat "$tmp/now"
  } > "$LEDGER"
  echo "ownership-certifier: ledger rewritten with $n_now line(s) over $count file(s)"
  exit 0
fi

[ -f "$LEDGER" ] || { echo "::error::ownership-certifier: $LEDGER missing — generate with --update"; exit 1; }
grep -v '^#' "$LEDGER" | sed '/^$/d' | sort -u > "$tmp/was"
comm -13 "$tmp/was" "$tmp/now" > "$tmp/new"
comm -23 "$tmp/was" "$tmp/now" > "$tmp/gone"
fail=0
if [ -s "$tmp/new" ]; then
  echo "::error::ownership-certifier: $(wc -l < "$tmp/new" | tr -d ' ') NEW violation(s) not in $LEDGER — an ownership verdict regressed:"
  head -12 "$tmp/new" | sed 's/^/    /'
  fail=1
fi
if [ -s "$tmp/gone" ]; then
  echo "::error::ownership-certifier: $(wc -l < "$tmp/gone" | tr -d ' ') ledger line(s) no longer raised — a fix landed; re-anchor with --update in the same change:"
  head -12 "$tmp/gone" | sed 's/^/    /'
  fail=1
fi
if [ "$fail" = 0 ]; then
  by=$(sed 's/^\[\(C[0-9]\)[^]]*\].*/\1/' "$tmp/now" | sort | uniq -c | awk '{printf "%s=%s ", $2, $1}')
  echo "ownership-certifier OK: $n_now violation(s) over $count file(s), exactly the ledger (${by:-none})"
fi
exit $fail
