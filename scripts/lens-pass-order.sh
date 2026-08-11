#!/usr/bin/env bash
# #912 pass-ordering lens: run the spec suite with each OPTIONAL nanopass
# skipped (ALMIDE_SKIP_PASS), one at a time.
#
# Verdicts per pass:
#   CLEAN   — suite fully green without the pass: it is a pure optimization,
#             nothing downstream silently depends on it.
#   LOUD    — dep-edge panic or compile errors: the dependency is declared or
#             surfaces at build time. Not a finding; the system refused.
#   FINDING — tests fail BY VALUE with the pass skipped: something downstream
#             silently depends on the pass's rewrite for CORRECTNESS. File it.
#
# Record the round's verdict table (including all-CLEAN) as a comment on #912.
set -u
BIN=${ALMIDE_BIN:-./target/release/almide}
SUITE=${LENS_SUITE:-spec/}
OPTIONAL=${LENS_PASSES:-"LICM EggSaturation ConstFold Peephole MatrixShapeSpec AutoParallel TailCallOpt SharedCellBorrow"}
out_dir=$(mktemp -d)
echo "lens-pass-order: suite=$SUITE bin=$BIN logs=$out_dir"
overall=0
for p in $OPTIONAL; do
  log="$out_dir/skip_$p.log"
  # A LOUD pass (per-file dep-panics) makes the suite CRAWL — round 2's
  # EggSaturation leg ground for 2h before the verdict was read off the log.
  # The verdict needs only the SIGNATURE, not a finished suite: cap the leg
  # and classify from the log (a capped leg without the loud signature is
  # reported for a manual rerun, never silently classified). Pure-bash
  # watchdog — macOS ships no `timeout`(1).
  LENS_LEG_TIMEOUT=${LENS_LEG_TIMEOUT:-900}
  ALMIDE_SKIP_PASS=$p "$BIN" test "$SUITE" >"$log" 2>&1 &
  leg_pid=$!
  waited=0
  code=""
  while kill -0 "$leg_pid" 2>/dev/null; do
    if [ "$waited" -ge "$LENS_LEG_TIMEOUT" ]; then
      kill "$leg_pid" 2>/dev/null
      wait "$leg_pid" 2>/dev/null
      code=124
      break
    fi
    sleep 5
    waited=$((waited + 5))
  done
  if [ -z "$code" ]; then
    wait "$leg_pid"
    code=$?
  fi
  if [ $code -eq 0 ]; then
    echo "CLEAN    $p"
    continue
  fi
  if [ $code -eq 124 ]; then
    if grep -qE "Compile error|panicked|error\[" "$log"; then
      echo "LOUD     $p  (leg capped at ${LENS_LEG_TIMEOUT}s; loud signature in log — see $log)"
    else
      echo "TIMEOUT  $p  — no loud signature; rerun this leg manually (see $log)"
      overall=1
    fi
    continue
  fi
  # Distinguish loud refusals (compile errors, dep panics) from silent value
  # diffs (assert failures in tests that compiled).
  if grep -qE "Compile error|panicked|error\[" "$log"; then
    echo "LOUD     $p  (see $log)"
  else
    echo "FINDING  $p  — tests failed by value; see $log"
    overall=1
  fi
done
exit $overall
