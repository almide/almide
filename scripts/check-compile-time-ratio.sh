#!/usr/bin/env bash
# COMPILE-TIME RATIO RATCHET (#4289).
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
# lexing + parsing every source it loaded and the total. Two same-run ratios
# are gated, so the machine cancels the way check-perf-ratio.sh cancels it:
#
#   check   sum(total) / sum(lex + parse) over the corpus: what the checker,
#           the lowering and the check-time gates cost per unit of source
#           read. A reach blow-up raises it (the extra modules are checked AND
#           lowered, while their lexing is a smaller share); so does any
#           per-module or per-node cost that grows faster than the source.
#   wasm    sum(wall time of `almide build --target wasm`) / sum(lex + parse):
#           the same, through the structural wasm leg's link, mono and emit.
#
# Each file is measured RUNS times interleaved and the MIN taken (the least-
# contaminated draw). Budget: COMPILE_RATIO_BUDGET_PCT (default 20) above the
# committed baseline; like check-perf-ratio.sh the gate also fails below 50%
# of it (a ratio that halves means the measurement broke, or the baseline
# should be lowered in the same change).
#
# Measured 2026-10-28 on a GCE n2-standard-16 (one VM, same corpus, release
# builds): see scripts/compile-time-ratio-baseline.txt for the anchors and
# what the pre-batch, post-batch and fixed trees read.
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

now_ns() { date +%s%N; }
sum_front=0; sum_total=0; sum_wasm=0
for f in "${CORPUS[@]}"; do
  best_total=""; best_front=""; best_wasm=""
  for _ in $(seq 1 "$RUNS"); do
    line=$("$ALMIDE" check --timings "$f" 2>&1 | grep '^almide-timings ' || true)
    [ -n "$line" ] || { echo "FAIL: no almide-timings line from 'almide check --timings $f'"; exit 1; }
    json=${line#almide-timings }
    total=$(printf '%s' "$json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["total_ns"])')
    front=$(printf '%s' "$json" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["lex_ns"] + d["parse_ns"])')
    t0=$(now_ns)
    "$ALMIDE" build "$f" --target wasm -o "$scratch/o.wasm" >/dev/null 2>&1 \
      || { echo "FAIL: almide build --target wasm $f"; exit 1; }
    t1=$(now_ns)
    wasm=$((t1 - t0))
    if [ -z "$best_total" ] || [ "$total" -lt "$best_total" ]; then best_total=$total; best_front=$front; fi
    if [ -z "$best_wasm" ] || [ "$wasm" -lt "$best_wasm" ]; then best_wasm=$wasm; fi
  done
  sum_front=$((sum_front + best_front)); sum_total=$((sum_total + best_total)); sum_wasm=$((sum_wasm + best_wasm))
done

read -r check_ratio wasm_ratio < <(python3 -c "print('%.3f %.3f' % ($sum_total / $sum_front, $sum_wasm / $sum_front))")
echo "compile-time ratio over ${#CORPUS[@]} files (min of $RUNS): lex+parse $((sum_front / 1000000))ms, check total $((sum_total / 1000000))ms, wasm build $((sum_wasm / 1000000))ms"
echo "  check = $check_ratio   wasm = $wasm_ratio"

status=0
for row in check wasm; do
  anchor=$(awk -v r="$row" '$1 == r { print $2 }' "$BASELINE_FILE")
  [ -n "$anchor" ] || { echo "FAIL: no '$row' anchor in $BASELINE_FILE"; exit 1; }
  got=$([ "$row" = check ] && echo "$check_ratio" || echo "$wasm_ratio")
  verdict=$(python3 -c "
a, g, b = $anchor, $got, $BUDGET_PCT
print('high' if g > a * (1 + b / 100) else 'low' if g < a * 0.5 else 'ok')")
  case $verdict in
    ok) echo "  ok   $row $got (anchor $anchor, ceiling +$BUDGET_PCT%)";;
    high) echo "  FAIL $row $got is more than $BUDGET_PCT% above its anchor $anchor: compiling a small program got slower relative to reading it (#4289)"; status=1;;
    low) echo "  FAIL $row $got is below half its anchor $anchor: the measurement broke, or a durable win should lower the anchor in the same change"; status=1;;
  esac
done
exit $status
