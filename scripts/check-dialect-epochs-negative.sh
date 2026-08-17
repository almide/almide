#!/usr/bin/env bash
# Negative controls for the dialect-epoch gate: prove check-dialect-epochs.sh
# FIRES on each violation it claims to catch. The committed ledger and the
# real constant are the positive control.
set -euo pipefail
cd "$(dirname "$0")/.."

GATE="bash scripts/check-dialect-epochs.sh"
LEDGER=proofs/dialect-epochs.toml
CONST=crates/almide-types/src/dialect.rs

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

expect_pass() { $GATE "$1" "$2" >/dev/null 2>&1 || { echo "FAIL: $3" >&2; exit 1; }; }
expect_fail() { $GATE "$1" "$2" >/dev/null 2>&1 && { echo "FAIL: $3" >&2; exit 1; }; return 0; }

expect_pass "$LEDGER" "$CONST" \
  "the committed ledger and constant did not pass — harness broken, negatives below would be meaningless"

# An empty ledger must fail, not vacuously pass (#976 find-nothing-exit-0).
: >"$tmp/empty.toml"
expect_fail "$tmp/empty.toml" "$CONST" \
  "gate passed an empty ledger — a vacuous ledger reads as green forever"

# A gap in the epoch sequence: a stamp could name an epoch that never existed.
sed 's/^n = 3$/n = 4/' "$LEDGER" >"$tmp/gap.toml"
expect_fail "$tmp/gap.toml" "$CONST" \
  "gate passed a gapped epoch sequence (1,2,4) — a stamp could name a skipped epoch"

# An epoch above 1 that breaks nothing is a release, not an epoch.
python3 - "$LEDGER" "$tmp/nobreak.toml" <<'PY'
import re, sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src).read()
# Empty the LAST breaks list.
head, sep, tail = text.rpartition("breaks = [")
tail = "]\n"
open(dst, "w").write(head + sep + "\n" + tail)
PY
expect_fail "$tmp/nobreak.toml" "$CONST" \
  "gate passed an epoch with an empty breaks list — an epoch with no break is a release"

# The constant disagreeing with the ledger is the drift this gate exists for.
sed 's/CURRENT_DIALECT: u32 = 3/CURRENT_DIALECT: u32 = 7/' "$CONST" >"$tmp/const_drift.rs"
expect_fail "$LEDGER" "$tmp/const_drift.rs" \
  "gate passed a constant that disagrees with the ledger — the drift it exists to catch"

# Blind-gate guard: if the constant cannot be read the gate must go loud, not green.
sed 's/CURRENT_DIALECT: u32 = 3/CURRENT_DIALECT_RENAMED: u32 = 3/' "$CONST" >"$tmp/const_moved.rs"
expect_fail "$LEDGER" "$tmp/const_moved.rs" \
  "gate passed when it could not find the constant — extraction breaking must be loud"

echo "dialect-epochs negative controls: 1 positive + 5 negatives all behaved"
