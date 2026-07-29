#!/usr/bin/env bash
# Frees churn gate: build each spec/churn fixture with ALMIDE_WASM_FREES=1,
# run on wasmtime under a wall-clock kill, compare to native output.
set -euo pipefail
BIN="${ALMIDE_BIN:-target/release/almide}"

# wasmtime presence up front (#980): perl's failed `exec` warns and exits 0,
# so a missing wasmtime used to be captured as the wasm "output" and reported
# as the misleading "FAIL (output diverges)". In CI a missing tool is a
# failure; locally it is an honest skip.
if ! command -v wasmtime >/dev/null; then
  if [ "${CI:-}" = "true" ]; then
    echo "::error::frees-churn: wasmtime not found — in CI a missing tool is a failure (#980)"
    exit 1
  fi
  echo "frees-churn: wasmtime not found — SKIP"
  exit 0
fi
# An emptied corpus must not pass vacuously (#976 class).
count=$(ls spec/churn/*.almd 2>/dev/null | wc -l | tr -d ' ')
[ "$count" -ge 5 ] || { echo "::error::frees-churn: only $count fixtures in spec/churn — the corpus moved (#980)"; exit 1; }

fail=0
for f in spec/churn/*.almd; do
  name=$(basename "$f" .almd)
  native=$("$BIN" run "$f" 2>/dev/null)
  ALMIDE_WASM_FREES=1 ALMIDE_NO_VERIFIED_OK=1 "$BIN" build "$f" --target wasm --no-verified -o /tmp/churn_gate.wasm >/dev/null
  # No timeout(1) on macOS: use perl alarm. A hang here is a free-list cycle.
  wasm=$(perl -e 'alarm 600; exec @ARGV' wasmtime /tmp/churn_gate.wasm 2>&1) || { echo "FAIL $name (exit $?)"; fail=1; continue; }
  if [ "$native" != "$wasm" ]; then echo "FAIL $name (output diverges)"; fail=1; else echo "ok $name"; fi
done
exit $fail
