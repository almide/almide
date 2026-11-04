#!/usr/bin/env bash
# COMPILE-TIME RATIO RATCHET (#4289, #4317).
#
# The integration batch #4276 made compiling an ordinary program 25-50%
# slower and nothing noticed until the release CI legs timed out: the
# edit-loop gate (check-edit-loop-scale.sh) imports the whole stdlib on
# purpose, so it cannot see a program LOADING more of it, and every other
# perf gate times the generated code, not the compiler. Two causes are named
# in #4289: a reach edge that loaded bytes, int, float, math and five
# collection modules into every program that called one string function, and
# a link scan that cloned every subtree once per ancestor.
#
# What it measures, on a fixed corpus of small programs (the ones a CI test
# compiles thousands of times): each file's front end through
# `almide check --timings`, which reports, in the same process, the time spent
# lexing + parsing every source it loaded, the bytes it read and the total.
# The unit is the run's own parse speed: `ref`, the time this machine takes to
# lex + parse REF_BYTES (1 MB) at the speed the corpus read its sources
# (sum(lex + parse) / sum(bytes)). Three same-run ratios are gated, so the
# machine cancels the way check-perf-ratio.sh cancels it:
#
#   check      sum(total) / ref over the corpus: what the checker, the
#              lowering and the check-time gates cost. A reach blow-up raises
#              it (the extra modules are read, checked AND lowered); so does
#              any per-module or per-node cost that grows faster than the
#              source.
#   wasm       sum(wall time of `almide build --target wasm`) / ref: the same,
#              through the structural wasm leg's link, mono and emit.
#   jsonround  the wall time of `almide build --target wasm` of
#              research/benchmark/perf/jsonround/jsonround.almd / ref: the
#              program #4458 measured at 308 ms (3.3x a peer compiler's 92 ms)
#              and brought under it — a JSON round trip links the most stdlib
#              of any small program.
#
# Until #4458 the unit was the corpus's own sum(lex + parse). That stopped
# measuring the machine once a program no longer parsed the bundled stdlib it
# does not reach (#4458 parses a module whole only when something reaches it):
# the denominator fell 3.4x while the compiler got faster, and every ratio
# rose. Parse SPEED (time per byte read) is what the old unit stood for.
#
# Each file is measured RUNS times interleaved and the MIN taken (the least-
# contaminated draw). Budget: COMPILE_RATIO_BUDGET_PCT (default 20) above the
# committed baseline; like check-perf-ratio.sh the gate also fails below 50%
# of it (a ratio that halves means the measurement broke, or the baseline
# should be lowered in the same change).
#
# See scripts/compile-time-ratio-baseline.txt for the anchors and what the
# trees before and after each fix read.
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE_BIN:-almide}"
BASELINE_FILE="scripts/compile-time-ratio-baseline.txt"
BUDGET_PCT="${COMPILE_RATIO_BUDGET_PCT:-20}"
RUNS="${COMPILE_RATIO_RUNS:-3}"
CORPUS=(
  examples/pid-kernel.almd
  spec/wasm_cross/codec_decode_field_order.almd
  spec/wasm_cross/codec_extra_keys.almd
  spec/wasm_cross/codec_int_float_boundaries.almd
  spec/wasm_cross/codec_list_default_field.almd
  spec/wasm_cross/codec_triple_list.almd
  spec/wasm_cross/effect_tail_carrier_family.almd
  spec/wasm_cross/effect_tco_err_rewrap.almd
  spec/wasm_cross/effect_unwrap_in_loop.almd
  spec/wasm_cross/gleam_records_tuples.almd
  spec/wasm_cross/koka_rbtree_balance.almd
  spec/wasm_cross/leak_free_spread_produced_base.almd
  spec/wasm_cross/list_update_callback_result_ownership.almd
  spec/wasm_cross/match_box_pattern_backstop.almd
  spec/wasm_cross/mutual_tail_recursion.almd
  spec/wasm_cross/nested_payload_guard.almd
  spec/wasm_cross/ord_recursive.almd
  spec/wasm_cross/recursive_payload_closure_capture.almd
  spec/wasm_cross/string_count_truncation.almd
  spec/wasm_cross/string_repeat_empty.almd
  spec/wasm_cross/tm_list_opt_fn_pass.almd
  spec/wasm_cross/tm_tree_fn_pass.almd
  spec/wasm_cross/whilep_str.almd
  spec/wasm_cross/wide_function_body.almd
)

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

JSONROUND=research/benchmark/perf/jsonround/jsonround.almd
REF_BYTES=1000000

now_ns() { date +%s%N; }
wasm_build_ns() {
  local t0 t1
  t0=$(now_ns)
  "$ALMIDE" build "$1" --target wasm -o "$scratch/o.wasm" >/dev/null 2>&1 \
    || { echo "FAIL: almide build --target wasm $1" >&2; exit 1; }
  t1=$(now_ns)
  echo $((t1 - t0))
}
sum_front=0; sum_total=0; sum_wasm=0; sum_bytes=0
for f in "${CORPUS[@]}"; do
  best_total=""; best_front=""; best_bytes=""; best_wasm=""
  for _ in $(seq 1 "$RUNS"); do
    line=$("$ALMIDE" check --timings "$f" 2>&1 | grep '^almide-timings ' || true)
    [ -n "$line" ] || { echo "FAIL: no almide-timings line from 'almide check --timings $f'"; exit 1; }
    json=${line#almide-timings }
    read -r total front bytes < <(printf '%s' "$json" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["total_ns"], d["lex_ns"] + d["parse_ns"], d["bytes"])')
    wasm=$(wasm_build_ns "$f")
    if [ -z "$best_total" ] || [ "$total" -lt "$best_total" ]; then best_total=$total; best_front=$front; best_bytes=$bytes; fi
    if [ -z "$best_wasm" ] || [ "$wasm" -lt "$best_wasm" ]; then best_wasm=$wasm; fi
  done
  sum_front=$((sum_front + best_front)); sum_total=$((sum_total + best_total)); sum_wasm=$((sum_wasm + best_wasm))
  sum_bytes=$((sum_bytes + best_bytes))
done
best_json=""
for _ in $(seq 1 "$RUNS"); do
  t=$(wasm_build_ns "$JSONROUND")
  if [ -z "$best_json" ] || [ "$t" -lt "$best_json" ]; then best_json=$t; fi
done

read -r ref_ns check_ratio wasm_ratio json_ratio < <(python3 -c "
ref = $sum_front / $sum_bytes * $REF_BYTES
print('%d %.3f %.3f %.3f' % (ref, $sum_total / ref, $sum_wasm / ref, $best_json / ref))")
echo "compile-time ratio over ${#CORPUS[@]} files (min of $RUNS): lex+parse $((sum_front / 1000000))ms over $((sum_bytes / 1000))KB (1 MB at that speed: $((ref_ns / 1000000))ms), check total $((sum_total / 1000000))ms, wasm build $((sum_wasm / 1000000))ms, jsonround wasm build $((best_json / 1000000))ms"
echo "  check = $check_ratio   wasm = $wasm_ratio   jsonround = $json_ratio"

# The band, one place: ok | high | low for a reading against its anchor.
verdict_of() {
  python3 -c "
a, g, b = $1, $2, $BUDGET_PCT
print('high' if g > a * (1 + b / 100) else 'low' if g < a * 0.5 else 'ok')"
}

# Recorded readings of the trees whose compile slowdown this gate exists for,
# as `row reading label`: the band must call each one high.
#   #4289: the post-batch release-fix tree, against anchors near 4.6 / 20.6.
#   #4317: #4182's merge and develop before the fix, whose wasm link scanned
#   every fn of every module it loaded — `int.to_string` linked 63 self-host
#   bodies into a three-function program and the corpus wasm gates ran ~3x
#   longer per fixture. Their check readings (4.66, 4.18) were in the check
#   row's band then: the front end's whole-module reach is not what #4317
#   fixed.
#   #4328: develop before the resolver kept only the reached functions of a
#   bundled module — every function of every loaded module was checked and
#   lowered. Measured on the same pinned GCE core as the fixed tree.
#   #4458: develop before the declaration index, the whitelist-bounded link
#   and the single resolution (942e7b6c0), read in the parse-speed unit; the
#   older trees above were slower still and were read in the old unit (see
#   the baseline file), so this tree is the one the band must call high now.
REGRESSED=(
  "check 27.0 develop-before(942e7b6c0,#4458)"
  "wasm 50.6 develop-before(942e7b6c0,#4458)"
  "jsonround 5.41 develop-before(942e7b6c0,#4458)"
)

status=0
for row in check wasm jsonround; do
  anchor=$(awk -v r="$row" '$1 == r { print $2 }' "$BASELINE_FILE")
  [ -n "$anchor" ] || { echo "FAIL: no '$row' anchor in $BASELINE_FILE"; exit 1; }
  # Negative control, run on every invocation: the band must call every
  # recorded reading of a tree this gate exists for high (REGRESSED below),
  # and a reading at 40% of the anchor low. A band that stops biting fails
  # here instead of reading green forever.
  for entry in "${REGRESSED[@]}"; do
    read -r r forged_high label <<<"$entry"
    [ "$r" = "$row" ] || continue
    [ "$(verdict_of "$anchor" "$forged_high")" = high ] \
      || { echo "FAIL: negative control — the $row band reads $label's $forged_high as not high against $anchor"; exit 1; }
  done
  forged_low=$(python3 -c "print($anchor * 0.4)")
  [ "$(verdict_of "$anchor" "$forged_low")" = low ] \
    || { echo "FAIL: negative control — the $row band reads $forged_low as not low against $anchor"; exit 1; }
  case $row in check) got=$check_ratio;; wasm) got=$wasm_ratio;; jsonround) got=$json_ratio;; esac
  verdict=$(verdict_of "$anchor" "$got")
  case $verdict in
    ok) echo "  ok   $row $got (anchor $anchor, ceiling +$BUDGET_PCT%)";;
    high) echo "  FAIL $row $got is more than $BUDGET_PCT% above its anchor $anchor: compiling a small program got slower relative to this machine's parse speed (#4289, #4317, #4458)"; status=1;;
    low) echo "  FAIL $row $got is below half its anchor $anchor: the measurement broke, or a durable win should lower the anchor in the same change"; status=1;;
  esac
done
exit $status
