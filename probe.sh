#!/usr/bin/env bash
set -u
A=target/release/almide
OUT=probe-out; mkdir -p $OUT
T="${TMPDIR:-/tmp}"
while read -r f; do
  [ -z "$f" ] && continue
  n="$(basename "$f" .almd)"
  for leg in v1 v0; do
    touch "$OUT/.mark"; sleep 1
    if [ $leg = v1 ]; then
      ALMIDE_VERIFIED_DEBUG=1 timeout 600 $A build "$f" -o "$OUT/$n.$leg.bin" > "$OUT/$n.$leg.buildlog" 2>&1
    else
      ALMIDE_NO_VERIFIED_OK=1 timeout 600 $A build "$f" --no-verified -o "$OUT/$n.$leg.bin" > "$OUT/$n.$leg.buildlog" 2>&1
    fi
    src=$(find "$T" "$HOME" /tmp -newer "$OUT/.mark" \( -name almide_gen_main.rs -o -name main.rs \) -path '*almide*' 2>/dev/null | head -1)
    [ -n "$src" ] && cp "$src" "$OUT/$n.$leg.rs"
    if [ -x "$OUT/$n.$leg.bin" ]; then
      valgrind --tool=callgrind --callgrind-out-file="$OUT/$n.$leg.cg" "$OUT/$n.$leg.bin" > /dev/null 2>&1 </dev/null
      callgrind_annotate --inclusive=no "$OUT/$n.$leg.cg" 2>/dev/null | head -45 > "$OUT/$n.$leg.top"
      callgrind_annotate --inclusive=yes "$OUT/$n.$leg.cg" 2>/dev/null | head -45 > "$OUT/$n.$leg.itop"
      rm -f "$OUT/$n.$leg.cg" "$OUT/$n.$leg.bin"
    fi
  done
  echo "done $f"
done < "$1"
