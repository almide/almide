#!/usr/bin/env bash
# The wasm coverage ratchet gate (mission-critical arc). Runs the hybrid
# suite with ALMIDE_FALLBACK_NAMES=1 and diffs the observed fallback set
# against proofs/wasm-fallback-baseline.txt:
#   - a fallback file NOT in the baseline  -> FAIL (wasm coverage regressed);
#   - a baseline entry whose file now runs the wasm leg -> FAIL as STALE
#     (prune it in the same change — the set only shrinks).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASELINE="$ROOT/proofs/wasm-fallback-baseline.txt"
BIN="${ALMIDE_BIN:-almide}"

observed=$(cd "$ROOT" && ALMIDE_FALLBACK_NAMES=1 "$BIN" test 2>&1 | sed -n 's/^FALLBACK //p' | sort)
listed=$(grep -v '^#' "$BASELINE" | awk -F' :: ' 'NF>=2 {print $1}' | sort)

new=$(comm -23 <(echo "$observed") <(echo "$listed") | sed '/^$/d' || true)
stale=$(comm -13 <(echo "$observed") <(echo "$listed") | sed '/^$/d' || true)

fail=0
if [ -n "$new" ]; then
  echo "WASM COVERAGE REGRESSION — file(s) fell off the wasm leg and are NOT in the baseline:" >&2
  echo "$new" | sed 's/^/  + /' >&2
  fail=1
fi
if [ -n "$stale" ]; then
  echo "STALE baseline entr(ies) — these files now run the wasm leg; prune them (the set only shrinks):" >&2
  echo "$stale" | sed 's/^/  - /' >&2
  fail=1
fi
if [ "$fail" -ne 0 ]; then exit 1; fi
n=$(echo "$listed" | sed '/^$/d' | wc -l | tr -d ' ')
echo "WASM COVERAGE RATCHET OK: $n fallback file(s), all enumerated in the baseline (target: 0/empty)."
