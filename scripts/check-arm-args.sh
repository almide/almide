#!/usr/bin/env bash
# Every module-op arm of the structural emitter DECLARES what it does with
# each argument block (#2004): an argument is lowered through
# `lower_arg(expr, want, ArgMode::Borrow | ArgMode::Retain)`
# (crates/almide-wasm/src/arm.rs), never through the bare `lower(`. Under
# Borrow the wrapper releases a temporary the arm only read; under Retain
# the temporary's credit moves into the arm's result. An arm that lowers an
# argument without saying which is the leak class #2004 named — a
# call-produced temporary nobody owns — and this gate makes it a failed
# build instead of a growing row in the ownership matrix
# (crates/almide-wasm/tests/native_result_ownership.rs).
#
# Scope: the files whose fns return `ArmResult` (the arm chain). The only
# `lower(` operands allowed there are the callback bodies an arm inlines
# (`body`, `fold_body`, `arm`) — those are not arguments of the op, they
# are the closure's expression, lowered in place.
#
# What it cannot judge: whether Borrow or Retain is the TRUE mode — that is
# the matrix (a Retain on a read block is a growing row) and the cross-target
# fixtures (a Borrow on a stored block is a divergence). The sites listed in
# NOT_ARMS lower arguments of a different convention (user-fn / closure
# calls move their arguments under the callee-owned convention, rc_arg_guard).
set -euo pipefail
cd "$(dirname "$0")/.."

SRC=crates/almide-wasm/src
NOT_ARMS=(
  calls.rs   # Named / closure call arguments: callee-owned convention (rc_arg_guard); its one ArmResult fn lowers no module argument itself
)

fail=0
checked=0
files=0
for f in "$SRC"/*.rs; do
  base=$(basename "$f")
  grep -q -- '-> ArmResult' "$f" || continue
  skip=0
  for n in "${NOT_ARMS[@]}"; do [ "$n" = "$base" ] && skip=1; done
  [ "$skip" = 1 ] && continue
  files=$((files + 1))
  checked=$((checked + $(grep -c 'lower_arg(' "$f" || true)))
  # ArgMode::Raw is the prim floor's declaration alone.
  if [ "$base" != prim.rs ] && grep -q 'ArgMode::Raw' "$f"; then
    echo "FAIL: $f declares ArgMode::Raw — only prim.rs may (the raw-address rule is the prim floor's)"
    fail=1
  fi
  while IFS=: read -r ln text; do
    if echo "$text" | grep -qE 'self\.lower\((body|fold_body|arm),'; then continue; fi
    echo "FAIL: $f:$ln lowers an argument without declaring its mode (use lower_arg(.., ArgMode::Borrow | ArgMode::Retain))"
    echo "    | $text"
    fail=1
  done < <(grep -nE 'self\.lower\(' "$f" || true)
done

if [ "$fail" -ne 0 ]; then
  echo "arm-args: an arm lowers an argument through the bare lower( — declare Borrow (the arm only reads the block) or Retain (the arm stores or returns it) at the site"
  exit 1
fi
echo "arm-args: OK — $checked declared argument site(s) across $files arm file(s), none undeclared"
