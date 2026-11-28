#!/usr/bin/env bash
# The decision-site ledger's c4 mutation pass (proofs/decision-sites.toml).
#
# Each proofs/decision-controls/<id>.patch removes or neuters one row's check;
# its second line names the test it must turn red:
#   # control_test: tests/<file>.rs::<fn>
# For every patch (or the ids given as arguments) this runs the test on the
# tree (base GREEN), applies the patch and runs it again (patched RED), and
# reverts the patch. A control that stays green under its patch is
# DECORATIVE: the check it removed decided nothing the test can see, and the
# row cannot be stamped. The verdict lines are what a row's `control_seen`
# records. Cargo builds: run it on a build VM, not a laptop.
#
# That every patch still APPLIES is checked on every PR by the judge,
# `tools/almide-gates decision-sites` (git apply --check).
#
#   bash scripts/run-decision-controls.sh [id ...]
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.." || exit 2

ids=("$@")
if [ ${#ids[@]} = 0 ]; then
  for p in proofs/decision-controls/*.patch; do ids+=("$(basename "$p" .patch)"); done
fi

run_test() { # [crates/<crate>/]tests/<file>.rs::<fn>, or [crates/<crate>/]src/<path>.rs::<fn> (a unit test)
  local file="${1%%::*}" fn="${1##*::}" pkg=()
  local unit=()
  case "$file" in
    src/*) unit=(--bin almide);;
    crates/*/src/*) unit=(-p "$(echo "$file" | cut -d/ -f2)" --lib);;
  esac
  if [ ${#unit[@]} -gt 0 ]; then
    # A unit test is named by its module path; the fn name must match exactly one test.
    local n
    n="$(cargo test --release -q "${unit[@]}" -- --list 2>/dev/null | grep -c "::$fn: test$")"
    [ "$n" = 1 ] || return 1
    cargo test --release -q "${unit[@]}" -- "::$fn" >/dev/null 2>&1
    return
  fi
  case "$file" in crates/*/tests/*) pkg=(-p "$(echo "$file" | cut -d/ -f2)");; esac
  cargo test --release -q "${pkg[@]}" --test "$(basename "$file" .rs)" -- --exact "$fn" >/dev/null 2>&1
}

fail=0
declare -A base
for id in "${ids[@]}"; do
  patch="proofs/decision-controls/$id.patch"
  test="$(sed -n 's/^# control_test: //p' "$patch" | head -1)"
  if [ -z "$test" ]; then
    echo "NO-TEST    $id: $patch names no control_test"; fail=1; continue
  fi
  if [ -z "${base[$test]:-}" ]; then
    if run_test "$test"; then base[$test]=green; else base[$test]=red; fi
  fi
  if [ "${base[$test]}" != green ]; then
    echo "BASE-RED   $id: $test fails on the tree itself"; fail=1; continue
  fi
  git apply "$patch" || { echo "NO-APPLY   $id"; fail=1; continue; }
  if run_test "$test"; then
    echo "DECORATIVE $id: $test stays green with $patch applied"; fail=1
  else
    echo "RED        $id: $test fails with the check removed"
  fi
  git apply -R "$patch" || { echo "::error::could not revert $patch — stop"; exit 2; }
done
exit $fail
