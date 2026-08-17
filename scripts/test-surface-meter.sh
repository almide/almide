#!/usr/bin/env bash
# TEST-SURFACE METER (docs/roadmap/active/test-surface-25x.md).
# Prints the committed test-surface counts by tier; with --update, appends a
# dated row to the roadmap's Trend table. Informational — the 25× goal is a
# direction, not a ratchet (bulk-generated count-gaming is a NON-goal there).
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

wasm_cross=$(ls spec/wasm_cross/*.almd | wc -l | tr -d ' ')
other_almd=$(find spec -name '*.almd' | grep -vc wasm_cross)
test_fns=$(grep -rc '#\[test\]' tests/ crates/ --include='*.rs' 2>/dev/null | awk -F: '{s+=$2} END {print s}')
diag_pairs=$(ls -d tests/diagnostics/*/ | wc -l | tr -d ' ')
walls=$(ls proofs/wall-corpus/*.almd | wc -l | tr -d ' ')

echo "test-surface: wasm_cross=$wasm_cross other_almd=$other_almd test_fns=$test_fns diag_pairs=$diag_pairs walls=$walls"

if [ "${1:-}" = "--update" ]; then
  doc="docs/roadmap/active/test-surface-25x.md"
  today=$(date +%Y-%m-%d)
  row="| $today | $wasm_cross | $other_almd | $test_fns | $diag_pairs | $walls |"
  grep -qF "$row" "$doc" || printf '%s\n' "$row" >> "$doc"
  echo "trend row appended to $doc"
fi
