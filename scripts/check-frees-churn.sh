#!/usr/bin/env bash
# Frees churn gate: build each spec/churn fixture with ALMIDE_WASM_FREES=1,
# run on wasmtime under a wall-clock kill, compare to native output.
set -euo pipefail
BIN="${ALMIDE_BIN:-target/release/almide}"
fail=0
for f in spec/churn/*.almd; do
  name=$(basename "$f" .almd)
  native=$("$BIN" run "$f" 2>/dev/null)
  ALMIDE_WASM_FREES=1 "$BIN" build "$f" --target wasm --no-verified -o /tmp/churn_gate.wasm >/dev/null
  # No timeout(1) on macOS: use perl alarm. A hang here is a free-list cycle.
  wasm=$(perl -e 'alarm 600; exec @ARGV' wasmtime /tmp/churn_gate.wasm 2>&1) || { echo "FAIL $name (exit $?)"; fail=1; continue; }
  if [ "$native" != "$wasm" ]; then echo "FAIL $name (output diverges)"; fail=1; else echo "ok $name"; fi
done
exit $fail
