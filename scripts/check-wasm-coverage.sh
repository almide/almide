#!/usr/bin/env bash
# Structural coverage of the wasm backend's EMITTER (flight-gap A-2).
#
# Measures line/region coverage of crates/almide-wasm/src/ under its full
# test net (590-fixture parity, 200-seed differential fuzz, first light,
# alias referee). HONEST SCOPE: this instruments the emitter's Rust — the
# EMITTED wasm runtime helpers execute inside wasmtime and are outside
# llvm-cov's reach; their witnesses are the corpus, the fuzzer, and the
# per-slice mutation evidence (PORTLOG).
#
# Usage:
#   bash scripts/check-wasm-coverage.sh            # summary + floor check
#   bash scripts/check-wasm-coverage.sh --report   # + per-function detail
#
# Requires cargo-llvm-cov (any location on PATH).

set -euo pipefail
cd "$(dirname "$0")/.."

# Grow-only floor (line coverage %, emitter sources only). Raise as
# uncovered branches are witnessed or retired — never lower.
FLOOR=90

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "cargo-llvm-cov not found on PATH" >&2
  exit 2
fi

SUMMARY=$(cargo llvm-cov --package almide-wasm --json --summary-only 2>/dev/null)
LINE_PCT=$(printf '%s' "$SUMMARY" | python3 -c "
import json, sys
d = json.load(sys.stdin)
totals = d['data'][0]['totals']['lines']
# Instrumentation-broken guard (stolen from roc build.zig:1696): a zero
# line count means the MEASUREMENT died, which must never read as a pass.
if totals['count'] == 0:
    print('BROKEN')
else:
    print(f\"{totals['percent']:.1f}\")
")
if [ "$LINE_PCT" = "BROKEN" ]; then
  echo "FAIL: coverage instrumentation reported zero lines — measurement is broken, not passing" >&2
  exit 1
fi

echo "almide-wasm emitter line coverage: ${LINE_PCT}% (floor ${FLOOR}%)"

if [ "${1:-}" = "--report" ]; then
  cargo llvm-cov report 2>/dev/null | grep -E "src/|TOTAL" || true
fi

python3 - "$LINE_PCT" "$FLOOR" <<'EOF'
import sys
pct, floor = float(sys.argv[1]), float(sys.argv[2])
if pct < floor:
    print(f"FAIL: coverage {pct}% fell below the grow-only floor {floor}%")
    sys.exit(1)
print("OK")
EOF
