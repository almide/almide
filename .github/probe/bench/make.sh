#!/bin/bash
# make.sh <outdir>: jr_{full,build,str} at n=50000 and n=200
set -eu
d=$(cd "$(dirname "$0")" && pwd); out=$1; mkdir -p "$out"
for m in full build str; do
  cat "$d/head.almd.inc" "$d/main_$m.inc" > "$out/jr_$m.almd"
  sed 's/^let n = 50000$/let n = 200/' "$out/jr_$m.almd" > "$out/jr_${m}_small.almd"
done
cp "$d/long.almd" "$out/jr_long_small.almd"
