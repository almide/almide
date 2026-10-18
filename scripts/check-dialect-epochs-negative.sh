#!/usr/bin/env bash
# Negative controls for the dialect-epoch gate: prove check-dialect-epochs.sh
# FIRES on each violation it claims to catch. The committed ledger and the
# real constant are the positive control.
set -euo pipefail
cd "$(dirname "$0")/.."

GATE="bash scripts/check-dialect-epochs.sh"
LEDGER=proofs/dialect-epochs.toml
CONST=crates/almide-types/src/dialect.rs

# Read the current epoch rather than spelling it: a hardcoded number turns every
# mutation below into a no-op sed the moment an epoch is declared, and a negative
# control that mutates nothing reports the gate as broken while the gate is fine.
CUR=$(grep -oE 'CURRENT_DIALECT: u32 = [0-9]+' "$CONST" | grep -oE '[0-9]+$')
[ -n "$CUR" ] || { echo "FAIL: cannot read CURRENT_DIALECT from $CONST" >&2; exit 1; }

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

# A leftover merge marker leaves the ledger unparseable while every awk rule
# below still finds its blocks — the shape that sat on develop after epoch 11.
{ cat "$LEDGER"; printf '=======\n'; } >"$tmp/marker.toml"
expect_fail "$tmp/marker.toml" "$CONST" \
  "gate passed a ledger with a merge marker — an unparseable ledger reads as green"

# A gap in the epoch sequence: a stamp could name an epoch that never existed.
# The gap is cut out of the MIDDLE so the highest epoch still matches the
# constant — otherwise the gate could fail on the drift rule and this control
# would never touch the rule it names.
python3 - "$LEDGER" "$tmp/gap.toml" "$CUR" <<'PY'
import re, sys
src, dst, cur = sys.argv[1], sys.argv[2], int(sys.argv[3])
if cur < 3:
    sys.exit("a middle gap needs at least three epochs; rewrite this control")
head, *blocks = open(src).read().split("[[epoch]]")
kept = [b for b in blocks if not re.search(rf"^n = {cur - 1}$", b, re.M)]
if len(kept) == len(blocks):
    sys.exit(f"the gap mutation matched no epoch n = {cur - 1}")
open(dst, "w").write(head + "".join("[[epoch]]" + b for b in kept))
PY
expect_fail "$tmp/gap.toml" "$CONST" \
  "gate passed a gapped epoch sequence — a stamp could name a skipped epoch"

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
sed "s/CURRENT_DIALECT: u32 = $CUR/CURRENT_DIALECT: u32 = $((CUR + 3))/" "$CONST" >"$tmp/const_drift.rs"
grep -q "CURRENT_DIALECT: u32 = $((CUR + 3))" "$tmp/const_drift.rs" || { echo "FAIL: the drift mutation matched nothing" >&2; exit 1; }
expect_fail "$LEDGER" "$tmp/const_drift.rs" \
  "gate passed a constant that disagrees with the ledger — the drift it exists to catch"

# Blind-gate guard: if the constant cannot be read the gate must go loud, not green.
sed "s/CURRENT_DIALECT: u32 = $CUR/CURRENT_DIALECT_RENAMED: u32 = $CUR/" "$CONST" >"$tmp/const_moved.rs"
grep -q "CURRENT_DIALECT_RENAMED" "$tmp/const_moved.rs" || { echo "FAIL: the rename mutation matched nothing" >&2; exit 1; }
expect_fail "$LEDGER" "$tmp/const_moved.rs" \
  "gate passed when it could not find the constant — extraction breaking must be loud"

# The deprecation rules run on a forged row appended to the ledger: the ledger
# may hold no open window at all, and a mutation of a row that is not there
# would test nothing. The well-formed row must pass first, or the two
# negatives below could fail for a reason other than the one they name.
dep_row() { printf '\n[[deprecation]]\nid = "forged"\nissue = 1\nsince_epoch = %s\nsince = "0.0.0"\nremoval_epoch = %s\nsummary = "x"\nremoval = "x"\n' "$1" "$2"; }
{ cat "$LEDGER"; dep_row "$CUR" "$((CUR + 1))"; } >"$tmp/dep_ok.toml"
expect_pass "$tmp/dep_ok.toml" "$CONST" \
  "a well-formed open deprecation did not pass — the two deprecation negatives would be meaningless"

# A deprecation removed no later than it was deprecated is no window at all.
{ cat "$LEDGER"; dep_row "$CUR" "$CUR"; } >"$tmp/dep_order.toml"
expect_fail "$tmp/dep_order.toml" "$CONST" \
  "gate passed a deprecation whose removal_epoch is not above its since_epoch"

# A window whose removal epoch has arrived and is still listed is overdue: the
# alias it describes outlived the epoch that promised to remove it (#3425).
{ cat "$LEDGER"; dep_row "$((CUR - 1))" "$CUR"; } >"$tmp/dep_overdue.toml"
expect_fail "$tmp/dep_overdue.toml" "$CONST" \
  "gate passed a deprecation whose removal_epoch has already arrived"

echo "dialect-epochs negative controls: 2 positives + 7 negatives all behaved"
