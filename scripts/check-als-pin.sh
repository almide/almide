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
# Advancing the pin is its own change, after the als PR carrying the new id
# has merged; the pin never moves in the PR that mints the id.
#
#   bash scripts/check-als-pin.sh            # gate (CI `checks` job)
#   ALS_LEDGER=<path> bash scripts/check-als-pin.sh   # offline: judge ledger from a file
#
# Exit 0 = every id is judge-known; 1 = a violation; 2 = environment (the
# judge ledger could not be fetched — CI treats that as red too, since a
# gate that cannot look is not a gate).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2

PIN_FILE="proofs/als-pin.txt"
LEDGER="docs/contracts/contracts.toml"

pin="$(grep -vE '^\s*(#|$)' "$PIN_FILE" | head -1 | tr -d '[:space:]')"
if ! [[ "$pin" =~ ^[0-9a-f]{40}$ ]]; then
  echo "::error::$PIN_FILE does not name a full 40-hex als commit (got '$pin')"
  exit 2
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
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
echo "als-pin OK: $n_ours contract id(s) all present in the judge ledger at ${pin:0:7} (judge holds $n_judge)"
