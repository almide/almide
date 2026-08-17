#!/usr/bin/env bash
# Negative controls for the retired-spelling gate: prove
# check-retired-spellings.sh FIRES on each way a retirement can die.
# The committed ledger is the positive control.
set -euo pipefail
cd "$(dirname "$0")/.."

# Inherits ALMIDE_BIN so CI and a local run resolve the same compiler.
GATE="bash scripts/check-retired-spellings.sh"
LEDGER=proofs/retired-spellings.toml

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

expect_pass() { $GATE "$1" >/dev/null 2>&1 || { echo "FAIL: $2" >&2; exit 1; }; }
expect_fail() { $GATE "$1" >/dev/null 2>&1 && { echo "FAIL: $2" >&2; exit 1; }; return 0; }

expect_pass "$LEDGER" \
  "the committed ledger did not pass — harness broken, the negatives below would be meaningless"

# An empty ledger must fail, not vacuously pass (#976 find-nothing-exit-0).
: >"$tmp/empty.toml"
expect_fail "$tmp/empty.toml" \
  "gate passed an empty ledger — a ledger that lists nothing reads as green forever"

# A row naming a spelling that is NOT retired (it is a live function) must
# fail: this is the shape a retirement takes when it silently comes back.
cat "$LEDGER" >"$tmp/live.toml"
cat >>"$tmp/live.toml" <<'EOF'

[[retired]]
spelling = "list.map"
epoch = 2
code = "E043"
replacement = "this row is a forgery — list.map is alive"
kind = "spelling"
EOF
expect_fail "$tmp/live.toml" \
  "gate passed a row claiming a LIVE function is retired — it is not executing the rows"

# A row whose declared code is not the one the compiler emits must fail: the
# ledger and the diagnostic have to stay joined.
sed 's/code = "E043"/code = "E999"/' "$LEDGER" >"$tmp/wrongcode.toml"
expect_fail "$tmp/wrongcode.toml" \
  "gate passed rows declaring a diagnostic code the compiler never emits"

# A carrier reclassified as a plain spelling must fail — the carrier IS
# defined in stdlib on purpose, and the kinds must not blur.
sed 's/kind = "carrier"/kind = "spelling"/' "$LEDGER" >"$tmp/blurred.toml"
expect_fail "$tmp/blurred.toml" \
  "gate passed a carrier reclassified as a spelling — the two kinds must stay distinct"

echo "retired-spellings negative controls: 1 positive + 4 negatives all behaved"
