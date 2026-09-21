#!/usr/bin/env bash
# ALS PIN GATE — the two-PR order, made mechanical (almide/als #60, second half).
#
# almide/als (the judge) holds the normative contract ledger; CONTRIBUTING
# there promises that a contract enters the judge BEFORE the implementation
# claims it. Between 2026-08-25 and 2026-09-06 that order lapsed 26 times
# (C-322..C-347 were minted here first) and the judge's verdict went stale.
# This gate is the reverse of the judge's `check-contracts.sh --impl-root`:
#
#   every `id = "C-NNN"` in docs/contracts/contracts.toml must exist in the
#   judge's docs/contracts/contracts.toml AT THE PINNED COMMIT
#   (proofs/als-pin.txt) — an id the judge has not seen is refused.
#
# #2403 adds the second half of the same promise. An id is not a contract; the
# `statement` is. For a year the gate compared ids only, so seven contracts
# (C-004 / C-053 / C-076 / C-197 / C-215 / C-237 / C-273, 14,332 bytes) grew
# normative text here that the judge never received, and every gate in both
# repos stayed green. Now, for every shared id, the `statement` and `title`
# values must be BYTE-IDENTICAL to the judge's at the pin. A drift is refused
# and named, with the first differing window of each side, and with the
# direction when it is a pure extension (one side's text is a prefix of the
# other's):
#
#   almide ahead — this tree has text the judge lacks: land it in almide/als
#                  first (the judge is normative), then advance the pin.
#   als ahead    — the judge has text this tree lacks: the normal in-flight
#                  state between an als merge and the implementation PR. It is
#                  tolerated ONLY when the id is listed in proofs/als-ahead.txt
#                  (one id per line, with the als PR that carries the text).
#                  That list is shrink-only: an entry whose id no longer drifts
#                  is stale and fails the gate, so the list cannot accumulate.
#
# Advancing the pin is its own change, after the als PR carrying the new id
# (or the new wording) has merged; the pin never moves in the PR that mints it.
#
#   bash scripts/check-als-pin.sh            # gate (CI `checks` job)
#   ALS_LEDGER=<path> bash scripts/check-als-pin.sh   # offline: judge ledger from a file
#   ALS_AHEAD=<path>  ...                     # the allowance list from a file (negative controls)
#
# Exit 0 = every id is judge-known and every shared statement/title is
# byte-identical (or listed as als-ahead); 1 = a violation; 2 = environment
# (the judge ledger could not be fetched — CI treats that as red too, since a
# gate that cannot look is not a gate).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2

PIN_FILE="proofs/als-pin.txt"
LEDGER="${OUR_LEDGER:-docs/contracts/contracts.toml}"
AHEAD_FILE="${ALS_AHEAD:-proofs/als-ahead.txt}"

pin="$(grep -vE '^\s*(#|$)' "$PIN_FILE" | head -1 | tr -d '[:space:]')"
if ! [[ "$pin" =~ ^[0-9a-f]{40}$ ]]; then
  echo "::error::$PIN_FILE does not name a full 40-hex als commit (got '$pin')"
  exit 2
fi

tmp="$(mktemp)"
work="$(mktemp -d)"
trap 'rm -rf "$tmp" "$work"' EXIT
if [ -n "${ALS_LEDGER:-}" ]; then
  cp "$ALS_LEDGER" "$tmp" || exit 2
else
  url="https://raw.githubusercontent.com/almide/als/$pin/docs/contracts/contracts.toml"
  if ! curl -fsSL --retry 3 --retry-delay 2 -o "$tmp" "$url"; then
    echo "::error::could not fetch the judge ledger at the pin ($url)"
    exit 2
  fi
fi

ids() { grep -oE '^\s*id\s*=\s*"C-[0-9]+"' "$1" | grep -oE 'C-[0-9]+' | sort -u; }
ours="$(ids "$LEDGER")"
judge="$(ids "$tmp")"
missing="$(comm -23 <(echo "$ours") <(echo "$judge"))"
n_ours="$(echo "$ours" | grep -c .)"
n_judge="$(echo "$judge" | grep -c .)"

if [ -n "$missing" ]; then
  for id in $missing; do
    echo "::error::$id is in $LEDGER but not in the judge ledger at pin ${pin:0:7} — land it in almide/als first, then advance $PIN_FILE"
  done
  echo "als-pin FAILED: $(echo "$missing" | grep -c .) id(s) unknown to the judge (ours $n_ours, judge $n_judge at ${pin:0:7})"
  exit 1
fi

# ---------------------------------------------------------------------------
# Statement / title parity (#2403). The ledger schema is flat TOML, one scalar
# per line (see its header), so `<id>|<key>\t<raw value>` is the whole record:
# the value is the line's text after `key =`, untouched — a changed quote, a
# trailing space, an escaped character all count, because the judge's copy is
# what is normative and "equal enough" is how the seven drifted.
# ---------------------------------------------------------------------------
fields() { # ledger -> "<id>|<key>\t<value>" sorted, one line per (id, key)
  awk '
    /^[[:space:]]*\[\[contract\]\]/ { id = "" ; next }
    /^[[:space:]]*id[[:space:]]*=[[:space:]]*"C-[0-9]+"/ { match($0, /C-[0-9]+/); id = substr($0, RSTART, RLENGTH); next }
    /^[[:space:]]*(statement|title)[[:space:]]*=/ {
      if (id == "") next
      key = $0; sub(/^[[:space:]]*/, "", key); sub(/[[:space:]]*=.*$/, "", key)
      val = $0; sub(/^[[:space:]]*(statement|title)[[:space:]]*=[[:space:]]*/, "", val)
      print id "|" key "\t" val
    }' "$1" | sort
}
fields "$LEDGER" >"$work/ours.tsv"
fields "$tmp"    >"$work/judge.tsv"
# Only shared keys are compared; an id the judge lacks was refused above, and an
# id only the judge has is the als-first window before the implementation PR.
join -t "$(printf '\t')" -j1 "$work/ours.tsv" "$work/judge.tsv" \
  | awk -F'\t' '$2 != $3 { print $1 }' >"$work/drift.txt"
n_shared="$(join -t "$(printf '\t')" -j1 "$work/ours.tsv" "$work/judge.tsv" | wc -l | tr -d ' ')"

# The als-ahead allowance: `C-NNN  <als PR / reason>` lines, comments and blanks ignored.
allowed=""
if [ -f "$AHEAD_FILE" ]; then
  allowed="$(grep -vE '^\s*(#|$)' "$AHEAD_FILE" | awk '{print $1}' | sort -u)"
  bad="$(echo "$allowed" | grep -vE '^C-[0-9]+$' || true)"
  if [ -n "$bad" ]; then
    echo "::error::$AHEAD_FILE has an entry that is not a contract id: $(echo "$bad" | tr '\n' ' ')"
    exit 2
  fi
fi

window() { # value byte-offset -> a short quoted window around the offset
  local v="$1" at="$2" from
  from=$(( at > 24 ? at - 24 : 0 ))
  printf '%s' "${v:$from:80}"
}
first_diff() { # a b -> 0-based offset of the first differing byte
  local a="$1" b="$2" i=0 n
  n=${#a}
  [ ${#b} -lt "$n" ] && n=${#b}
  while [ "$i" -lt "$n" ] && [ "${a:$i:1}" = "${b:$i:1}" ]; do i=$((i + 1)); done
  echo "$i"
}
# The prefix test ignores the closing quote of the TOML string, so a value
# that only ADDS sentences reads as an extension rather than a divergence.
extends() { # longer shorter -> 0 iff `longer` is `shorter` plus text at its end
  local l="${1%\"}" s="${2%\"}"
  [ ${#l} -gt ${#s} ] && [ "${l:0:${#s}}" = "$s" ]
}

violations=0
n_ahead_ok=0
seen_ids=""
while IFS= read -r key; do
  [ -z "$key" ] && continue
  id="${key%%|*}"; field="${key#*|}"
  o="$(grep -F "$key	" "$work/ours.tsv"  | head -1 | cut -f2-)"
  j="$(grep -F "$key	" "$work/judge.tsv" | head -1 | cut -f2-)"
  at="$(first_diff "$o" "$j")"
  if extends "$o" "$j"; then
    direction="almide ahead (this tree has ${#o} bytes, the judge ${#j}; the extra text is here)"
    remedy="land the wording in almide/als first (the judge is normative), then advance $PIN_FILE"
  elif extends "$j" "$o"; then
    direction="als ahead (the judge has ${#j} bytes, this tree ${#o}; the extra text is the judge's)"
    remedy="bring the wording here with the implementation PR, or list $id in $AHEAD_FILE while that PR is in flight"
  else
    direction="diverged (neither side is a prefix of the other; ours ${#o} bytes, judge ${#j})"
    remedy="reconcile in almide/als first (the judge is normative), then advance $PIN_FILE"
  fi
  case "$seen_ids" in *"$id "*) ;; *) seen_ids="$seen_ids$id ";; esac
  if echo "$allowed" | grep -qxF "$id"; then
    # The allowance covers the judge being ahead and nothing else: text HERE
    # that the judge lacks is never in flight, whatever the list says.
    if extends "$j" "$o"; then
      echo "als-pin: $id.$field differs at byte $at — $direction — listed in $AHEAD_FILE, tolerated"
      n_ahead_ok=$((n_ahead_ok + 1))
      continue
    fi
    remedy="$remedy (the $AHEAD_FILE entry covers only text the JUDGE is ahead with)"
  fi
  echo "::error::$id.$field differs from the judge at pin ${pin:0:7} — $direction — $remedy"
  echo "  first difference at byte $at"
  echo "  ours : ...$(window "$o" "$at")..."
  echo "  judge: ...$(window "$j" "$at")..."
  violations=$((violations + 1))
done <"$work/drift.txt"

# Shrink-only: an allowance whose id no longer drifts (the implementation PR
# landed, or the pin caught up) must be removed, or the list becomes a blanket.
stale=0
for id in $allowed; do
  if ! grep -qE "^$id\|" "$work/drift.txt"; then
    echo "::error::$AHEAD_FILE lists $id as als-ahead but its statement and title match the judge at ${pin:0:7} — remove the stale entry (the list is shrink-only)"
    stale=$((stale + 1))
  fi
done

if [ "$violations" -gt 0 ] || [ "$stale" -gt 0 ]; then
  echo "als-pin FAILED: $violations drifted statement/title field(s) across $(echo "$seen_ids" | wc -w | tr -d ' ') id(s), $stale stale als-ahead entr(y/ies) (ids ours $n_ours, judge $n_judge at ${pin:0:7}; $n_shared shared fields compared)"
  exit 1
fi
echo "als-pin OK: $n_ours contract id(s) all present in the judge ledger at ${pin:0:7} (judge holds $n_judge); $n_shared shared statement/title field(s) byte-identical, $n_ahead_ok tolerated as als-ahead"
