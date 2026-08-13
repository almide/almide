#!/usr/bin/env bash
# COMPILER STRUCTURAL COVERAGE (flight-evidence-gaps F2-1): measure which lines
# of the TRUST-SPINE crates (almide-mir) the verification suites actually
# execute. This is the DIRECT LOOK the evidence ladder was missing: a green gate
# says nothing about code the gate never runs (the 2026-07-03 match-linearization
# lived exactly in such a hole). Statement coverage is the DO-178C entry rung —
# MC/DC is the DAL-A rung; this script establishes the measurement, not a target.
#
#   bash proofs/coverage.sh            # measure + enforce the ratchet (= --check)
#   bash proofs/coverage.sh --check    # same, explicit (what CI passes)
#   bash proofs/coverage.sh --update   # additionally RAISE the baseline on gain
#
# Scope note: this instruments `cargo test -p almide-mir` (unit + gate tests) AND
# a render_program sweep over spec/wasm_cross (the parity workload). The v0
# compiler crates (almide-codegen etc.) are the production path — measured
# separately once this rung is stable (they need the wasm/native e2e harness
# under instrumentation, a heavier build).
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Parse the mode for real (#990: `--check` was accepted and ignored — the
# ratchet happened to run by default, but an unknown flag should be an error,
# not a silently-absorbed fiction).
MODE="${1:---check}"
case "$MODE" in
  --check|--update) ;;
  *) echo "usage: coverage.sh [--check|--update]" >&2; exit 2 ;;
esac

# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

# Resolve llvm-tools from the ACTIVE toolchain's sysroot, not a $HOME glob
# (#990: the `stable-*` glob depends on the runner image's default toolchain
# NAME — one image change and the 58% ratchet silently stops measuring).
SYSROOT="$(rustc --print sysroot)"
LLVM_BIN="$(echo "$SYSROOT"/lib/rustlib/*/bin | awk '{print $1}')"
if [ ! -x "$LLVM_BIN/llvm-profdata" ]; then
    if [ "${CI:-}" = "true" ]; then
        echo "::error::coverage: llvm-tools not found under $SYSROOT — in CI a missing tool is a failure (#990); rustup component add llvm-tools-preview"
        exit 1
    fi
    echo "coverage: llvm-tools not installed (rustup component add llvm-tools-preview) — SKIP"
    exit 0
fi
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
# `-perm -u+x`, not `-perm /111`: the `/` form is a GNU extension and BSD find
# (macOS) rejects it outright — the second call has no `|| true`, so under
# `set -e` this whole gate died at step 2/4 with "illegal mode string" on every
# non-GNU host, never reaching the ratchet it exists to enforce (#1244 round 5).
TESTBINS="$(find "$COVDIR/t/release/deps" -maxdepth 1 -type f -perm -u+x ! -name '*.d' ! -name '*.dylib' | grep -E '/(almide_mir|almide_codegen|integration|lower|render)[^/]*$' || true)"
[ -n "$TESTBINS" ] || TESTBINS="$(find "$COVDIR/t/release/deps" -maxdepth 1 -type f -perm -u+x ! -name '*.d' ! -name '*.dylib')"
# No vacuous measurement: zero test binaries would still produce profraw from
# the step-3 workloads, so the run would report a NUMBER for a suite that never
# ran. That is the #990 failure mode again — fail instead.
[ -n "$TESTBINS" ] || { echo "coverage: NO test binaries found under $COVDIR/t/release/deps — the discovery went blind"; exit 1; }
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
REPORT="$("$LLVM_BIN/llvm-cov" report $OBJS \
    -instr-profile="$COVDIR/all.profdata" \
    -ignore-filename-regex='(\.cargo|rustc|/tests?/|tests_part|examples/)' 2>/dev/null \
  | awk 'NR<=2 || /almide-(mir|codegen|frontend)\// || /^TOTAL/' | grep -vE 'tests?_part')"
printf '%s\n' "$REPORT" | tail -40

# ── RATCHET (#566): TOTAL line coverage may only go UP ─────────────────────
# Baseline file holds one number: the floor (integer percent ×100 to avoid
# float compare, e.g. 6589 = 65.89%). `--check` fails when the measured TOTAL
# drops below it; `--update` raises it to the measured value (never lowers).
BASELINE_FILE="$ROOT/proofs/coverage-baseline.txt"
total_line_pct="$(printf '%s\n' "$REPORT" | awk '/^TOTAL/ { for (i=1;i<=NF;i++) if ($i ~ /%$/) last=$i } END { gsub(/%/,"",last); print last }')"
total_c="$(printf '%s\n' "$total_line_pct" | awk '{ printf "%d", $1 * 100 }')"
echo
echo "TOTAL line coverage: ${total_line_pct}%"
if [ -f "$BASELINE_FILE" ]; then
    floor="$(cat "$BASELINE_FILE")"
    if [ "$total_c" -lt "$floor" ]; then
        echo "COVERAGE RATCHET FAIL: TOTAL ${total_line_pct}% < baseline $(awk -v f="$floor" 'BEGIN{printf "%.2f", f/100}')%"
        echo "  New code is landing untested. Add tests, or (only with a recorded"
        echo "  justification) lower proofs/coverage-baseline.txt in its own commit."
        exit 1
    fi
    echo "coverage ratchet OK: ${total_line_pct}% >= floor $(awk -v f="$floor" 'BEGIN{printf "%.2f", f/100}')%"
    if [ "$MODE" = "--update" ] && [ "$total_c" -gt "$floor" ]; then
        echo "$total_c" > "$BASELINE_FILE"
        echo "coverage ratchet RAISED to ${total_line_pct}%"
    fi
else
    echo "$total_c" > "$BASELINE_FILE"
    echo "coverage ratchet SEEDED at ${total_line_pct}%"
fi
