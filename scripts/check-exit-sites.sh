#!/usr/bin/env bash
# Every frame exit the structural emitter writes goes through the ExitPlan
# renderer (#1995): a `return` / `return_call` / `return_call_indirect` in a
# frame-emitting file must sit right after `emit_exit(` — the one writer of
# a frame's release instructions (crates/almide-wasm/src/exit_plan.rs).
#
# Why a source gate: #1988, #1990 and #2001 were the same defect — a new
# emitter path ended the frame without the release the epilogue performs,
# and nothing red said so. This makes an unplanned exit a failed build.
#
# What it cannot judge: the plan's CONTENT (which credits an edge releases)
# — that is the credit rows in crates/almide-wasm/tests/call_result_credit.rs
# and the structural witness through the kernel-proven checker (proofs/gate.sh).
#
# Runtime helpers are hand-written wasm bodies without an RC frame (the
# allocator, string primitives, display scanners, map probes) and the
# deterministic-budget unwind terminates the run: those files are listed
# below and are NOT frame emitters. Adding a file here is a review event.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC=crates/almide-wasm/src
NOT_FRAMES=(
  runtime.rs runtime_alloc.rs runtime_str.rs runtime_line.rs
  runtime_large.rs   # the >64 KiB free-list half of $alloc/$free (#3348)
  runtime_alloc_cow.rs   # $map_reserve and $cow, runtime_alloc.rs's helpers split out for the file budget (#4081)
  map_index.rs matrix_scalars.rs
  map_holes.rs   # map_index.rs's tombstone helpers ($mapidx_kill/grow/settle/dead), split out for the file budget (#3691)
  map_ordered.rs   # map_index.rs's flood-fallback AVL helper bodies (#4104): no RC frame of their own
  utf8_helpers.rs value_helpers.rs   # decoder / Value-equality helper bodies
  display.rs   # display/eq/scan helper bodies: no RC frame of their own
  deep_walk.rs   # the deep drop / eq / display walk of a recursive type's helper bodies (#4250, #4252): no RC frame of their own
  fuel.rs      # the deterministic-budget unwind (BudgetUnwind): the run terminates
)
WINDOW=4

fail=0
checked=0
for f in "$SRC"/*.rs; do
  base=$(basename "$f")
  skip=0
  for n in "${NOT_FRAMES[@]}"; do [ "$n" = "$base" ] && skip=1; done
  [ "$skip" = 1 ] && continue
  # The renderer itself writes no return; exit_plan.rs is the definition.
  [ "$base" = exit_plan.rs ] && continue
  while IFS=: read -r ln _; do
    checked=$((checked + 1))
    start=$((ln - WINDOW)); [ "$start" -lt 1 ] && start=1
    if ! sed -n "${start},${ln}p" "$f" | grep -q 'emit_exit('; then
      echo "FAIL: $f:$ln ends the frame without the ExitPlan renderer (no emit_exit( within $WINDOW lines above)"
      sed -n "${start},${ln}p" "$f" | sed 's/^/    | /'
      fail=1
    fi
  done < <(grep -nE '\.return_\(\)|\.return_call\(|\.return_call_indirect\(' "$f" || true)
done

if [ "$fail" -ne 0 ]; then
  echo "exit-sites: an exit edge bypasses exit_plan.rs — route it through emit_exit(&self.exit_plan(..)) or, for a hand-written helper body, list the file in NOT_FRAMES with its reason"
  exit 1
fi
echo "exit-sites: OK — $checked frame exit(s) in $SRC go through the ExitPlan renderer"
