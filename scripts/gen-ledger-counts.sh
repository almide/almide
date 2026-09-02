#!/usr/bin/env bash
# gen-ledger-counts.sh — re-measure the aggregate counts, restamp
# proofs/ledger-counts.toml with today's date, and regenerate the four docs
# that render them (README.md claims + stats, proofs/STAGE-STATUS.md,
# docs/contracts/README.md, docs/contracts/conformance.md).
#
#   bash scripts/gen-ledger-counts.sh
#
# WHEN: a release step before `release-seal.sh gen` (the seal freezes the
# evidence, so the totals it sits beside should be current), or whenever the
# nightly scripts/check-ledger-counts.sh reports drift. NOT in a fixture or
# contract PR: deriving the totals per PR is what made every pair of them
# conflict at the merge queue (the whole reason the record exists — see
# scripts/lib/ledger-counts.sh). Commit the ledger and the four docs together.
set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2
. scripts/lib/ledger-counts.sh

before="$(mktemp)"; trap 'rm -f "$before"' EXIT
[ -f "$COUNTS_LEDGER" ] && cp "$COUNTS_LEDGER" "$before" || : > "$before"

counts_stamp
bash scripts/gen-claims.sh
bash scripts/gen-readme-stats.sh
bash docs/contracts/generate-readme.sh > docs/contracts/README.md
bash docs/contracts/generate-conformance.sh > docs/contracts/conformance.md

echo "ledger-counts: stamped $(counts_date) in $COUNTS_LEDGER; the four docs re-rendered."
if diff <(grep -vE '^(#|date|$)' "$before") <(grep -vE '^(#|date|$)' "$COUNTS_LEDGER") >/dev/null; then
  echo "ledger-counts: no count moved (only the stamp date)."
else
  echo "ledger-counts: moved —"
  diff <(grep -vE '^(#|date|$)' "$before") <(grep -vE '^(#|date|$)' "$COUNTS_LEDGER") | grep '^[<>]' || true
fi
