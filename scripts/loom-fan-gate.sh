#!/usr/bin/env bash
# LOOM OVER THE FAN GATE (security row 6)
# =======================================
#
# scripts/miri-fan-gate.sh runs the native fan gate's unsafe (the
# lifetime-erasing transmute in almide_fan_deal, `unsafe impl Sync for
# AlmideFanGate`, the raw parent pointers) under Miri, which SAMPLES
# schedules: one per seed per preemption rate. An interleaving no sampled
# schedule reaches is unchecked there. This gate ENUMERATES them: loom runs
# the gate protocol's own source (tools/loom-fan-gate: build.rs extracts
# runtime/rs/src/list.rs from `fn almide_fan_deal` to the end of
# `fn almide_fan_steal` and points it at loom's primitives) over every
# interleaving of a caller and helper threads, and refuses any execution in
# which a helper touches an offer's frame without that access
# happening-before the frame's teardown, or a block is not run exactly once.
#
# The models: a flat offer and one helper; a nested offer (an offer inside a
# block, the caller stealing same-tree offers while it waits) and one helper;
# two offers from two threads (two `fan { }` arms) and one helper.
#
#   bash scripts/loom-fan-gate.sh
#
# LOOM_MAX_PREEMPTIONS  overrides every model's preemption bound (unset: the
#                       flat model is unbounded — every interleaving — and the
#                       nested and two-offer models are bounded at 3 and 2;
#                       tools/loom-fan-gate/tests/model.rs says why).
#
# The controls run first. Each breaks the protocol in the extracted source
# and MUST come back red from the model; if one does not, the model cannot
# see that defect and the green run after it would prove nothing:
#   nowait     the caller returns without waiting for the helpers inside its
#              gate (the defect miri-fan-gate.sh's control also plants)
#   noreaders  withdrawing an offer does not wait for the threads reading
#              its slot, so a helper can enter a gate whose caller already
#              stopped counting entrants
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
MANIFEST=tools/loom-fan-gate/Cargo.toml
LOG="$(mktemp -d)"
trap 'rm -rf "$LOG"' EXIT

run() { # <control or empty> <log>
  LOOM_FAN_CONTROL="$1" cargo test --release --locked --manifest-path "$MANIFEST" -- --test-threads=3 >"$2" 2>&1
}

for control in nowait noreaders; do
  start=$(date +%s)
  run "$control" "$LOG/$control.log"
  rc=$?
  if [ $rc -eq 0 ]; then
    echo "::error::loom-fan-gate: the $control control came back green — the model does not see that defect, so the unbroken run would prove nothing"
    tail -40 "$LOG/$control.log"
    exit 1
  fi
  # the violation itself, not any failure: a build error, or a model red for
  # another reason, is not this control's verdict
  if ! grep -q 'Causality violation' "$LOG/$control.log"; then
    echo "::error::loom-fan-gate: the $control control failed (exit $rc) without a loom causality violation — not the defect it plants"
    tail -60 "$LOG/$control.log"
    exit 1
  fi
  echo "loom-fan-gate: control $control red as required ($(( $(date +%s) - start ))s): $(grep -m1 -oE 'Causality violation[^.]*' "$LOG/$control.log")"
done

start=$(date +%s)
run "" "$LOG/model.log"
rc=$?
grep -E '^test .* \.\.\. (ok|FAILED)$' "$LOG/model.log" | sed 's/^/  /'
if [ $rc -ne 0 ]; then
  echo "::error::loom-fan-gate: the fan gate protocol failed under loom (exit $rc)"
  tail -80 "$LOG/model.log"
  exit 1
fi
echo "loom-fan-gate: every enumerated interleaving clean (LOOM_MAX_PREEMPTIONS=${LOOM_MAX_PREEMPTIONS:-per model}, $(( $(date +%s) - start ))s)"
