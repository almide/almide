#!/usr/bin/env bash
# NATIVE v1 PERF GATE, SMALL PROGRAMS (#4006) — the second check behind
# scripts/check-native-v1-perf.sh's "a v1 loss in a program under 1M
# instructions" blind spot.
#
# The perf gate compares whole-run instruction counts and leaves a program
# whose v0 run is under 1M instructions ungated: process startup (the dynamic
# loader, relocation, the stack guard's map read) dominates it, and differs
# between any two binaries by thousands of instructions. Most certified
# programs are that small, and the shape a small program runs once is the
# shape a large one runs in a loop.
#
# What it checks: every program on proofs/native-verified-floor.txt, and
# every shape bench of research/benchmark/perf/v1-shapes/, whose v0 run is
# under NATIVE_V1_PERF_MIN_IR (1M) instructions — the rest are the perf
# gate's — is built both ways and run under
# `valgrind --tool=callgrind --collect-atstart=no` with collection toggled on
# the binary's `main` (the generated crate's, `almide_*::main`) and on every
# thread's start (a fan arm runs on a thread of its own): no loader, no
# runtime start-up before `main`, no exit after it. The stack-guard install
# that both legs' `main` runs reads the process's memory map, whose size
# differs between any two binaries, so collection is toggled off across it
# (both legs keep it out of line). The rest of each leg's `main` glue is the
# same code whatever the program (on v0 the SIGPIPE reset, the panic hook and
# the final flush), so each leg's count for a program whose `main` does
# nothing is taken off: what remains is the program's own work, exactly. The
# measurement is deterministic because valgrind schedules threads round-robin
# (`--fair-sched=yes`): under its default lock a fan program's waits spin a
# schedule-dependent number of times (fan_trap_waits_for_elements_below read
# 47136 to 58161 on v0 across runs; with the round-robin schedule, 59677 on
# every run). The
# gated number is the ratio of the two works against NATIVE_V1_PERF_MAX_RATIO
# (1.03), on every program — there is no size floor.
# A loss fails unless proofs/native-v1-perf-ledger.txt lists the program,
# under the perf gate's rules (a listed program reading clean, or worse than
# its row, fails too).
#
# Negative control: ALMIDE_NATIVE_DRILL_NO_MOVES=1 makes the v1 render keep
# its pre-#4006 copies; with it this gate must exit 1 (the nightly runs it).
# ALMIDE_NATIVE_DRILL_COND_I64=1 makes it spell every comparison as an i64
# 0/1 tested with `!= 0` again (crates/almide-mir/src/render_native_cond.rs);
# with it this gate must read recursion_depth_within_limits as a LOSS.
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE_BIN:-target/release/almide}"
LEDGER="proofs/native-v1-perf-ledger.txt"
MAX_RATIO="${NATIVE_V1_PERF_MAX_RATIO:-1.03}"
MIN_IR="${NATIVE_V1_PERF_MIN_IR:-1000000}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
command -v valgrind >/dev/null || { echo "native-v1-perf-small: valgrind is not installed"; exit 2; }
command -v callgrind_annotate >/dev/null || { echo "native-v1-perf-small: callgrind_annotate is not installed"; exit 2; }
[ -x "$ALMIDE" ] || { echo "native-v1-perf-small: no compiler at $ALMIDE"; exit 2; }

ir() {
  timeout 900 valgrind --tool=cachegrind --fair-sched=yes --cache-sim=no --cachegrind-out-file=/dev/null "$1" </dev/null 2>&1 >/dev/null \
    | awk '/I *refs:/ { gsub(",", "", $NF); print $NF }'
}
# Instructions executed inside the binary's `main` and every thread's start.
source scripts/lib/native-v1-perf-in-main.sh
ratio() { awk -v a="$1" -v b="$2" 'BEGIN { if (a > 0) printf "%.3f", b / a; else print "inf" }'; }

# Each leg's fixed glue: `main` of a program that does nothing.
printf 'fn main() -> Unit = ()\n' > "$WORK/empty.almd"
if ! log=$(ALMIDE_VERIFIED_DEBUG=1 "$ALMIDE" build "$WORK/empty.almd" -o "$WORK/empty.v1" 2>&1) \
  || ! grep -q "native: v1 trust-spine render" <<<"$log"; then
  echo "native-v1-perf-small: the empty program did not render certified"; exit 2
fi
ALMIDE_NO_VERIFIED_OK=1 "$ALMIDE" build "$WORK/empty.almd" --no-verified -o "$WORK/empty.v0" >/dev/null 2>&1 \
  || { echo "native-v1-perf-small: the empty program did not build on v0"; exit 2; }
g0=$(in_main "$WORK/empty.v0"); g1=$(in_main "$WORK/empty.v1")
if [ -z "$g0" ] || [ -z "$g1" ] || [ "$g0" -le 0 ] || [ "$g1" -le 0 ]; then
  echo "native-v1-perf-small: the entry toggle collected nothing on the empty program (v0=${g0:-none} v1=${g1:-none})"; exit 2
fi
echo "glue v0=$g0 v1=$g1"

declare -A listed
while IFS=$'\t' read -r prog r _found _plan; do
  case "$prog" in ''|'#'*) continue ;; esac
  listed["$prog"]="$r"
done < "$LEDGER"

declare -A time_bound
while IFS=$'\t' read -r prog sc _reads _why; do
  case "$prog" in ''|'#'*) continue ;; esac
  time_bound["$prog"]="$sc"
done < proofs/native-v1-perf-time-bound.txt

fail=0; gated=0; rows=()
while read -r f; do
  n="$(echo "$f" | tr '/.' '__')"
  if ! log=$(ALMIDE_VERIFIED_DEBUG=1 timeout 600 "$ALMIDE" build "$f" -o "$WORK/$n.v1" 2>&1); then
    echo "BUILD-FAIL v1 $f"; fail=1; continue
  fi
  grep -q "native: v1 trust-spine render" <<<"$log" || { rows+=("UNCERTIFIED $f"); continue; }
  if ! ALMIDE_NO_VERIFIED_OK=1 timeout 600 "$ALMIDE" build "$f" --no-verified -o "$WORK/$n.v0" >/dev/null 2>&1; then
    rows+=("v1-only $f"); continue
  fi
  a=$(ir "$WORK/$n.v0")
  [ -n "$a" ] || { echo "RUN-FAIL $f (v0)"; fail=1; continue; }
  # The large programs are check-native-v1-perf.sh's, and so is every
  # time-bound program whatever its size (it runs as many passes as fit in
  # its wall time; that gate counts it per pass).
  [ "$a" -ge "$MIN_IR" ] && continue
  [ -n "${time_bound[$f]:-}" ] && continue
  m0=$(in_main "$WORK/$n.v0"); m1=$(in_main "$WORK/$n.v1")
  # callgrind prints `.` for a count of zero: `main` was never entered.
  if ! [[ "$m0" =~ ^[0-9]+$ ]] || ! [[ "$m1" =~ ^[0-9]+$ ]]; then
    echo "MEASURE-FAIL $f (v0=${m0:-none} v1=${m1:-none}): \`main\` was never entered"; fail=1; continue
  fi
  w0=$((m0 - g0)); w1=$((m1 - g1))
  # A program that does less than its leg's empty `main` (an early abort)
  # counts as one instruction of work, so the ratio stays defined.
  [ "$w0" -ge 1 ] || w0=1
  [ "$w1" -ge 1 ] || w1=1
  gated=$((gated + 1))
  r=$(ratio "$w0" "$w1")
  over=$(awk -v r="$r" -v m="$MAX_RATIO" 'BEGIN { print (r > m) ? 1 : 0 }')
  if [ -n "${listed[$f]:-}" ]; then
    worse=$(awk -v r="$r" -v m="${listed[$f]}" 'BEGIN { print (r > m + 0.005) ? 1 : 0 }')
    if [ "$over" = 0 ]; then echo "CLEAN $f reads $r — remove its row from $LEDGER"; fail=1
    elif [ "$worse" = 1 ]; then echo "WORSE $f reads $r, ledger says ${listed[$f]}"; fail=1
    else rows+=("debt $f work ratio=$r (ledger ${listed[$f]})"); fi
  elif [ "$over" = 1 ]; then
    echo "LOSS $f: v1 $w1 vs v0 $w0 program instructions = ${r}x (> $MAX_RATIO)"; fail=1
  else
    rows+=("ok $f work v0=$w0 v1=$w1 ratio=$r")
  fi
done < <(source scripts/lib/native-v1-perf-corpus.sh; native_v1_perf_programs)

for r in "${rows[@]}"; do echo "$r"; done
# Most of the floor is small; fewer than 20 gated means the scan broke.
if [ "$gated" -lt 20 ]; then echo "native-v1-perf-small: only $gated gated programs — the scan is blind"; exit 1; fi
if [ "$fail" != 0 ]; then echo "native-v1-perf-small: FAILED ($gated gated)"; exit 1; fi
echo "native-v1-perf-small: ok ($gated gated)"
