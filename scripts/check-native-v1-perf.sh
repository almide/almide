#!/usr/bin/env bash
# NATIVE v1-vs-v0 PERF GATE (#4006, the no-perf-loss rule).
#
# The certified native render (v1, MIR → typed Rust) replaces the standard
# codegen (v0) one program at a time, and a program moved onto it must not run
# slower. check-perf-ratio.sh watches a handful of benchmarks against
# handwritten Rust; it never compared the two Almide renders, and missed a
# `list.push` that allocated a one-element Vec per push and a swap that copied
# the whole list twice per round (2.7x).
#
# What it checks: every program on proofs/native-verified-floor.txt, plus the
# curated shape benches in research/benchmark/perf/v1-shapes/, is built twice —
# `almide build` (v1 when the program renders certified) and
# `almide build --no-verified` (v0) — and both binaries run under
# `valgrind --tool=cachegrind --cache-sim=no`. The gated number is the ratio of
# their total instruction counts (I refs): deterministic, and the same on any
# runner (GitHub and GCE e2 machines expose no PMU, so `perf stat` cannot count
# there). A program whose v0 run is under NATIVE_V1_PERF_MIN_IR instructions is
# reported but not gated: process startup dominates it.
#
# Fails when v1/v0 > NATIVE_V1_PERF_MAX_RATIO (default 1.03) on a gated program
# that proofs/native-v1-perf-ledger.txt does not list, when a listed program
# reads above its recorded ratio, and when a listed program reads clean (its
# row must go: the ledger only shrinks). The ledger is a DEBT list — each row
# carries the date it was found and the plan that removes it — not an
# allowance.
#
# Instruction counts miss memory locality (a change can cut instructions and
# run slower), so every gated program at or above NATIVE_V1_PERF_TIME_IR
# (default 20M) instructions is also timed: the min of NATIVE_V1_PERF_RUNS (9)
# interleaved runs of each binary. A wall-time ratio above 1.10 is a WARNING
# in the summary; it never fails the gate (shared runners are noisy).
#
# A shape bench that does not render certified is reported as UNCERTIFIED and
# not compared (both builds would be v0).
#
# Negative control: ALMIDE_NATIVE_DRILL_NO_MOVES=1 in the environment makes
# the v1 render keep its pre-#4006 copies (no in-place push, a clone for every
# last-use Dup/SetLocal); with it the push_swap shape bench must read over the
# ratio and this script exit 1 (proofs/gate-verification.toml); the nightly
# runs that control over the shape benches alone (NATIVE_V1_PERF_ONLY_SHAPES=1).
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE_BIN:-target/release/almide}"
LEDGER="proofs/native-v1-perf-ledger.txt"
MAX_RATIO="${NATIVE_V1_PERF_MAX_RATIO:-1.03}"
MIN_IR="${NATIVE_V1_PERF_MIN_IR:-1000000}"
TIME_IR="${NATIVE_V1_PERF_TIME_IR:-20000000}"
RUNS="${NATIVE_V1_PERF_RUNS:-9}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
command -v valgrind >/dev/null || { echo "native-v1-perf: valgrind is not installed"; exit 2; }
[ -x "$ALMIDE" ] || { echo "native-v1-perf: no compiler at $ALMIDE"; exit 2; }

ir() {
  timeout 900 valgrind --tool=cachegrind --cache-sim=no --cachegrind-out-file=/dev/null "$1" </dev/null 2>&1 >/dev/null \
    | awk '/I *refs:/ { gsub(",", "", $NF); print $NF }'
}
best_time() { # min wall seconds of one run of $1, over the caller's loop
  local t
  t=$( { TIMEFORMAT=%R; time "$1" </dev/null >/dev/null 2>&1; } 2>&1 )
  echo "$t"
}

# NATIVE_V1_PERF_ONLY_SHAPES=1: the shape benches alone (the negative control's run).
programs() {
  [ -n "${NATIVE_V1_PERF_ONLY_SHAPES:-}" ] || grep -E '^spec/' proofs/native-verified-floor.txt
  ls research/benchmark/perf/v1-shapes/*.almd
}

fail=0; gated=0; warns=(); rows=()
declare -A listed
while IFS=$'\t' read -r prog ratio found plan; do
  case "$prog" in ''|'#'*) continue ;; esac
  listed["$prog"]="$ratio"
done < "$LEDGER"

while read -r f; do
  n="$(echo "$f" | tr '/.' '__')"
  if ! log=$(ALMIDE_VERIFIED_DEBUG=1 timeout 600 "$ALMIDE" build "$f" -o "$WORK/$n.v1" 2>&1); then
    echo "BUILD-FAIL v1 $f"; fail=1; continue
  fi
  if ! grep -q "native: v1 trust-spine render" <<<"$log"; then
    rows+=("UNCERTIFIED $f"); continue
  fi
  # A program the standard codegen cannot build (fuel and time programs are
  # built on the trust spine only) has nothing to compare against.
  if ! ALMIDE_NO_VERIFIED_OK=1 timeout 600 "$ALMIDE" build "$f" --no-verified -o "$WORK/$n.v0" >/dev/null 2>&1; then
    rows+=("v1-only $f"); continue
  fi
  a=$(ir "$WORK/$n.v0"); b=$(ir "$WORK/$n.v1")
  if [ -z "$a" ] || [ -z "$b" ]; then echo "RUN-FAIL $f (v0=${a:-none} v1=${b:-none})"; fail=1; continue; fi
  r=$(awk -v a="$a" -v b="$b" 'BEGIN { printf "%.3f", b / a }')
  if [ "$a" -lt "$MIN_IR" ]; then rows+=("small $f v0=$a v1=$b ratio=$r"); continue; fi
  gated=$((gated + 1))
  over=$(awk -v r="$r" -v m="$MAX_RATIO" 'BEGIN { print (r > m) ? 1 : 0 }')
  if [ -n "${listed[$f]:-}" ]; then
    worse=$(awk -v r="$r" -v m="${listed[$f]}" 'BEGIN { print (r > m + 0.005) ? 1 : 0 }')
    if [ "$over" = 0 ]; then echo "CLEAN $f reads $r — remove its row from $LEDGER"; fail=1
    elif [ "$worse" = 1 ]; then echo "WORSE $f reads $r, ledger says ${listed[$f]}"; fail=1
    else rows+=("debt $f ratio=$r (ledger ${listed[$f]})"); fi
  elif [ "$over" = 1 ]; then
    echo "LOSS $f: v1 $b vs v0 $a instructions = ${r}x (> $MAX_RATIO)"; fail=1
  else
    rows+=("ok $f ratio=$r")
  fi
  if [ "$a" -ge "$TIME_IR" ]; then
    t0=9e9; t1=9e9
    for _ in $(seq "$RUNS"); do
      t=$(best_time "$WORK/$n.v0"); t0=$(awk -v x="$t" -v y="$t0" 'BEGIN { print (x < y) ? x : y }')
      t=$(best_time "$WORK/$n.v1"); t1=$(awk -v x="$t" -v y="$t1" 'BEGIN { print (x < y) ? x : y }')
    done
    tr=$(awk -v a="$t0" -v b="$t1" 'BEGIN { if (a > 0) printf "%.3f", b / a; else print "n/a" }')
    rows+=("time $f v0=${t0}s v1=${t1}s ratio=$tr")
    if awk -v r="$tr" 'BEGIN { exit !(r + 0 > 1.10) }'; then warns+=("WARNING wall time $f: v1 ${t1}s vs v0 ${t0}s = ${tr}x"); fi
  fi
done < <(programs)

for r in "${rows[@]}"; do echo "$r"; done
for w in "${warns[@]}"; do echo "$w"; done
if [ -n "${GITHUB_STEP_SUMMARY:-}" ] && [ "${#warns[@]}" -gt 0 ]; then
  printf '%s\n' "${warns[@]}" >> "$GITHUB_STEP_SUMMARY"
fi
if [ -z "${NATIVE_V1_PERF_ONLY_SHAPES:-}" ] && [ "$gated" -lt 10 ]; then echo "native-v1-perf: only $gated gated programs — the scan is blind"; exit 1; fi
if [ "$fail" != 0 ]; then echo "native-v1-perf: FAILED ($gated gated)"; exit 1; fi
echo "native-v1-perf: ok ($gated gated, ${#warns[@]} wall-time warning(s))"
