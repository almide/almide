#!/usr/bin/env bash
set -u
mkdir -p hand-out rtlib
rustc hand/rt.rs --crate-type lib --crate-name almide_v1_rt --edition 2021 -C overflow-checks=off -A warnings -C debug-assertions=off -C opt-level=1 --out-dir rtlib
for v in orig narrow narrow2; do
  rustc hand/$v.rs --crate-name almide_out --edition 2021 -C overflow-checks=off -A warnings -C opt-level=1 -C debug-assertions=on -o hand-out/$v.bin --extern almide_v1_rt=rtlib/libalmide_v1_rt.rlib -L rtlib || { echo "$v BUILD-FAIL"; continue; }
  hand-out/$v.bin > hand-out/$v.stdout
  objdump -d -C --no-show-raw-insn hand-out/$v.bin | awk '/<almide_out::main>:$/ {p=1} /^$/ {p=0} p' | head -40 > hand-out/$v.asm
  valgrind --tool=callgrind --collect-atstart=no --toggle-collect='almide_*::main' --toggle-collect='*almide_stack_guard_install' --callgrind-out-file=hand-out/$v.cg hand-out/$v.bin > /dev/null 2>&1
  echo "$v $(callgrind_annotate --auto=no hand-out/$v.cg | awk '/PROGRAM TOTALS/ {gsub(",","",$1); print $1}') $(cat hand-out/$v.stdout)" | tee -a hand-out/summary.txt
done
