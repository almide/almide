#!/usr/bin/env bash
# RETIRED SPELLING GATE — every retirement is EXECUTED, not asserted.
#
# `proofs/retired-spellings.toml` lists names the language will not take back.
# The diagnostic that rejects them lives in a hardcoded `match` in
# `check/infer.rs`, so deleting one arm removes one retirement silently. This
# gate closes that by COMPILING a probe for every row and requiring the
# declared diagnostic code to come out — a retirement nobody can demonstrate
# is a retirement that already came back.
#
# Asserts:
#   1. the ledger parses and is non-empty (a vacuous ledger is a failure, not
#      a pass — the #976 find-nothing-exit-0 class)
#   2. every row's spelling still produces its declared diagnostic code
#   3. a `kind = "spelling"` row is not DEFINED anywhere in stdlib/ (the name
#      coming back as a real function is the other way a retirement dies). A
#      `kind = "carrier"` row legitimately keeps its definition — it is the
#      desugar target; only source's ability to NAME it was retired, which
#      check 2 already proves.
#
# Usage: check-retired-spellings.sh [ledger] [almide-binary]
set -uo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="${1:-$ROOT/proofs/retired-spellings.toml}"
BIN="${2:-${ALMIDE_BIN:-$ROOT/target/release/almide}}"

fail=0
err() { echo "::error::$1"; fail=1; }

[ -f "$LEDGER" ] || { err "$LEDGER missing"; exit 1; }
[ -x "$BIN" ] || { err "almide binary not found at $BIN — build with 'cargo build --release' first"; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Rows as `spelling<TAB>code`, in file order.
rows=$(awk -F' = ' '
  /^spelling *=/ { s = $2; gsub(/"/, "", s) }
  /^code *=/ { c = $2; gsub(/"/, "", c) }
  /^kind *=/ { k = $2; gsub(/"/, "", k); printf "%s\t%s\t%s\n", s, c, k }
' "$LEDGER")

count=$(printf '%s\n' "$rows" | grep -c . || true)
if [ "$count" -eq 0 ]; then
  err "no [[retired]] rows — a ledger that lists nothing cannot pass"
  exit 1
fi

checked=0
while IFS=$'\t' read -r spelling code kind; do
  [ -n "$spelling" ] || continue
  module="${spelling%%.*}"
  probe="$tmp/probe.almd"
  # One probe shape per module family. The call only has to REACH name
  # resolution — the retirement fires there, before argument checking.
  case "$module" in
    fs)
      printf 'import fs\n\neffect fn main() -> Unit = {\n  let v = %s("f", 0, (a, l) => int.parse(l)!)\n  println("${v}")\n}\n' "$spelling" > "$probe"
      ;;
    *)
      printf 'effect fn main() -> Unit = {\n  let xs = ["1"]\n  let v = %s(xs, (x) => int.parse(x)!)\n  println("${v}")\n}\n' "$spelling" > "$probe"
      ;;
  esac
  got=$("$BIN" check "$probe" 2>&1 | grep -oE "error\[$code\]" | head -1 || true)
  if [ -z "$got" ]; then
    err "retired spelling '$spelling' no longer produces $code — the retirement is not enforced any more"
  fi
  checked=$((checked + 1))
done <<< "$rows"

# A retired name reappearing as a real stdlib definition is the other death.
while IFS=$'\t' read -r spelling _code kind; do
  [ -n "$spelling" ] || continue
  [ "$kind" = "spelling" ] || continue
  bare="${spelling#*.}"
  if grep -rqE "^\s*(pub )?(effect )?fn ${bare}\b" "$ROOT/stdlib/" 2>/dev/null; then
    err "retired spelling '$spelling' is defined again in stdlib/ — a retirement cannot be undone by redefinition"
  fi
done <<< "$rows"

[ "$fail" -eq 0 ] || exit 1
echo "retired-spellings: OK — $checked retirement(s), each demonstrated to still fire"
