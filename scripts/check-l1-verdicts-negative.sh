#!/usr/bin/env bash
# Negative controls for the L1 verdict ledger gate (Survey 4 law 4): prove
# check-l1-verdicts.sh FIRES on each schema violation it claims to catch.
# The real ledger is the positive control.
set -euo pipefail
cd "$(dirname "$0")/.."

GATE="bash scripts/check-l1-verdicts.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

expect_pass() {
  if ! $GATE "$1" >/dev/null 2>&1; then
    echo "FAIL: $2" >&2
    exit 1
  fi
}
expect_fail() {
  if $GATE "$1" >/dev/null 2>&1; then
    echo "FAIL: $2" >&2
    exit 1
  fi
}

expect_pass proofs/l1-verdicts.toml \
  "the committed ledger did not pass — harness broken, negatives meaningless"

# Missing field: strip msr_measured from every block.
grep -v '^msr_measured' proofs/l1-verdicts.toml >"$tmp/missing-field.toml"
expect_fail "$tmp/missing-field.toml" \
  "gate passed a ledger with msr_measured stripped — blind to missing fields"

# Open-vocabulary verdict (the ledger aligns keys with padding — match it).
sed -E 's/^verdict +=.*/verdict = "maybe"/' proofs/l1-verdicts.toml >"$tmp/bad-vocab.toml"
expect_fail "$tmp/bad-vocab.toml" \
  "gate passed verdict=\"maybe\" — the closed vocabulary is not enforced"

# Non-ascending ids: every id becomes LV-001.
sed -E 's/^id +=.*/id = "LV-001"/' proofs/l1-verdicts.toml >"$tmp/dup-ids.toml"
expect_fail "$tmp/dup-ids.toml" \
  "gate passed duplicated ids — ascending-id rule is not enforced"

# Empty ledger (zero blocks) must fail, not vacuously pass.
: >"$tmp/empty.toml"
expect_fail "$tmp/empty.toml" \
  "gate passed an empty ledger — the find-nothing-exit-0 shape"

echo "l1-verdicts negative controls: 1 positive + 4 negatives all behaved"
