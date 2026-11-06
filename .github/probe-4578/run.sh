#!/usr/bin/env bash
# Usage: run.sh <almide-binary> <dir-of-probes>
set -u
A="$1"; DIR="$2"
"$A" --version
for f in "$DIR"/*.almd; do
  n=$(basename "$f" .almd)
  if grep -q '^test "' "$f"; then sub=test; else sub=run; fi
  nat=$(timeout 300 "$A" $sub "$f" 2>&1 | tail -4 | tr '\n' ' '); nst=$?
  was=$(ALMIDE_WALL_REASON=1 timeout 300 "$A" $sub "$f" --target wasm 2>&1 | tail -6 | tr '\n' ' ')
  echo "== $n [$sub]"
  echo "   native: $nat"
  echo "   wasm  : $was"
done
