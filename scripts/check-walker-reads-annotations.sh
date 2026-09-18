#!/usr/bin/env bash
# THE WALKER ONLY READS ANNOTATIONS (#2186 step 3).
#
# `crates/almide-codegen/src/walker/` is the renderer: it turns IR nodes into
# Rust text and picks a SPELLING for each storage class the passes decided
# (an `AlmideRcCow` local, a shared cell, a lazy global). It must not DECIDE
# ownership — whether a value is moved, cloned, borrowed or owned at a site —
# because rustc is then the only thing standing between a wrong decision and
# a miscompile, and the decision is invisible to every pass gate. Every such
# decision lives in a pass (`BorrowInsertion`, `CloneInsertion`,
# `CaptureClone`, `VarStorage`, `BorrowLowering`) and reaches the walker as
# an IR node or a `CodegenAnnotations` entry.
#
# This gate greps the walker for the ways the decision has crept back in
# before:
#
#   1. an analysis walk of its own (`IrVisitor` / `IrMutVisitor` /
#      `free_vars`) — the walker computing a fact a pass should publish;
#   2. a per-function ownership set recomputed from the params
#      (`ref_params` / `ref_mut_params` / `param_vars`) — the "fn-local
#      truth" that shadowed a program-wide annotation (#1130, #1143, #2194);
#   3. sniffing RENDERED TEXT to decide ownership (`starts_with("match ")`,
#      `ends_with(".clone()")`) — a decision made on the output instead of
#      the IR;
#   4. reading a param's borrow mode anywhere but the two spelling sites
#      that legitimately need it: the fn signature (`walker/mod.rs`) and the
#      `*p = v` store through a `&mut` param (`walker/statements.rs`).
#
# Any hit is a FAIL naming file:line. The negative control is
# scripts/check-walker-reads-annotations-negative.sh.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

WALKER="${1:-crates/almide-codegen/src/walker}"
fail=0

hit() {
  # $1 = pattern (extended regex), $2 = what it means, $3 = files to scan
  local pattern="$1" why="$2"; shift 2
  local found
  found=$(grep -nE "$pattern" "$@" 2>/dev/null || true)
  if [ -n "$found" ]; then
    echo "::error::walker-reads-annotations: $why"
    echo "$found" | sed 's/^/    /'
    fail=1
  fi
}

all=("$WALKER"/*.rs)
[ -e "${all[0]}" ] || { echo "walker-reads-annotations FAIL — no Rust sources under $WALKER" >&2; exit 1; }

hit '\b(IrVisitor|IrMutVisitor)\b|free_vars::|free_vars\(' \
  "the walker runs an analysis walk of its own — publish the fact from a pass instead" "${all[@]}"
hit '\b(ref_params|ref_mut_params|param_vars)\b' \
  "a per-function ownership set recomputed in the walker — read the IR's param mode or an annotation" "${all[@]}"
hit 'starts_with\("(match |if )"\)|ends_with\("\.clone\(\)"\)|starts_with\(.\{.\)' \
  "an ownership decision taken by sniffing rendered text — spell it in the IR" "${all[@]}"

others=()
for f in "${all[@]}"; do
  case "$(basename "$f")" in
    mod.rs|statements.rs) ;;
    *) others+=("$f") ;;
  esac
done
if [ ${#others[@]} -gt 0 ]; then
  hit '\bParamBorrow\b|\.borrow\b[^_(]' \
    "a param's borrow mode read outside the signature and the &mut-store spellings" "${others[@]}"
fi

if [ "$fail" = 0 ]; then
  echo "walker-reads-annotations OK ($(ls "${all[@]}" | wc -l | tr -d ' ') files: no analysis walk, no fn-local ownership set, no rendered-text sniffing)"
fi
exit $fail
