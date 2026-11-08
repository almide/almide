#!/usr/bin/env bash
# Probe: for each program in $1, build v1 and v0, keep the rendered Rust, and
# callgrind both with the small gate's toggles (program work inside main only).
set -u
A=target/release/almide
OUT=probe-out; mkdir -p $OUT
T="${TMPDIR:-/tmp}"
: > $OUT/summary.txt
while read -r f; do
  [ -z "$f" ] && continue
  n="$(basename "$f" .almd)"
  for leg in v1 v1n v0; do
    touch "$OUT/.mark"; sleep 1
    if [ $leg = v1n ]; then
      PROBE_NODA=1 timeout 600 $A build "$f" -o "$OUT/$n.$leg.bin" > "$OUT/$n.$leg.buildlog" 2>&1
    elif [ $leg = v1 ]; then
      ALMIDE_VERIFIED_DEBUG=1 timeout 600 $A build "$f" -o "$OUT/$n.$leg.bin" > "$OUT/$n.$leg.buildlog" 2>&1
    else
      ALMIDE_NO_VERIFIED_OK=1 timeout 600 $A build "$f" --no-verified -o "$OUT/$n.$leg.bin" > "$OUT/$n.$leg.buildlog" 2>&1
    fi
    src=$(find "$T" "$HOME" /tmp -newer "$OUT/.mark" \( -name almide_gen_main.rs -o -name main.rs \) -path '*almide*' 2>/dev/null | head -1)
    [ -n "$src" ] && cp "$src" "$OUT/$n.$leg.rs"
    if [ -x "$OUT/$n.$leg.bin" ]; then
      timeout 900 valgrind --tool=callgrind --fair-sched=yes --collect-atstart=no --toggle-collect='almide_*::main' --toggle-collect='*thread_start*' \
        --toggle-collect='*almide_stack_guard_install' --callgrind-out-file="$OUT/$n.$leg.cg" "$OUT/$n.$leg.bin" > "$OUT/$n.$leg.stdout" 2>&1 </dev/null
      callgrind_annotate --inclusive=no "$OUT/$n.$leg.cg" 2>/dev/null | head -60 > "$OUT/$n.$leg.top"
      callgrind_annotate --inclusive=yes "$OUT/$n.$leg.cg" 2>/dev/null | head -60 > "$OUT/$n.$leg.itop"
      callgrind_annotate --tree=caller --inclusive=yes "$OUT/$n.$leg.cg" 2>/dev/null > "$OUT/$n.$leg.tree"
      tot=$(callgrind_annotate --auto=no "$OUT/$n.$leg.cg" 2>/dev/null | awk '/PROGRAM TOTALS/ { gsub(",", "", $1); print $1 }')
      echo "$n $leg $tot" >> $OUT/summary.txt
      rm -f "$OUT/$n.$leg.cg" "$OUT/$n.$leg.bin"
    else
      echo "$n $leg BUILD-FAIL" >> $OUT/summary.txt
    fi
  done
  echo "done $f"
done < "$1"
awk '{k=$1; if($2=="v1") a[k]=$3; else if($2=="v1n") c[k]=$3; else b[k]=$3} END {for (k in a) if (b[k]>0) printf "%s v1=%s v1n=%s v0=%s ratio=%.3f noda=%.3f\n", k, a[k], c[k], b[k], a[k]/b[k], c[k]/b[k]}' $OUT/summary.txt | sort > $OUT/ratios.txt
cat $OUT/ratios.txt
