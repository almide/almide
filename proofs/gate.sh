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
# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1


echo "== build the kernel-proven checker from the Coq proof =="
"$ROOT/proofs/build-checker.sh" >/dev/null

COQC="${COQC:-$(command -v coqc)}"

# THE KERNEL ORACLE (brick 6b): re-verify a witness verdict with the Rocq KERNEL
# itself — generate an assertion file with the witness bytes inlined verbatim and
# `coqc` it (vm_compute inside the kernel-checked logic). The extracted binary's
# verdict was already asserted by the caller, so binary and kernel must AGREE or
# the row fails loudly — the extraction pipeline (OCaml codegen + ocamlopt) is no
# longer a trust root for the gate's verdict, only a fast path (TRUSTED_BASE §2).
# Returns nonzero on a verdict mismatch (callers decide whether that is fatal —
# the tamper drill INVERTS it to prove the oracle has teeth).
kernel_verify() { # checker-mode witness-file expected_exit(0=true|1=false)
  local mode=$1 wf=$2 expect=$3 fn want gen
  case "$mode" in
    ownership)       fn="check_bc" ;;
    names)           fn="check_names_cert" ;;
    caps)            fn="check_caps_cert" ;;
    caps-transitive) fn="check_prog_cert" ;;
    call-modes)      fn="check_modes_cert" ;;
    *) echo "kernel_verify: unknown checker mode $mode" >&2; return 2 ;;
  esac
  if [ "$expect" -eq 0 ]; then want=true; else want=false; fi
  gen="$(mktemp /tmp/KernelGate_XXXXXX).v"
  python3 - "$wf" "$fn" "$want" > "$gen" <<'PYEOF'
import sys
w = open(sys.argv[1]).read()
lit = '"' + w.replace('"', '""') + '"'
print("From AlmideTrust Require Import OwnershipChecker NameTotality CapabilityBound CapabilityReach CallModes.")
print("From Stdlib Require Import String.")
print("Open Scope string_scope.")
print("Goal %s %s = %s." % (sys.argv[2], lit, sys.argv[3]))
print("Proof. vm_compute. reflexivity. Qed.")
PYEOF
  (cd "$ROOT/proofs" && "$COQC" -Q . AlmideTrust "$gen" >/dev/null 2>&1)
  local rc=$?
  rm -f "$gen" "${gen%.v}.vo" "${gen%.v}.vos" "${gen%.v}.vok" "${gen%.v}.glob"
  return $rc
}

emit() { (cd "$ROOT" && cargo run -q -p almide-mir --example emit_cert -- "$1" "$2"); }

run() { # scenario property expected_exit
  run_mode "$1" "$2" "$2" "$3"
}
# The call-modes witness (`modes`) is checked by the `call-modes` checker mode —
# emit property and checker mode differ, hence this variant.
run_mode() { # scenario emit-property checker-mode expected_exit
  emit "$1" "$2" > /tmp/compiler.witness
  set +e
  "$ROOT/proofs/checker" "$3" /tmp/compiler.witness >/tmp/gate.out 2>&1; local rc=$?
  set -e
  if [ "$rc" -ne "$4" ]; then
    echo "FAIL [$2] $1: got exit $rc want $4 ($(cat /tmp/gate.out))"; exit 1
  fi
  kernel_verify "$3" /tmp/compiler.witness "$4" \
    || { echo "FAIL [$2] $1: KERNEL oracle disagrees with the binary verdict"; exit 1; }
  echo "ok   [$2] $1: witness '$(cat /tmp/compiler.witness | tr '\n' '|')' -> $(cat /tmp/gate.out) (kernel agrees)"
}

# REAL .almd → frontend → MIR → witness, then the proven checker re-verifies it.
emit_src() { # fixture function property
  (cd "$ROOT" && cargo run -q -p almide-mir --example emit_cert_from_source \
    -- "proofs/fixtures/$1" "$2" "$3");
}
run_src() { # fixture function emit-property expected_exit  (checker mode == emit-property)
  run_src_mode "$1" "$2" "$3" "$3" "$4"
}
# The transitive-cap witness (`tcaps`) is a declared|reachable subset, so it is
# re-verified by the SAME proven subset checker as `caps` — emit property and
# checker MODE differ, hence this variant.
run_src_mode() { # fixture function emit-property checker-mode expected_exit
  emit_src "$1" "$2" "$3" > /tmp/real.witness
  set +e
  "$ROOT/proofs/checker" "$4" /tmp/real.witness >/tmp/gate.out 2>&1; local rc=$?
  set -e
  if [ "$rc" -ne "$5" ]; then
    echo "FAIL [$3] $1::$2 (real source): got exit $rc want $5 ($(cat /tmp/gate.out))"; exit 1
  fi
  kernel_verify "$4" /tmp/real.witness "$5" \
    || { echo "FAIL [$3] $1::$2: KERNEL oracle disagrees with the binary verdict"; exit 1; }
  echo "ok   [$3] $1::$2 (real source): witness '$(cat /tmp/real.witness | tr '\n' '|')' -> $(cat /tmp/gate.out) (kernel agrees)"
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

echo "-- property: call-modes (call sites use the callee's declared param modes) --"
# main passes a heap Handle to beep. AGREE: beep declares one borrow param →
# ACCEPT. MISMATCH: beep declares NO heap param (a mis-lowered call boundary —
# the caller-thinks-borrow/callee-thinks-otherwise shape whose inlining
# double-frees, CallModes.disagreement_double_frees) → REJECT.
run_mode modes-agree    modes call-modes 0
run_mode modes-mismatch modes call-modes 1

echo "-- property: ownership, format v4 (brick 5a/5b: branch agreement + borrow) --"
# BRANCH agreement: both arms acquire one alias (net +1, a heap-result-branch
# shape) → `i{a|a}dd` ACCEPT; a mis-lowered branch whose arms disagree
# (+1 vs 0 — a path-dependent leak) → `i{a|}d` REJECT. The lowering's per-arm
# balance is no longer a trusted convention: the proven CBranch rule re-derives
# arm agreement from the witness itself.
run branch-agree    ownership 0
run branch-mismatch ownership 1
# BORROW liveness: an in-place unique use (MakeUnique) of a live owned object
# is `ibd` (+0 guarded) → ACCEPT; the same use AFTER the release is `idb` — a
# use-after-free the cert previously could not witness → REJECT.
run borrow-live ownership 0
run borrow-uaf  ownership 1

echo "-- property: call-modes over CLOSURE dispatch (brick 5c: possible-callee set) --"
# main calls through a funcref. AGREE: the dispatch shape matches the one table
# target, the site's modes equal its signature → ACCEPT. UNKNOWABLE: the site's
# shape matches NO table target (heap handle vs scalar param) → the sentinel
# row conservatively REJECTS.
run_mode closure-agree      modes call-modes 0
run_mode closure-unknowable modes call-modes 1

echo "-- REAL .almd → frontend → MIR → proven checker (weekly indicator ①: 0→1) --"
run_src return_list.almd build ownership 0
run_src return_list.almd build names     0
# A real program with an EFFECT CALL: ownership of the live string is verified,
# and the capability witness comes from REAL SOURCE (the println reaches Stdout)
# — undeclared, so the cap bound REJECTS it (the sandbox promise on real code).
run_src print_str.almd   main  ownership 0
run_src print_str.almd   main  caps      1
# Compositional (per-call-site) capability: `main` calls `beep` which reaches
# Stdout. main's DIRECT caps are empty (caps → ACCEPT, blind to the callee), but
# the TRANSITIVE witness accounts for the callee at the call site (tcaps,
# re-verified by the proven subset checker) → REJECT. The checker never opens
# the callee; the compiler folds reachability, the checker does the subset.
run_src      transitive_caps.almd main caps  0
run_src_mode transitive_caps.almd main tcaps caps 1
run_src      two_functions.almd  main ownership 0
run_src      two_functions.almd  main names     0
# MANIFEST-DECLARED caps (2c ACCEPT case): the declared bound is the OPERATOR's
# `[permissions].allow` manifest, no longer the vacuous effect-fn-declares-
# everything default. The SAME printing program ACCEPTs under allow=["IO"]
# (used {Stdout} ⊆ declared) and REJECTs under allow=["Rand"].
run_src_manifest() { # fixture function property manifest expected_exit
  (cd "$ROOT" && cargo run -q -p almide-mir --example emit_cert_from_source \
    -- "proofs/fixtures/$1" "$2" "$3" "proofs/fixtures/$4") > /tmp/real.witness
  set +e
  "$ROOT/proofs/checker" "$3" /tmp/real.witness >/tmp/gate.out 2>&1; local rc=$?
  set -e
  if [ "$rc" -ne "$5" ]; then
    echo "FAIL [$3 ⊳ $4] $1::$2 (real source): got exit $rc want $5 ($(cat /tmp/gate.out))"; exit 1
  fi
  kernel_verify "$3" /tmp/real.witness "$5" \
    || { echo "FAIL [$3 ⊳ $4] $1::$2: KERNEL oracle disagrees with the binary verdict"; exit 1; }
  echo "ok   [$3 ⊳ $4] $1::$2 (real source): witness '$(cat /tmp/real.witness | tr '\n' '|')' -> $(cat /tmp/gate.out) (kernel agrees)"
}
run_src_manifest manifest_print.almd main caps manifest_io.toml   0
run_src_manifest manifest_print.almd main caps manifest_rand.toml 1
# Call-mode agreement on a REAL two-function program: every CallFn site's actual
# modes equal the callee's declared heap-param modes (the borrow-only v1
# convention), re-verified by the proven checker — per-function ownership certs
# now COMPOSE by CallModes.check_fill_sound.
run_src_mode two_functions.almd  main modes call-modes 0
# ... and with a heap argument ACTUALLY PASSED: main hands its list to use_it
# (one borrow param) — the site's actual [borrow] equals the declared signature.
run_src      heap_arg_call.almd  main ownership 0
run_src_mode heap_arg_call.almd  main modes call-modes 0
# A REAL heap-result branch (brick 5a): both of pick's arms allocate + move out,
# the merge receives + returns — witnessed through the branch-aware emitter and
# re-verified by the format-v4 proven checker.
run_src      heap_result_if.almd pick ownership 0
# A REAL first-class-function dispatch (brick 5c): `inc()` returns a lifted
# funcref, `f(5)` is an Op::CallIndirect — the witness expands the site to one
# agreement row per possible callee (the lifted lambda), proven per-site.
run_src_mode funcref_call.almd   main modes call-modes 0
# A REAL CAPTURING closure: `adder(3)` returns a closure BLOCK (fnidx + captured
# scalar — a fresh owned heap value, "im"/"id" balanced) and `f(5)` dispatches
# with the block as the borrowed env arg — ownership AND the env's call-mode
# agreement both re-verified by the proven checkers.
run_src      closure_capture.almd main ownership 0
run_src_mode closure_capture.almd main modes call-modes 0

echo "-- kernel-oracle TAMPER DRILL (the extraction-divergence detector, every build) --"
# (i) a CORRUPTED witness (one extra release byte → double-free) must be rejected
# by BOTH the extracted binary and the kernel — agreement on the reject side.
emit balanced ownership > /tmp/tamper.witness
printf 'd' >> /tmp/tamper.witness
set +e; "$ROOT/proofs/checker" ownership /tmp/tamper.witness >/dev/null 2>&1; trc=$?; set -e
if [ "$trc" -ne 1 ]; then echo "FAIL tamper(i): the binary accepted a corrupted witness"; exit 1; fi
kernel_verify ownership /tmp/tamper.witness 1 \
  || { echo "FAIL tamper(i): the kernel accepted a corrupted witness"; exit 1; }
echo "ok   tamper(i): a corrupted witness is rejected by the binary AND the kernel"
# (ii) a SIMULATED DIVERGENT VERDICT: hand the kernel the reject witness but claim
# the binary said ACCEPT — the kernel twin must FAIL. This proves the oracle has
# teeth: a generator that vacuously passed everything would slip through here.
set +e; kernel_verify ownership /tmp/tamper.witness 0; krc=$?; set -e
if [ "$krc" -eq 0 ]; then
  echo "FAIL tamper(ii): the kernel oracle certified a WRONG verdict (drill broken)"; exit 1
fi
echo "ok   tamper(ii): a simulated divergent verdict is CAUGHT by the kernel oracle"

echo
echo "GATE OK: the kernel-proven checker re-verified per-build witnesses on THREE"
echo "properties (ownership + name totality + capability bound), AND a REAL .almd"
echo "program's ownership+name witnesses through the actual frontend (indicator ①"
echo "0→1) — with EVERY row's verdict independently certified by the Rocq KERNEL"
echo "(vm_compute on the witness bytes; binary/kernel divergence fails the build,"
echo "so the extraction pipeline is a fast path, not a trust root). Each accept ⟹"
echo "the property holds of the witnessed MIR, by the Coq theorems. (Whole-program"
echo "WASM-byte safety beyond the rc primitives is still the §3 renderer contract.)"
