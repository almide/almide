#!/usr/bin/env bash
# Reproduction for ADR-0011 / execution-inception.md §1.
#
# Runs the SAME 4-arm `fan {}` program on both targets N times and tallies the
# arm print order. Under the #1000 stance ("the observation equals the
# list-order sequential one") every run on every target must print A B C D.
#
#   wasm   — one distinct order (ABCD). Correct.
#   native — MANY distinct orders, because the arms are real scoped threads and
#            nothing buffers their output. This is not merely a native ⇄ wasm
#            divergence: it is a native ⇄ NATIVE one, i.e. the property whose
#            sole remaining instance C-006 claims was retired in 0.29.0.
#
# Exits 0 when the divergence reproduces (the defect is still present), and 1
# when every target agrees — which is what landing Rung 1 (arm-scoped output
# transactions) must make happen. Flip it into a gate then.
#
# Usage: research/spike/substrate-observability/run-repro.sh [runs]
set -euo pipefail
cd "$(dirname "$0")"

RUNS="${1:-10}"
SRC="arm_output_order.almd"
BIN="$(mktemp -d)/repro_bin"
trap 'rm -rf "$(dirname "$BIN")"' EXIT

command -v almide >/dev/null || { echo "almide not on PATH"; exit 2; }

echo "almide: $(almide --version)"
command -v wasmtime >/dev/null && echo "wasmtime: $(wasmtime --version)"
echo "cores: $(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo '?')"
echo

order() { grep -oE '^[ABCD]$' | tr -d '\n'; echo; }

almide build "$SRC" -o "$BIN" >/dev/null
echo "=== native x$RUNS ==="
native_orders=$(for _ in $(seq 1 "$RUNS"); do "$BIN" | order; done | sort | uniq -c | sort -rn)
echo "$native_orders"
n_native=$(echo "$native_orders" | wc -l | tr -d ' ')

echo
echo "=== wasm x3 ==="
wasm_orders=$(for _ in 1 2 3; do almide run "$SRC" --target wasm 2>/dev/null | order; done | sort | uniq -c)
echo "$wasm_orders"
n_wasm=$(echo "$wasm_orders" | wc -l | tr -d ' ')

echo
echo "distinct orders — native: $n_native, wasm: $n_wasm"
if [ "$n_native" -gt 1 ]; then
  echo "REPRODUCED: native disagrees with itself across runs (C-004 EXCEPTION, misclassified as cross-target)."
  exit 0
fi
echo "NOT REPRODUCED: every target agreed. Either Rung 1 has landed (turn this into a gate),"
echo "or the host serialised the arms anyway (try more runs, or a machine with more cores)."
exit 1
