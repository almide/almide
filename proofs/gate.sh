#!/usr/bin/env bash
# WITNESS-VERIFICATION GATE (critical-path brick 3): the UNTRUSTED producer
# (almide-mir) emits a per-build witness for each flight-grade property; the
# KERNEL-PROVEN checker re-verifies it. accept ⟹ the property holds OF THE
# WITNESSED MIR — by the Coq soundness theorems:
#   ownership  →  check_all_sound        (RC-balanced: no double-free, no leak)
#   names      →  check_names_cert_sound (no dangling MIR reference)
#   caps       →  check_caps_cert_sound  (no undeclared host capability)
# The producer may be buggy; if its witness is wrong, the proven checker rejects.
#
# SCOPE (honest — see proofs/TRUSTED_BASE.md):
#  - The hand-built rows below are projected from REPRESENTATIVE MIR shapes
#    (examples/emit_cert.rs) — they cover accept AND reject for each property.
#  - The REAL-SOURCE rows take an actual .almd through the EXISTING frontend
#    (parse → check → lower → optimize → mono → ir_link) and then almide-mir's
#    lowering to MIR (examples/emit_cert_from_source.rs) — the G1 end-to-end PCC
#    path (weekly indicator ①). The lowering is the value-semantics subset; a
#    program outside it is an explicit Unsupported, never a silent skip.
#  - The witness ⟹ emitted-wasm-bytes link is still the §3 renderer contract
#    (trusted), NOT the proven checker — so even a real program's WASM bytes are
#    not yet gated; only its MIR-level witness is.
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

# REAL .almd → frontend → MIR → witness, then the proven checker re-verifies it.
emit_src() { # fixture function property
  (cd "$ROOT" && cargo run -q -p almide-mir --example emit_cert_from_source \
    -- "proofs/fixtures/$1" "$2" "$3");
}
run_src() { # fixture function property expected_exit
  emit_src "$1" "$2" "$3" > /tmp/real.witness
  set +e
  "$ROOT/proofs/checker" "$3" /tmp/real.witness >/tmp/gate.out 2>&1; local rc=$?
  set -e
  if [ "$rc" -eq "$4" ]; then
    echo "ok   [$3] $1::$2 (real source): witness '$(cat /tmp/real.witness | tr '\n' '|')' -> $(cat /tmp/gate.out)"
  else
    echo "FAIL [$3] $1::$2 (real source): got exit $rc want $4 ($(cat /tmp/gate.out))"; exit 1
  fi
}

echo "== compiler output  ⊳  proven checker =="
echo "-- property: ownership (no double-free, no leak) --"
run balanced ownership 0
run leak     ownership 1

echo "-- property: names (no dangling MIR reference) --"
run balanced names 0
run dangling names 1

echo "-- property: caps (no undeclared host capability) --"
run sandboxed  caps 0
run undeclared caps 1

echo "-- REAL .almd → frontend → MIR → proven checker (weekly indicator ①: 0→1) --"
run_src return_list.almd build ownership 0
run_src return_list.almd build names     0
# A real program with an EFFECT CALL: ownership of the live string is verified,
# and the capability witness comes from REAL SOURCE (the println reaches Stdout)
# — undeclared, so the cap bound REJECTS it (the sandbox promise on real code).
run_src print_str.almd   main  ownership 0
run_src print_str.almd   main  caps      1

echo
echo "GATE OK: the kernel-proven checker re-verified per-build witnesses on THREE"
echo "properties (ownership + name totality + capability bound), AND a REAL .almd"
echo "program's ownership+name witnesses through the actual frontend (indicator ①"
echo "0→1). Each accept ⟹ the property holds of the witnessed MIR, by the Coq"
echo "theorems. (Whole-program WASM-byte safety is still the §3 renderer contract.)"
