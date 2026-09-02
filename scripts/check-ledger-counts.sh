#!/usr/bin/env bash
# LEDGER-COUNTS FRESHNESS GATE — the stamped totals vs the tree.
#
# The aggregate counts the generated docs quote are RECORDED in
# proofs/ledger-counts.toml (dated) and rendered from there, never re-derived
# per PR: deriving them per PR made every fixture or contract PR rewrite the
# same "N fixtures / N contracts" lines, so any two conflicted at the merge
# queue. The PR gates (check-contracts.sh, gen-claims.sh --check,
# gen-readme-stats.sh --check) therefore compare STRUCTURE exactly and only
# require the counts block to be present, well-formed and equal to the record.
# This gate is the other half: it re-measures every count and fails when the
# record has drifted from the tree. It runs NIGHTLY (.github/workflows/
# ledger-counts.yml) and as a release step — red means "refresh", the same
# ethos as a fuzz-night finding, never a reason for a fixture PR to touch the
# totals. Refresh: bash scripts/gen-ledger-counts.sh (commit the ledger and the
# four docs together).
#
# It FAILS when:
#   (a) the ledger is missing, or its date is not YYYY-MM-DD, or a count the
#       measurement produces has no recorded row;
#   (b) a recorded count differs from a fresh measurement (each drifted key is
#       named with both values);
#   (c) a doc's counts block is missing, or differs from the record's rendering
#       (a hand-edited number, or a doc not regenerated after a restamp).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2
. scripts/lib/ledger-counts.sh

fail=0
err() { fail=1; echo "::error::$*"; }

# ── (a) the record is well-formed ────────────────────────────────────────────
if [ ! -f "$COUNTS_LEDGER" ]; then
  err "$COUNTS_LEDGER is missing — run: bash scripts/gen-ledger-counts.sh"
  exit 1
fi
date_rec="$(grep -E '^date[[:space:]]*=' "$COUNTS_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*"?//; s/"?[[:space:]]*$//')"
if ! printf '%s' "$date_rec" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'; then
  err "$COUNTS_LEDGER: date is '$date_rec', expected YYYY-MM-DD — run: bash scripts/gen-ledger-counts.sh"
fi

# ── (b) every count, recorded vs measured ────────────────────────────────────
measured="$(counts_measure)"
n_keys=0; n_drift=0
while IFS= read -r line; do
  key="${line%%[[:space:]]*}"; key="${key%%=*}"
  new="${line#*= }"
  n_keys=$((n_keys + 1))
  old="$(grep -E "^$key[[:space:]]*=" "$COUNTS_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//')"
  if [ -z "$old" ]; then
    err "$COUNTS_LEDGER has no row for '$key' (measured $new) — run: bash scripts/gen-ledger-counts.sh"
  elif [ "$old" != "$new" ]; then
    n_drift=$((n_drift + 1))
    echo "  $key: recorded $old, measured $new"
  fi
done <<< "$measured"
if [ "$n_drift" -ne 0 ]; then
  err "$n_drift of $n_keys stamped count(s) drifted from the tree (stamped $date_rec; see above) — refresh: bash scripts/gen-ledger-counts.sh"
fi

# ── (c) each doc's block is the record, rendered ─────────────────────────────
# FILE, block index in FILE, renderer.
check_block() {
  local file="$1" nth="$2" render="$3" want got
  want="$($render)"
  got="$(counts_extract "$file" "$nth")"
  if [ -z "$got" ]; then
    err "$file: counts block #$nth is missing — regenerate it (bash scripts/gen-ledger-counts.sh)"
  elif [ "$want" != "$got" ]; then
    err "$file: counts block #$nth differs from $COUNTS_LEDGER (a hand edit, or a doc not regenerated after a restamp) — run: bash scripts/gen-ledger-counts.sh"
    diff <(printf '%s\n' "$got") <(printf '%s\n' "$want") | head -12 || true
  fi
}
check_block README.md 1 counts_render_claims
check_block README.md 2 counts_render_stats
check_block proofs/STAGE-STATUS.md 1 counts_render_stages
check_block docs/contracts/README.md 1 counts_render_index
check_block docs/contracts/conformance.md 1 counts_render_conformance

if [ "$fail" -ne 0 ]; then
  echo "::error::ledger-counts gate FAILED — the stamped totals need a refresh (bash scripts/gen-ledger-counts.sh), not a fixture PR."
  exit 1
fi
echo "ledger-counts: OK — $n_keys count(s) stamped $date_rec match a fresh measurement; every doc block is the record rendered."

# ── NEGATIVE CONTROLS (each flips green->red on a one-line edit) ─────────────
#   (1) add a spec/wasm_cross fixture without restamping   -> (b) wasm_cross_fixtures
#       (and conformance_fixtures once a contract cites it).
#   (2) hand-edit a number inside a doc's counts block     -> (c) names the doc.
#   (3) corrupt the ledger's date                          -> (a) date shape.
#   (4) delete a ledger row                                -> (a) missing row.
# Recorded in proofs/gate-verification.toml at the landing.
