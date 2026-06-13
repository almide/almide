#!/usr/bin/env bash
# END-TO-END FLIGHT-GRADE GATE (critical-path brick 3): the UNTRUSTED compiler
# (almide-mir) emits a per-build witness for each flight-grade property; the
# KERNEL-PROVEN checker re-verifies it. accept ⟹ the property holds on the
# emitted bytes — by the Coq soundness theorems:
#   ownership  →  check_all_sound        (no double-free, no leak)
#   names      →  check_names_cert_sound (no dangling MIR reference)
# The compiler may be buggy; if its witness is wrong, the proven checker rejects.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "== build the kernel-proven checker from the Coq proof =="
"$ROOT/proofs/build-checker.sh" >/dev/null

emit() { (cd "$ROOT" && cargo run -q -p almide-mir --example emit_cert -- "$1" "$2"); }

run() { # scenario property expected_exit
  emit "$1" "$2" > /tmp/compiler.witness
  set +e
  "$ROOT/proofs/checker" "$2" /tmp/compiler.witness >/tmp/gate.out 2>&1; local rc=$?
  set -e
  if [ "$rc" -eq "$3" ]; then
    echo "ok   [$2] $1: witness '$(cat /tmp/compiler.witness | tr '\n' '|')' -> $(cat /tmp/gate.out)"
  else
    echo "FAIL [$2] $1: got exit $rc want $3 ($(cat /tmp/gate.out))"; exit 1
  fi
}

echo "== compiler output  ⊳  proven checker =="
echo "-- property: ownership (no double-free, no leak) --"
run balanced ownership 0
run leak     ownership 1

echo "-- property: names (no dangling MIR reference) --"
run balanced names 0
run dangling names 1

echo
echo "GATE OK: the untrusted compiler's per-build witnesses were re-verified by"
echo "the kernel-proven checker on TWO properties (ownership + name totality),"
echo "each accept ⟹ the property holds, by the Coq soundness theorems."
