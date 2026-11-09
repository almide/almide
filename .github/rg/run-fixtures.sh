#!/usr/bin/env bash
# Build each new fixture natively and on wasm; print exit + outputs; 20 native runs each.
set -u
A=$PWD/target/release/almide
for f in fan_ordered_direct_call_arms fan_arm_stderr_timeline never_tail_of_result_effect_fn; do
  src=/tmp/fx/$f.almd
  echo "===== $f"
  if $A build $src -o /tmp/fx/$f.bin >/tmp/fx/b.log 2>&1; then
    declare -A seen=()
    for i in $(seq 1 20); do o=$( /tmp/fx/$f.bin 2>/tmp/fx/e; echo "exit=$?"; echo "--stderr:"; cat /tmp/fx/e ); seen["$o"]=$(( ${seen["$o"]:-0} + 1 )); done
    for k in "${!seen[@]}"; do echo "native x${seen[$k]}:"; echo "$k" | sed 's/^/    /'; done
    unset seen
  else echo "native BUILD FAILED:"; tail -15 /tmp/fx/b.log | sed 's/^/    /'; fi
  o=$( $A run $src --target wasm 2>/tmp/fx/e; echo "exit=$?"; echo "--stderr:"; cat /tmp/fx/e ); echo "wasm:"; echo "$o" | sed 's/^/    /'
done
