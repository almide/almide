#!/usr/bin/env bash
# Measure each leg's `main` work the way check-native-v1-perf-small.sh does,
# for an empty program and a program that prints one line.
set -u
W=$(mktemp -d)
in_main() {
  local out="$W/cg.$RANDOM"
  valgrind --tool=callgrind --fair-sched=yes --collect-atstart=no --toggle-collect='almide_*::main' --toggle-collect='*thread_start*' \
    --toggle-collect='*almide_stack_guard_install' --callgrind-out-file="$out" "$1" </dev/null >/dev/null 2>&1
  callgrind_annotate --auto=no "$out" 2>/dev/null | awk '/PROGRAM TOTALS/ { gsub(",", "", $1); print $1 }'
  callgrind_annotate --inclusive=no "$out" 2>/dev/null | grep -E "^ *[0-9,]+ \(" | head -12 | cut -c1-150 > "$W/top.txt"
}
printf 'fn main() -> Unit = ()\n' > "$W/empty.almd"
printf 'fn main() -> Unit = println("x")\n' > "$W/one.almd"
printf 'fn main() -> Unit = {\n  println("x")\n  println("y")\n}\n' > "$W/two.almd"
for p in empty one two; do
  ALMIDE_VERIFIED_DEBUG=1 ./target/release/almide build "$W/$p.almd" -o "$W/$p.v1" >/dev/null 2>&1
  ALMIDE_NO_VERIFIED_OK=1 ./target/release/almide build "$W/$p.almd" --no-verified -o "$W/$p.v0" >/dev/null 2>&1
  for l in v0 v1; do n=$(in_main "$W/$p.$l"); echo "GLUE $p $l $n"; sed 's/^/GLUETOP /' "$W/top.txt"; done
done
