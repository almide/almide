#!/usr/bin/env bash
# #912 diagnostic-divergence lens: every check-accepted
# tests/diagnostics/*/fixed.almd must EMIT `--target rust` cleanly
# (`almide <file> --target rust`, the source-emit form).
#
# EMIT, not `almide build`: the lens hunts check-accepts-but-EMIT-REINTERPRETS
# (a silent divergence between the checker's reading and codegen's). `build`
# additionally runs capability gates — e.g. fan.bounded/fan.race's v1-native
# requirement — whose failures are LOUD declared refusals, not silent
# divergences; measuring through `build` mixes that wall class into the lens
# (3 fan fixtures fail `build` on every release back to 0.56.0). Round 3
# re-derived this the hard way — the protocol is pinned here so it cannot
# drift again.
set -u
BIN=${ALMIDE_BIN:-./target/release/almide}
fails=0; total=0
for f in tests/diagnostics/*/fixed.almd; do
  total=$((total+1))
  if ! "$BIN" "$f" --target rust >/dev/null 2>&1; then
    fails=$((fails+1)); echo "DIVERGE: $f"
  fi
done
echo "diag lens: $((total-fails))/$total emit-ok, $fails divergences"
[ "$fails" -eq 0 ]
