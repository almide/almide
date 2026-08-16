#!/usr/bin/env bash
# DIALECT EPOCH LEDGER GATE.
#
# `@dialect(N)` in a source file means "verified against dialect epoch N", and
# what epoch N *is* lives in proofs/dialect-epochs.toml. The compiler carries
# the current epoch as a constant. Two hand-maintained copies of one number
# disagree eventually — this gate is the cross-check, the same shape as
# rustc's CURRENT_RUSTC_VERSION placeholder check.
#
# Asserts:
#   1. the ledger parses and is non-empty (a vacuous ledger is a failure, not
#      a pass — the #976 find-nothing-exit-0 class)
#   2. epochs are positive, unique, strictly ascending and GAPLESS (1..=max),
#      so a stamp can never name an epoch that was skipped
#   3. every epoch above 1 lists at least one `breaks` entry — an epoch with
#      no break is a release, and releases are not epochs
#   4. max(epoch) == CURRENT_DIALECT in crates/almide-types/src/dialect.rs
set -uo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Paths are overridable so check-dialect-epochs-negative.sh can feed this gate
# forged ledgers and forged constants; the defaults are the real ones.
LEDGER="${1:-$ROOT/proofs/dialect-epochs.toml}"
CONST_FILE="${2:-$ROOT/crates/almide-types/src/dialect.rs}"

fail=0
err() { echo "::error::$1"; fail=1; }

[ -f "$LEDGER" ] || { err "$LEDGER missing"; exit 1; }
[ -f "$CONST_FILE" ] || { err "$CONST_FILE missing"; exit 1; }

max_epoch=$(awk '
  /^\[\[epoch\]\]/ { in_block = 1; n = ""; breaks_open = 0; breaks_items = 0; count++ ; next }
  in_block && /^n *=/ {
    n = $3
    if (n !~ /^[0-9]+$/ || n + 0 < 1) { printf "::error::epoch %d: `n` must be a positive integer\n", count > "/dev/stderr"; bad = 1 }
    if (n + 0 != last + 1) { printf "::error::epoch %s: not gapless/ascending (previous was %d)\n", n, last > "/dev/stderr"; bad = 1 }
    last = n + 0
    next
  }
  in_block && /^breaks *= *\[/ { breaks_open = 1; if ($0 ~ /\]/) { breaks_open = 0 } ; next }
  breaks_open && /^ *"/ { breaks_items++; next }
  breaks_open && /^\]/ {
    breaks_open = 0
    if (last > 1 && breaks_items == 0) { printf "::error::epoch %d: lists no `breaks` — an epoch with no break is a release, not an epoch\n", last > "/dev/stderr"; bad = 1 }
    breaks_items = 0
    next
  }
  END {
    if (count == 0) { print "::error::no [[epoch]] entries — a vacuous ledger cannot pass" > "/dev/stderr"; exit 1 }
    if (bad) exit 1
    print last
  }
' "$LEDGER") || fail=1

[ "$fail" -eq 0 ] || exit 1

const_epoch=$(grep -oE 'CURRENT_DIALECT: u32 = [0-9]+' "$CONST_FILE" | grep -oE '[0-9]+$')
if [ -z "$const_epoch" ]; then
  err "could not read CURRENT_DIALECT from $CONST_FILE — the extraction broke, not the ledger"
  exit 1
fi

if [ "$max_epoch" != "$const_epoch" ]; then
  err "dialect drift: ledger's highest epoch is $max_epoch but CURRENT_DIALECT is $const_epoch — bump both in the same change"
  exit 1
fi

echo "dialect-epochs: OK — $max_epoch epoch(s), CURRENT_DIALECT agrees"
