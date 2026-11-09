#!/bin/bash
# quick.sh <almide> <src-tree> <outdir>: behaviour checks of the json/value fixtures on one compiler.
set -uo pipefail
A=$1; T=$2; O=$3; mkdir -p "$O"
cd "$T"
fails=0
# 1. every json/codec/value wasm_cross fixture: wasm output hash vs the committed run manifest, and native == wasm
for f in $(ls spec/wasm_cross/ | grep -E '^(json|codec|value|http_decode_json)' | sed 's|^|spec/wasm_cross/|'); do
  w=$("$A" run "$f" --target wasm 2>"$O/err.txt"); wrc=$?
  n=$("$A" run "$f" 2>"$O/nerr.txt"); nrc=$?
  if [ -n "$w" ]; then h=$(printf '%s\n' "$w" | sha256sum | cut -d' ' -f1); else h=$(printf '' | sha256sum | cut -d' ' -f1); fi
  want=$(grep -P "\t$f\$" crates/almide-spine/tests/golden/spec-run-manifest.txt | cut -f1,2)
  got="$h	$wrc"
  st=OK
  [ "$w" != "$n" ] || [ "$wrc" != "$nrc" ] && st=DIVERGE
  [ -n "$want" ] && [ "$want" != "$got" ] && st="$st MANIFEST($want vs $got)"
  [ -z "$want" ] && st="$st NEW"
  echo "$st $f"
  if [ "$st" != OK ]; then
    fails=$((fails+1))
    { echo "=== $f"; echo "--- native rc=$nrc"; printf '%s\n' "$n" | head -c 20000; cat "$O/nerr.txt" | head -20; echo "--- wasm rc=$wrc"; printf '%s\n' "$w" | head -c 20000; cat "$O/err.txt" | head -20; } >> "$O/diverge.txt"
  fi
done
# 2. the json/value stdlib tests on both legs
for t in $(ls spec/stdlib/ | grep -E '^(json|value|codec|coverage_json)' | sed 's|^|spec/stdlib/|'); do
  "$A" test "$t" > "$O/t.txt" 2>&1; rn=$?
  "$A" test "$t" --target wasm > "$O/tw.txt" 2>&1; rw=$?
  echo "test native=$rn wasm=$rw $t"
  [ $rn = 0 ] && [ $rw = 0 ] || { fails=$((fails+1)); { echo "=== $t"; tail -30 "$O/t.txt"; tail -30 "$O/tw.txt"; } >> "$O/diverge.txt"; }
done
echo "FAILS $fails"
