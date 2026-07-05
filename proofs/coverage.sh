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

LLVM_BIN="$(echo "$HOME"/.rustup/toolchains/stable-*/lib/rustlib/*/bin | awk '{print $1}')"
[ -x "$LLVM_BIN/llvm-profdata" ] || {
    echo "coverage: llvm-tools not installed (rustup component add llvm-tools-preview) — SKIP"
    exit 0
}
cd "$ROOT"

# MANUAL llvm-cov pipeline — cargo-llvm-cov's multi-run orchestration silently
# measured the WRONG binary twice (0.00% over 4 stray files reported as data,
# 2026-07-03), so each step here is explicit and its artifact is checked.
COVDIR="$ROOT/target/coverage"
rm -rf "$COVDIR"; mkdir -p "$COVDIR"
export RUSTFLAGS="-C instrument-coverage"

echo "== 1/4 instrumented build (almide-mir + almide-codegen tests, render_program, the almide CLI) =="
cargo test -p almide-mir -p almide-codegen --release --no-run --target-dir "$COVDIR/t" 2>&1 | tail -1
cargo build --release -p almide-mir --example render_program --target-dir "$COVDIR/t" 2>&1 | tail -1
cargo build --release --bin almide --target-dir "$COVDIR/t" 2>&1 | tail -1

echo "== 2/4 run the test suites =="
TESTBINS="$(find "$COVDIR/t/release/deps" -maxdepth 1 -type f -perm +111 ! -name '*.d' ! -name '*.dylib' | grep -E '/(almide_mir|almide_codegen|integration|lower|render)[^/]*$' || true)"
[ -n "$TESTBINS" ] || TESTBINS="$(find "$COVDIR/t/release/deps" -maxdepth 1 -type f -perm +111 ! -name '*.d' ! -name '*.dylib')"
i=0
for tb in $TESTBINS; do
    i=$((i+1))
    LLVM_PROFILE_FILE="$COVDIR/test-$i-%m.profraw" "$tb" >/dev/null 2>&1 || true
done
echo "  test binaries run: $i"

echo "== 3/4 workloads: render_program over ALL runnable spec + the v0 CLI over spec =="
RP="$COVDIR/t/release/examples/render_program"
CLI="$COVDIR/t/release/almide"
n=0
for f in $(find spec -name '*.almd' | LC_ALL=C sort); do
    grep -q 'fn main' "$f" || continue
    LLVM_PROFILE_FILE="$COVDIR/rp-%m.profraw" "$RP" "$f" >/dev/null 2>&1 || true
    n=$((n+1))
done
echo "  fixtures rendered (v1 path): $n"
# The v0 PRODUCTION path (almide-codegen walker/emit): `almide test` compiles +
# runs every test-block file through the full frontend→codegen pipeline.
LLVM_PROFILE_FILE="$COVDIR/cli-%m-%p.profraw" "$CLI" test spec/ >/dev/null 2>&1 || true
echo "  v0 CLI: almide test spec/ (frontend + codegen production path)"

echo "== 4/4 merge + report (compiler crate lines) =="
nprof="$(ls "$COVDIR"/*.profraw 2>/dev/null | wc -l | tr -d ' ')"
[ "$nprof" -gt 0 ] || { echo "coverage: NO profraw produced — measurement failed"; exit 1; }
"$LLVM_BIN/llvm-profdata" merge -sparse "$COVDIR"/*.profraw -o "$COVDIR/all.profdata"
OBJS="-object $RP -object $CLI"
for tb in $TESTBINS; do OBJS="$OBJS -object $tb"; done
"$LLVM_BIN/llvm-cov" report $OBJS \
    -instr-profile="$COVDIR/all.profdata" \
    -ignore-filename-regex='(\.cargo|rustc|/tests?/|tests_part|examples/)' 2>/dev/null \
  | awk 'NR<=2 || /almide-(mir|codegen|frontend)\// || /^TOTAL/' | grep -vE 'tests?_part' \
  | tail -40
