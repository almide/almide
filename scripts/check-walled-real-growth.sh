#!/usr/bin/env bash
# Refuse growth of the walled-real ledger unless each new row cites an OPEN issue (#3058).
#
# proofs/walled-real-baseline.txt is compared by proofs/corpus-wall.sh against
# the classifier as an EXACT set, so a PR that walls a new function must add its
# row — and on 2026-09-29 that is how the ledger went 71 -> 166 in one day: every
# row was "recorded", none was owned. This gate is the owner check. A row whose
# `file :: fn` key is absent at BASE must end in a citation
#
#     spec/wasm_cross/x.almd :: f  # #1234
#
# naming an issue that is OPEN now (the work that will burn the row). A row that
# only moves, or that was already there, needs nothing. Rows that go away are
# the point and are never judged.
#
# The same owner check holds the shape-matrix known-open list (#3309), whose
# rows are `<cell>  <columns>  # #NNNN`: run it with
# LEDGER=proofs/shape-matrix-baseline.txt. The key is everything before the
# citation, so a row whose columns change is a new row and needs an open owner.
#
# usage: [LEDGER=<ledger>] check-walled-real-growth.sh <base-ref> [head-ref]
#        check-walled-real-growth.sh --self-test
# The open-state lookup uses `gh`; set WALLED_REAL_OFFLINE=1 to check the
# citation syntax only (the self-test does). A lookup that fails is a refusal:
# an unverifiable citation is not an owner.
set -euo pipefail

LEDGER="${LEDGER:-proofs/walled-real-baseline.txt}"

# `file :: fn` keys of a ledger text on stdin (comments, blanks and the
# citation stripped — the same normalisation corpus-wall.sh applies).
keys() {
  grep -v '^#' | grep -v '^[[:space:]]*$' \
    | sed -E 's/[[:space:]]+#[[:space:]]*#[0-9]+[[:space:]]*$//' | LC_ALL=C sort -u || true
}

issue_open() {
  [ "${WALLED_REAL_OFFLINE:-0}" = 1 ] && return 0
  local state
  state="$(gh issue view "$1" --json state -q .state 2>/dev/null)" || return 1
  [ "$state" = OPEN ]
}

# judge <base ledger file> <head ledger file>; prints refusals, returns 1 on any.
judge() {
  local base="$1" head="$2" bad=0 key line n
  local added
  added="$(LC_ALL=C comm -13 <(keys < "$base") <(keys < "$head"))"
  [ -z "$added" ] && { echo "$LEDGER growth: no new rows"; return 0; }
  while IFS= read -r key; do
    [ -z "$key" ] && continue
    line="$(grep -F -- "$key" "$head" | grep -v '^#' | head -1)"
    n="$(printf '%s' "$line" | sed -nE 's/.*[[:space:]]#[[:space:]]*#([0-9]+)[[:space:]]*$/\1/p')"
    if [ -z "$n" ]; then
      echo "REFUSED (no issue cited): $key" >&2; bad=1
    elif ! issue_open "$n"; then
      echo "REFUSED (#$n is not an open issue): $key" >&2; bad=1
    else
      echo "new row owned by #$n: $key"
    fi
  done <<< "$added"
  if [ "$bad" = 1 ]; then
    echo "$LEDGER growth FAIL: a new row must end in '  # #NNNN' naming the open issue that will burn it." >&2
    return 1
  fi
}

self_test() {
  local d; d="$(mktemp -d)"; trap 'rm -rf "$d"' RETURN
  printf '# header\nspec/a.almd :: f\n' > "$d/base"
  printf '# header\nspec/a.almd :: f\n' > "$d/same"
  printf 'spec/a.almd :: f\nspec/b.almd :: g\n' > "$d/uncited"
  printf 'spec/a.almd :: f\nspec/b.almd :: g  # #3058\n' > "$d/cited"
  printf 'spec/a.almd :: f  # #12\n' > "$d/recited"
  printf '' > "$d/shrunk"
  export WALLED_REAL_OFFLINE=1
  judge "$d/base" "$d/same" >/dev/null || { echo "self-test: unchanged ledger refused" >&2; return 1; }
  judge "$d/base" "$d/cited" >/dev/null || { echo "self-test: cited row refused" >&2; return 1; }
  judge "$d/base" "$d/recited" >/dev/null || { echo "self-test: a citation on an old row counted as growth" >&2; return 1; }
  judge "$d/base" "$d/shrunk" >/dev/null || { echo "self-test: a shrink refused" >&2; return 1; }
  if judge "$d/base" "$d/uncited" >/dev/null 2>&1; then
    echo "self-test: an uncited new row passed" >&2; return 1
  fi
  echo "walled-real growth self-test: OK (5 cases)"
}

if [ "${1:-}" = --self-test ]; then self_test; exit $?; fi
BASE="${1:?usage: check-walled-real-growth.sh <base-ref> [head-ref]}"
HEAD_REF="${2:-HEAD}"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
git show "$BASE:$LEDGER" > "$tmp/base" 2>/dev/null || : > "$tmp/base"
git show "$HEAD_REF:$LEDGER" > "$tmp/head"
judge "$tmp/base" "$tmp/head"
