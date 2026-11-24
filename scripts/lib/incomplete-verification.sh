# shellcheck shell=bash
# The corpus's own use of `--allow-incomplete-verification` (C-462), read from
# proofs/incomplete-verification.toml — sourced by the harnesses that make a
# wasm module of a corpus program.
#
#   incomplete_flag ROUTE PATH   prints the flag when PATH is listed for ROUTE
#                                (test | run | build | bench), nothing otherwise.
#
# A harness that runs a whole directory in one command (`almide test spec/
# --target wasm`) passes the flag unconditionally and hands its log to
# `scripts/check-incomplete-verification.sh ROUTE LOG`, which refuses an
# INCOMPLETE warning from a program the ledger does not list.

INCOMPLETE_LEDGER="${INCOMPLETE_LEDGER:-$(git rev-parse --show-toplevel 2>/dev/null || pwd)/proofs/incomplete-verification.toml}"

incomplete_flag() {
  local route="$1" path="${2#./}"
  if grep -qxF "entry = \"$route $path\"" "$INCOMPLETE_LEDGER" 2>/dev/null; then
    printf '%s' "--allow-incomplete-verification"
  fi
}
