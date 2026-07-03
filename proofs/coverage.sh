#!/usr/bin/env bash
# COMPILER STRUCTURAL COVERAGE (flight-evidence-gaps F2-1): measure which lines
# of the TRUST-SPINE crates (almide-mir) the verification suites actually
# execute. This is the DIRECT LOOK the evidence ladder was missing: a green gate
# says nothing about code the gate never runs (the 2026-07-03 match-linearization
# lived exactly in such a hole). Statement coverage is the DO-178C entry rung —
# MC/DC is the DAL-A rung; this script establishes the measurement, not a target.
#
#   bash proofs/coverage.sh          # measure + print the summary table
#
# Scope note: this instruments `cargo test -p almide-mir` (unit + gate tests) AND
# a render_program sweep over spec/wasm_cross (the parity workload). The v0
# compiler crates (almide-codegen etc.) are the production path — measured
# separately once this rung is stable (they need the wasm/native e2e harness
# under instrumentation, a heavier build).
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

command -v cargo-llvm-cov >/dev/null || {
    echo "coverage: cargo-llvm-cov not installed (cargo install cargo-llvm-cov) — SKIP"
    exit 0
}
cd "$ROOT"

echo "== instrumented run: cargo test -p almide-mir + render_program sweep =="
cargo llvm-cov clean --workspace >/dev/null 2>&1 || true
# 1) the unit/gate tests
cargo llvm-cov --no-report -p almide-mir --release >/dev/null 2>&1 || true
# 2) the parity workload: render every wasm_cross fixture through the instrumented
#    render_program (walls included — a wall exercises the admission gates).
cargo llvm-cov run --no-report --release -p almide-mir --example render_program -- /dev/null >/dev/null 2>&1 || true
RP_COV="$ROOT/target/llvm-cov-target/release/examples/render_program"
if [ -x "$RP_COV" ]; then
    n=0
    for f in spec/wasm_cross/*.almd; do
        LLVM_PROFILE_FILE="$ROOT/target/llvm-cov-target/rp-%m-%p.profraw" "$RP_COV" "$f" >/dev/null 2>&1 || true
        n=$((n+1))
    done
    echo "  render_program sweep: $n fixtures"
fi
echo
echo "== almide-mir line coverage (verification suites + parity workload) =="
cargo llvm-cov report --release 2>/dev/null | awk 'NR<=2 || /almide-mir/ || /^TOTAL/' | head -40
