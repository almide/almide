#!/usr/bin/env bash
# Phase 0 decision gate runner for docs/roadmap/active/v1-mir-architecture.md §8.
#
# Re-verifies, from scratch, that ALL FIVE ownership-tricky shapes pass the gate:
# one Perceus ownership decision (held in a minimal MIR) renders FAITHFULLY to
# both idiomatic Rust AND a reference-counted (wasm-semantics) form, the two
# AGREE by construction, and a renderer that RE-DECIDES ownership is caught.
#
# Each shape is a self-contained meta-harness: it builds the MIR decision,
# renders it two ways, compiles BOTH renderings with `rustc --edition 2021 -O`,
# runs them, compares, and exits 0 only on PASS. No workspace, no deps.
#
# Usage: research/spike/v1-mir/run-gate.sh
set -euo pipefail
cd "$(dirname "$0")"

# Shape #2 (`list.get(xs,i) ?? d`, the #643 core) is the cargo entry point.
# Shapes #1/#3/#4/#5 are standalone files under shapes/.
SHAPE_643="cargo run --quiet"
SHAPES=(alias_return boxed_pattern_610 closure_capture alias_cow)

pass=0
fail=0

echo "==================== shape: list_get_643 (cargo) ===================="
if $SHAPE_643; then pass=$((pass+1)); else fail=$((fail+1)); echo "GATE FAIL: list_get_643"; fi

for key in "${SHAPES[@]}"; do
  echo
  echo "==================== shape: $key ===================="
  bin="$(mktemp -d)/gate_$key"
  rustc --edition 2021 -O "shapes/$key.rs" -o "$bin"
  if "$bin"; then pass=$((pass+1)); else fail=$((fail+1)); echo "GATE FAIL: $key"; fi
done

echo
echo "==================== DECISION GATE TALLY ===================="
echo "  pass=$pass fail=$fail (of 5)"
if [ "$fail" -eq 0 ]; then
  echo "  VERDICT: PASS — RC and Rust move/borrow share one canonical form across all 5 shapes"
  exit 0
else
  echo "  VERDICT: FAIL — see §8 decision-gate failure branch"
  exit 1
fi
