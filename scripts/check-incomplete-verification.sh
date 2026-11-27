#!/usr/bin/env bash
# C-462 (owner ruling 2026-11-24): a wasm module whose own certificate is
# INCOMPLETE is refused by default; `--allow-incomplete-verification` lets it
# through with a `warning[verification-incomplete]: <file>: passed under …`
# line. The corpus carries the flag only for the (route, program) pairs
# proofs/incomplete-verification.toml lists (its count and shape are held by
# tests/incomplete_verification_ledger_test.rs).
#
# A harness that runs a whole directory in one command passes the flag to
# every program; this check reads that run's log and fails on a warning from
# a program the ledger does not list for ROUTE — a new INCOMPLETE program is
# then refused exactly as the default would refuse it.
#
# With --full (the log is a run over the WHOLE corpus of that route, not a
# slice) a listed program the log does not warn about is a stale row — it is
# certified now: delete it and lower the ceiling.
#
#   check-incomplete-verification.sh ROUTE LOG [--full]
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LEDGER="$ROOT/proofs/incomplete-verification.toml"
route="${1:?usage: check-incomplete-verification.sh ROUTE LOG}"
log="${2:?usage: check-incomplete-verification.sh ROUTE LOG}"
[ -r "$LEDGER" ] || { echo "::error::incomplete-verification: no $LEDGER"; exit 2; }
[ -r "$log" ] || { echo "::error::incomplete-verification: no log $log"; exit 2; }

fail=0; n=0
while IFS= read -r path; do
  n=$((n + 1))
  path="${path#./}"
  case "$path" in "$ROOT"/*) path="${path#"$ROOT"/}" ;; esac
  if ! grep -qxF "entry = \"$route $path\"" "$LEDGER"; then
    echo "::error::incomplete-verification: $route $path is INCOMPLETE but proofs/incomplete-verification.toml does not list it — certify it in the producer"
    fail=1
  fi
done < <(sed -n 's/^warning\[verification-incomplete\]: \(.*\): passed under .*/\1/p' "$log" | sort -u)

if [ "${3:-}" = "--full" ]; then
  warned=$(sed -n 's/^warning\[verification-incomplete\]: \(.*\): passed under .*/\1/p' "$log" | sed "s#^\./##; s#^$ROOT/##" | sort -u)
  while IFS= read -r path; do
    if ! grep -qxF "$path" <<<"$warned"; then
      echo "::error::incomplete-verification: $route $path is listed but the run did not find it INCOMPLETE — a stale row: delete it and lower the ceiling"
      fail=1
    fi
  done < <(sed -n "s/^entry = \"$route \(.*\)\"\$/\1/p" "$LEDGER")
fi

[ "$fail" = 0 ] && echo "incomplete-verification: $n INCOMPLETE $route program(s) in $log, every one listed"
exit "$fail"
