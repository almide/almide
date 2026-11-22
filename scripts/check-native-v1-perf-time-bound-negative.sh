#!/usr/bin/env bash
# NEGATIVE CONTROL for the time-bound rows of scripts/check-native-v1-perf.sh.
#
# A time-bound program (proofs/native-v1-perf-time-bound.txt) is compared per
# pass of its loop, not by its total: its pass count is whatever fits in its
# wall time. Dividing by a pass count could hide a real loss, so this control
# runs the gate's own measurement (scripts/lib/native-v1-perf-in-main.sh
# per_pass_work, the same syscall-count declaration the datetime spin uses:
# clock_gettime, two reads per pass) over two programs built here:
#
#   base — a spin on datetime.monotonic_ns bounded by datetime.now_ms, the
#          shape of spec/wasm_cross/datetime_monotonic_ns.almd;
#   dear — the same spin with one more step of integer work per pass.
#
# base against itself must read within the gate's 1.03 (the per-pass count is
# stable from run to run though the pass counts differ), and dear against
# base must read over it (a pass made dearer is still a loss). Either way
# round is exit 1.
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE_BIN:-target/release/almide}"
MAX_RATIO="${NATIVE_V1_PERF_MAX_RATIO:-1.03}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
command -v valgrind >/dev/null || { echo "time-bound-negative: valgrind is not installed"; exit 2; }
command -v callgrind_annotate >/dev/null || { echo "time-bound-negative: callgrind_annotate is not installed"; exit 2; }
[ -x "$ALMIDE" ] || { echo "time-bound-negative: no compiler at $ALMIDE"; exit 2; }
source scripts/lib/native-v1-perf-in-main.sh

spin() { # $1 = the extra statement of each pass, $2 = what main prints of it
  cat <<EOF
effect fn main() -> Unit = {
  let a = datetime.monotonic_ns()
  let escape = datetime.now_ms() + 5000
  var reached = false
  var gave_up = false
  var acc = 0
  while not reached and not gave_up {
    let now = datetime.monotonic_ns()
    $1
    reached = now - a >= 20000000
    gave_up = datetime.now_ms() >= escape
  }
  println("reached: \${reached} \${$2}")
}
EOF
}
spin 'acc = acc + 1' 'acc > 0' > "$WORK/base.almd"
spin 'acc = acc + now % 1000003' 'acc >= 0' > "$WORK/dear.almd"
printf 'fn main() -> Unit = ()\n' > "$WORK/empty.almd"
for p in empty base dear; do
  if ! l=$(ALMIDE_VERIFIED_DEBUG=1 "$ALMIDE" build "$WORK/$p.almd" -o "$WORK/$p" 2>&1) \
    || ! grep -q "native: v1 trust-spine render" <<<"$l"; then
    echo "time-bound-negative: $p did not render certified"; echo "$l" | head -5; exit 2
  fi
done
g=$(in_main "$WORK/empty")
[[ "$g" =~ ^[0-9]+$ ]] || { echo "time-bound-negative: the entry toggle collected nothing on the empty program"; exit 2; }
e=$(syscall_calls "$WORK/empty" clock_gettime)

read -r b1 p1 _ <<<"$(per_pass_work "$WORK/base" "$g" clock_gettime 2 "$e")"
read -r b2 p2 _ <<<"$(per_pass_work "$WORK/base" "$g" clock_gettime 2 "$e")"
read -r d1 p3 _ <<<"$(per_pass_work "$WORK/dear" "$g" clock_gettime 2 "$e")"
for v in "${p1:-0}" "${p2:-0}" "${p3:-0}"; do
  awk -v x="$v" 'BEGIN { exit !(x < 100) }' && { echo "time-bound-negative: a run read under 100 passes ($p1 / $p2 / $p3)"; exit 1; }
done
same=$(awk -v a="$b1" -v b="$b2" 'BEGIN { printf "%.3f", b / a }')
dear=$(awk -v a="$b1" -v b="$d1" 'BEGIN { printf "%.3f", b / a }')
echo "base per pass $b1 ($p1 passes), again $b2 ($p2 passes): $same"
echo "dear per pass $d1 ($p3 passes) against base: $dear"
fail=0
if awk -v r="$same" -v m="$MAX_RATIO" 'BEGIN { exit !(r > m || r < 1 / m) }'; then
  echo "time-bound-negative: FAILED — the same binary read $same per pass on two runs (outside $MAX_RATIO)"; fail=1
fi
if ! awk -v r="$dear" -v m="$MAX_RATIO" 'BEGIN { exit !(r > m) }'; then
  echo "time-bound-negative: FAILED — a dearer pass read $dear, not over $MAX_RATIO"; fail=1
fi
[ "$fail" = 0 ] && echo "time-bound-negative: ok (same $same, dear $dear > $MAX_RATIO)"
exit "$fail"
