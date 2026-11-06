#!/usr/bin/env bash
# Usage: run.sh <almide-binary> <dir-of-probes>
set -u
A="$1"; DIR="$2"
"$A" --version
filt() { grep -E 'wall|WALL|declin|Unsupported|passed|failed|error|E08|=|SKIP|FAIL|panic' | grep -vE '^\s*(\||[0-9]+ \|)|hint: Prefix|warning' | cut -c1-600 | head -12 | sed 's/^/      /'; }
for f in "$DIR"/*.almd; do
  n=$(basename "$f" .almd)
  if grep -q '^test "' "$f"; then sub=test; else sub=run; fi
  echo "== $n [$sub]"
  echo "   native:"; timeout 300 "$A" $sub "$f" 2>&1 | filt
  echo "   wasm:";   ALMIDE_WALL_REASON=1 timeout 300 "$A" $sub "$f" --target wasm 2>&1 | filt
done
