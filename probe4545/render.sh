#!/usr/bin/env bash
# usage: render.sh <almide-bin> <outdir>: lowered IR of every spec .almd, the v0 Rust emit and the
# wasm bytes of every spec/wasm_cross fixture, for a before/after identity compare.
set -u
A=$1; O=$2
mkdir -p $O/ir $O/rs $O/wasm
: > $O/status.tsv
find spec -name '*.almd' | LC_ALL=C sort > $O/files.txt
while read -r f; do
  k=$(echo "$f" | tr '/' '__')
  timeout 60 $A "$f" --emit-ir > "$O/ir/$k.json" 2> "$O/ir/$k.err"; s=$?
  printf 'ir\t%s\t%s\t%s\n' "$s" "$(sha256sum < "$O/ir/$k.json" | cut -c1-16)" "$f" >> $O/status.tsv
done < $O/files.txt
for f in spec/wasm_cross/*.almd; do
  k=$(echo "$f" | tr '/' '__')
  timeout 120 $A build "$f" --target wasm -o "$O/wasm/$k.wasm" > "$O/wasm/$k.log" 2>&1; s=$?
  h=$( [ -f "$O/wasm/$k.wasm" ] && sha256sum < "$O/wasm/$k.wasm" | cut -c1-16 || echo none)
  printf 'wasm\t%s\t%s\t%s\n' "$s" "$h" "$f" >> $O/status.tsv
  timeout 60 $A "$f" --target rust > "$O/rs/$k.rs" 2> "$O/rs/$k.err"; s=$?
  printf 'rs\t%s\t%s\t%s\n' "$s" "$(sha256sum < "$O/rs/$k.rs" | cut -c1-16)" "$f" >> $O/status.tsv
done
rm -f $O/wasm/*.wasm
echo "rendered $(wc -l < $O/status.tsv) rows"
