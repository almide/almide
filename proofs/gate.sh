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

# THE PORTABLE CHECKER (#2152): `almide-verify` is what a BINARY distribution
# runs — an independently versioned Rust transcription of the same Coq
# definitions, needing no Rocq toolchain. It carries no soundness theorem of
# its own, so its agreement with the extracted checker is GATED here, on every
# row below (accept and reject alike) and on a seeded random differential at
# the end — never assumed.
echo "== build almide-verify (the portable checker, held to the extracted one's verdicts) =="
(cd "$ROOT" && cargo build -q -p almide-verify)
VERIFY="${CARGO_TARGET_DIR:-$ROOT/target}/debug/almide-verify"
[ -x "$VERIFY" ] || { echo "FAIL: almide-verify was not built at $VERIFY"; exit 1; }
# (No `set +e`/`set -e` toggling inside: re-enabling errexit in here would make
# the final test abort the whole gate when a caller probes for a MISMATCH, as
# the tamper(iii) drill does.)
portable_agrees() { # checker-mode witness-file expected_exit(0=accept|1=reject)
  local rc=0
  "$VERIFY" "$1" "$2" >/dev/null 2>&1 || rc=$?
  [ "$rc" -eq "$3" ]
}

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
    ownership)       fn="check_xc" ;;
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
  portable_agrees "$3" /tmp/compiler.witness "$4" \
    || { echo "FAIL [$2] $1: almide-verify disagrees with the proven checker"; exit 1; }
  echo "ok   [$2] $1: witness '$(cat /tmp/compiler.witness | tr '\n' '|')' -> $(cat /tmp/gate.out) (kernel + almide-verify agree)"
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
  portable_agrees "$4" /tmp/real.witness "$5" \
    || { echo "FAIL [$3] $1::$2: almide-verify disagrees with the proven checker"; exit 1; }
  echo "ok   [$3] $1::$2 (real source): witness '$(cat /tmp/real.witness | tr '\n' '|')' -> $(cat /tmp/gate.out) (kernel + almide-verify agree)"
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
# ALIAS after the release (#3229): an owned object freed, then `Dup`'d and
# moved out is `idam` — it BALANCES, so the count alone accepted it. On a line
# born by a top-level `i`, a count of 0 means DEAD, and the owned-line rule
# probes every later `a` → REJECT. (`balanced` above, `iadd`, aliases the
# object while it is live and still accepts.)
run alias-after-free ownership 1

echo "-- property: ownership, format v5 (law 6: the arm-terminal Return exit) --"
# The R2 `!` exit shape: the arm drops everything it owns BEFORE the
# frame-targeted Return, the surviving arm releases normally → `i{dx|}d`
# ACCEPT (CBranchRet: the flagged arm runs entry→0, the join continues from
# the surviving arm alone). A returning arm that MISSES the pre-return drop
# is a leak on the exit path → `i{x|}d`, the flagged arm sits at count 1 and
# the proven checker REJECTs. First round-trip of an `x` witness through the
# extracted binary + the coqc oracle (check_xc) — previously exercised only
# by in-file Coq Examples.
run branch-ret      ownership 0
run branch-ret-leak ownership 1

echo "-- property: ownership, format v6 (the arm-terminal ABORT) --"
# #2755: an arm that ends the PROCESS (`t`: main's `!` abort, an out-of-bounds
# index — `proc_exit` then `unreachable`, pinned by the emitter's E083 exit
# validator) need only be fault-free: what it still holds is discharged with
# the heap. Every other path balances exactly. The drills: an arm that
# "aborts" and then CONTINUES (an op after `t`) is malformed; a release at 0
# before the abort is a fault; the surviving path still owes its balance; a
# `t` outside a branch has no survivor. Three verdicts on each row.
run_raw() { # certificate expected_exit label
  printf '%s\n' "$1" > /tmp/raw.witness
  set +e; "$ROOT/proofs/checker" ownership /tmp/raw.witness >/tmp/gate.out 2>&1; local rc=$?; set -e
  if [ "$rc" -ne "$2" ]; then echo "FAIL [ownership v6] $3 '$1': got exit $rc want $2"; exit 1; fi
  kernel_verify ownership /tmp/raw.witness "$2"   || { echo "FAIL [ownership v6] $3: KERNEL oracle disagrees"; exit 1; }
  portable_agrees ownership /tmp/raw.witness "$2" || { echo "FAIL [ownership v6] $3: almide-verify disagrees"; exit 1; }
  echo "ok   [ownership v6] $3 '$1': exit $rc (kernel + almide-verify agree)"
}
run_raw 'i{t|d}'  0 "abort discharges what the arm holds"
run_raw '{it|}'   0 "a block born on the aborting path only"
run_raw 'i{td|d}' 1 "an abort that continues is malformed"
run_raw '{dt|}'   1 "a release at 0 before the abort"
run_raw 'i{t|}'   1 "the surviving path still balances"
run_raw 'it'      1 "an abort outside a branch"

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
  portable_agrees "$3" /tmp/real.witness "$5" \
    || { echo "FAIL [$3 ⊳ $4] $1::$2: almide-verify disagrees with the proven checker"; exit 1; }
  echo "ok   [$3 ⊳ $4] $1::$2 (real source): witness '$(cat /tmp/real.witness | tr '\n' '|')' -> $(cat /tmp/gate.out) (kernel + almide-verify agree)"
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
# A HEAP capture (closure env full mode): the block CO-OWNS the captured String
# (`a`+`m` on the caller's object) and the dispatch passes TWO heap args
# (env + argument) — both witnesses proven, kernel-agreed.
run_src      closure_heap_capture.almd greeter ownership 0
run_src      closure_heap_capture.almd main    ownership 0
run_src_mode closure_heap_capture.almd main    modes call-modes 0

echo "-- kernel-oracle TAMPER DRILL (the extraction-divergence detector, every build) --"
# (i) a CORRUPTED witness (one extra release byte → double-free) must be rejected
# by BOTH the extracted binary and the kernel — agreement on the reject side.
emit balanced ownership > /tmp/tamper.witness
printf 'd' >> /tmp/tamper.witness
set +e; "$ROOT/proofs/checker" ownership /tmp/tamper.witness >/dev/null 2>&1; trc=$?; set -e
if [ "$trc" -ne 1 ]; then echo "FAIL tamper(i): the binary accepted a corrupted witness"; exit 1; fi
kernel_verify ownership /tmp/tamper.witness 1 \
  || { echo "FAIL tamper(i): the kernel accepted a corrupted witness"; exit 1; }
portable_agrees ownership /tmp/tamper.witness 1 \
  || { echo "FAIL tamper(i): almide-verify accepted a corrupted witness"; exit 1; }
echo "ok   tamper(i): a corrupted witness is rejected by the binary, the kernel AND almide-verify"
# (ii) a SIMULATED DIVERGENT VERDICT: hand the kernel the reject witness but claim
# the binary said ACCEPT — the kernel twin must FAIL. This proves the oracle has
# teeth: a generator that vacuously passed everything would slip through here.
set +e; kernel_verify ownership /tmp/tamper.witness 0; krc=$?; set -e
if [ "$krc" -eq 0 ]; then
  echo "FAIL tamper(ii): the kernel oracle certified a WRONG verdict (drill broken)"; exit 1
fi
echo "ok   tamper(ii): a simulated divergent verdict is CAUGHT by the kernel oracle"

# ── #1696 phase A2: the STRUCTURAL leg's witnesses through the SAME proven
# checker. The recorder (crates/almide-wasm/src/witness.rs) logs each RC
# event as its instruction is emitted, so an accept here certifies the
# emitted instruction stream's ownership discipline — one contract level
# closer to the bytes than the incumbent's MIR-side projection. Three
# standing samples (two fixture fns + one organic corpus fn) plus the
# same corruption drill the incumbent rows get.
echo
echo "== structural leg  ⊳  proven checker (#1696 phase A2) =="
emit_structural() { # fixture-rel fn-name
  (cd "$ROOT" && cargo run -q -p almide-wasm --example emit_structural_witness -- "$1" "$2")
}
run_structural() { # fixture-rel fn-name expected_exit
  emit_structural "$1" "$2" > /tmp/structural.witness
  set +e
  "$ROOT/proofs/checker" ownership /tmp/structural.witness >/tmp/gate.out 2>&1; local rc=$?
  set -e
  if [ "$rc" -ne "$3" ]; then
    echo "FAIL [structural] $1::$2: got exit $rc want $3 ($(cat /tmp/gate.out))"; exit 1
  fi
  kernel_verify ownership /tmp/structural.witness "$3"     || { echo "FAIL [structural] $1::$2: KERNEL oracle disagrees"; exit 1; }
  portable_agrees ownership /tmp/structural.witness "$3" \
    || { echo "FAIL [structural] $1::$2: almide-verify disagrees with the proven checker"; exit 1; }
  echo "ok   [structural] $1::$2: witness '$(cat /tmp/structural.witness | tr '\n' '|')' accepted (kernel + almide-verify agree)"
}
run_structural spec/wasm_cross/witness_straightline.almd shed 0
run_structural spec/wasm_cross/witness_straightline.almd tag 0
run_structural spec/wasm_cross/r5_lowmisc_param_try_err.almd id_list 0
# Corruption drill: strip one release — the checker must see the leak.
emit_structural spec/wasm_cross/witness_straightline.almd shed | sed 's/dd$/d/' > /tmp/structural.tamper
set +e; "$ROOT/proofs/checker" ownership /tmp/structural.tamper >/dev/null 2>&1; src_rc=$?; set -e
if [ "$src_rc" -ne 1 ]; then echo "FAIL structural-tamper: a leaked structural witness was accepted"; exit 1; fi
kernel_verify ownership /tmp/structural.tamper 1   || { echo "FAIL structural-tamper: the kernel accepted the leak"; exit 1; }
portable_agrees ownership /tmp/structural.tamper 1 || { echo "FAIL structural-tamper: almide-verify accepted the leak"; exit 1; }
echo "ok   structural-tamper: a leaked structural witness is rejected by the binary AND the kernel"

# ── #1696 phase B1: the CALL BOUNDARY through the same checker. A droppable
# Var argument shares (`a`, the site's real rc_inc) and moves into the callee
# (`m`); a `return_call` releases the owned param / local before the jump
# (`d`); the callee's own param is released at its epilogue (`id`). The drill
# strips the tail-site release — exactly the leak class the witness found in
# 63 module-space wrappers (fan_map, http_set_header, __gby_add) before
# module space joined the tail-release set.
echo
echo "== structural leg, call boundary  ⊳  proven checker (#1696 phase B1) =="
run_structural spec/wasm_cross/witness_straightline.almd take 0
run_structural spec/wasm_cross/witness_straightline.almd pass 0
run_structural spec/wasm_cross/witness_straightline.almd bind_then_pass 0
emit_structural spec/wasm_cross/witness_straightline.almd pass | sed 's/^iamd$/iam/' > /tmp/structural.tamper
set +e; "$ROOT/proofs/checker" ownership /tmp/structural.tamper >/dev/null 2>&1; src_rc=$?; set -e
if [ "$src_rc" -ne 1 ]; then echo "FAIL structural-tamper(B1): a return_call that skips its param release was accepted"; exit 1; fi
kernel_verify ownership /tmp/structural.tamper 1   || { echo "FAIL structural-tamper(B1): the kernel accepted the unreleased param"; exit 1; }
portable_agrees ownership /tmp/structural.tamper 1 || { echo "FAIL structural-tamper(B1): almide-verify accepted the unreleased param"; exit 1; }
echo "ok   structural-tamper(B1): an unreleased tail-site param is rejected by the binary AND the kernel"

# ── #1696 step 4: STATEMENT CALLS and MODULE CALLS through the same checker.
# `discard` drops an owned call result in statement position — the route
# releases the credit it arrived with (`id`); `stamp` binds a native arm's
# declared-Owned result (`i` … `d`); `shout` tails a registry-route module
# call under the callee's param_owned convention (`iamd` + the result `im`);
# `say` lends a param to the println arm (Borrow: no RC site, `id` is the
# param's own pair). The drill strips the discard release — the leak class
# the structural leg had before the discard route (a bare `f(x)` statement
# dropped its owned result on the floor).
echo
echo "== structural leg, statement + module calls  ⊳  proven checker (#1696 step 4) =="
run_structural spec/wasm_cross/witness_straightline.almd discard 0
run_structural spec/wasm_cross/witness_straightline.almd stamp 0
run_structural spec/wasm_cross/witness_straightline.almd shout 0
run_structural spec/wasm_cross/witness_straightline.almd say 0
emit_structural spec/wasm_cross/witness_straightline.almd discard | sed '2s/^id$/i/' > /tmp/structural.tamper
set +e; "$ROOT/proofs/checker" ownership /tmp/structural.tamper >/dev/null 2>&1; src_rc=$?; set -e
if [ "$src_rc" -ne 1 ]; then echo "FAIL structural-tamper(step4): a discarded result that was never released was accepted"; exit 1; fi
kernel_verify ownership /tmp/structural.tamper 1   || { echo "FAIL structural-tamper(step4): the kernel accepted the unreleased discard"; exit 1; }
portable_agrees ownership /tmp/structural.tamper 1 || { echo "FAIL structural-tamper(step4): almide-verify accepted the unreleased discard"; exit 1; }
echo "ok   structural-tamper(step4): an unreleased statement-call result is rejected by the binary AND the kernel"

# ── #2755: TEMPORARIES on the flat alphabet through the same checker. `nest`
# hands a call's owned result straight to another call (`im`: born, moved
# into the owned param); `greet` concatenates — its operands are bound first
# (`id` each), the fresh concat moves out (`im`); `wrap` tails `some(xs)`, the
# param sharing into the payload slot (`am`) and the cell moving out (`im`).
# Two drills: a nested temporary that never leaves the frame, and a concat
# operand that is never released — each must be seen as the leak it is.
echo
echo "== structural leg, temporaries  ⊳  proven checker (#2755) =="
run_structural spec/wasm_cross/witness_straightline.almd nest 0
run_structural spec/wasm_cross/witness_straightline.almd greet 0
run_structural spec/wasm_cross/witness_straightline.almd wrap 0
tamper_structural() { # fn sed-expr label
  emit_structural spec/wasm_cross/witness_straightline.almd "$1" | sed "$2" > /tmp/structural.tamper
  if cmp -s /tmp/structural.tamper <(emit_structural spec/wasm_cross/witness_straightline.almd "$1"); then
    echo "FAIL structural-tamper($3): the drill changed nothing (the witness shape moved)"; exit 1
  fi
  set +e; "$ROOT/proofs/checker" ownership /tmp/structural.tamper >/dev/null 2>&1; src_rc=$?; set -e
  if [ "$src_rc" -ne 1 ]; then echo "FAIL structural-tamper($3): a leaked temporary was accepted"; exit 1; fi
  kernel_verify ownership /tmp/structural.tamper 1   || { echo "FAIL structural-tamper($3): the kernel accepted the leak"; exit 1; }
  portable_agrees ownership /tmp/structural.tamper 1 || { echo "FAIL structural-tamper($3): almide-verify accepted the leak"; exit 1; }
  echo "ok   structural-tamper($3): the leak is rejected by the binary AND the kernel"
}
tamper_structural nest 's/^im$/i/' "#2755 nested call"
tamper_structural greet '3s/^id$/i/' "#2755 concat operand"

# ── #2756: BRANCH FRAMES through the same checker. The recorder logs each RC
# event with the `if` / `match` structure it was emitted under and renders
# one line per object: a single path flat, two paths as the whole-line
# branch `{p|q}` (the checker runs each arm from rc 0 and both must end at 0),
# more as per-site `{a|b}` / `{a x|b}` items. `pick` hands its list to
# `take` on one arm only (`{iamd|id}`); `label` keeps a string alive across
# a match, returning its share on one arm (`{|am}`). Drills: drop the
# release on the arm that took the share, and the move-out on the arm that
# returned it — each arm is checked on its own, so each leak is seen.
echo
echo "== structural leg, branch frames  ⊳  proven checker (#2756) =="
run_structural spec/wasm_cross/witness_straightline.almd pick 0
run_structural spec/wasm_cross/witness_straightline.almd label 0
tamper_structural pick 's/^{iamd|id}$/{iam|id}/' "#2756 if arm"
tamper_structural label 's/^{|am}$/{|a}/' "#2756 match arm"

# ── #2757: a SELF TAIL CALL (loop-converted by tco.rs) certified as the next
# activation of the frame: `count_down` shares its list into the next
# activation's param and releases its own credit before the loop-back
# (`{iamd|id}` — recursive path, base path). Drill: drop the loop-back
# release, which is the leak a loop-form frame that forgets its params has.
echo
echo "== structural leg, self tail calls  ⊳  proven checker (#2757) =="
run_structural spec/wasm_cross/witness_straightline.almd count_down 0
tamper_structural count_down 's/^{iamd|id}$/{iam|id}/' "#2757 loop-back"

# ── #2757: LOOP BODIES as activations, each iteration on its own line from
# rc 0 (the loop is a holder that must hand back every credit it takes).
# `tally` shares its list into `take` on every pass (`am` on the loop line)
# and binds a per-iteration concat (`id`: the next rebind or the epilogue
# releases it); `drain`'s row lives one `while` iteration (`{|id}`: the
# check that leaves binds nothing). Drills: the loop line keeps a credit it
# took, and an iteration's block is never released.
echo
echo "== structural leg, loop bodies  ⊳  proven checker (#2757) =="
run_structural spec/wasm_cross/witness_straightline.almd tally 0
run_structural spec/wasm_cross/witness_straightline.almd drain 0
tamper_structural tally '2s/^am$/a/' "#2757 for body"
tamper_structural drain 's/^{|id}$/{|i}/' "#2757 while body"

# ── #2758: an EFFECT frame certified at its raw ok type. `stash` shares its
# borrowed param into the ok carrier's slot (`am`); the carrier is born and
# moves out of the frame (`im`). Drill: the carrier never leaves — the leak
# an effect frame that built its answer and dropped it would have.
echo
echo "== structural leg, effect frames  ⊳  proven checker (#2758) =="
run_structural spec/wasm_cross/witness_straightline.almd stash 0
tamper_structural stash '2s/^im$/i/' "#2758 ok carrier"

# ── #2758: EARLY `!` EXITS. The `!` site is a branch whose arm propagates:
# `stash_len`'s parked carrier is shared, released with the frame and moved
# out on that arm (`{iadm|id}`); the payload is a view the bind takes a
# credit of. `stash_both` has two sites: the second carrier has three paths,
# so its exit folds into a v5 branch-return item (`i{admx|}d`, checked from
# the count at the site to exactly 0). Drills: the propagated carrier never
# leaves, and the folded exit forgets its move-out.
echo
echo "== structural leg, early exits  ⊳  proven checker (#2758) =="
run_structural spec/wasm_cross/witness_straightline.almd stash_len 0
run_structural spec/wasm_cross/witness_straightline.almd stash_both 0
tamper_structural stash_len 's/^{iadm|id}$/{iad|id}/' "#2758 propagated carrier"
tamper_structural stash_both 's/^i{admx|}d$/i{adx|}d/' "#2758 folded exit"

# ── #2758: CLOSURES. `adder` shares its param into the new env (`am`: the
# env's drop glue releases it) and the env block moves out (`im`). The
# lambda body (`<lambda#0>`, the fixture's only lambda) is a frame of its
# own: its param callee-owned (`id`), its capture a view of the env (an
# empty line). Drills: the env never leaves, and the lambda keeps its param.
echo
echo "== structural leg, closures  ⊳  proven checker (#2758) =="
run_structural spec/wasm_cross/witness_straightline.almd adder 0
run_structural spec/wasm_cross/witness_straightline.almd '<lambda#0>' 0
tamper_structural adder '2s/^im$/i/' "#2758 closure env"
tamper_structural '<lambda#0>' '1s/^id$/i/' "#2758 lambda param"

# ── #2758: a CLOSURE CALL. `apply_len` lends its Fn value to the lifted body
# and shares its list into the body's callee-owned param (`am`). Drill: the
# share never moves into the callee.
run_structural spec/wasm_cross/witness_straightline.almd apply_len 0
tamper_structural apply_len 's/^am$/a/' "#2758 closure call argument"

# ── #2755: AGGREGATES and INTERPOLATION. `pair` tails a tuple: the param
# shares into the slot (`am`) and the fresh block moves out (`im`). `left`
# destructures its borrowed param: each binder is a view of a slot (an empty
# line for the param), and the returned binder shares and moves out (`am`).
# `hello` tails an interpolation: the build reads the part, the captured
# block moves out (`im`). Drills: the tuple never leaves, the returned view's
# share never moves out, and the captured text never leaves.
echo
echo "== structural leg, aggregates and interpolation  ⊳  proven checker (#2755) =="
run_structural spec/wasm_cross/witness_straightline.almd pair 0
run_structural spec/wasm_cross/witness_straightline.almd left 0
run_structural spec/wasm_cross/witness_straightline.almd hello 0
tamper_structural pair '2s/^im$/i/' "#2755 tuple literal"
tamper_structural left 's/^am$/a/' "#2755 destructured binder"
# `t.0` / `r.f`: a slot VIEW of the bound param, shared out by the tail.
run_structural spec/wasm_cross/witness_straightline.almd first_of 0
run_structural spec/wasm_cross/witness_straightline.almd name_of 0
tamper_structural first_of 's/^am$/m/' "#2755 tuple slot view"
tamper_structural name_of 's/^am$/m/' "#2755 record field view"
tamper_structural hello 's/^im$/i/' "#2755 interpolation"

# ── #2755 / #2758: INLINED CALLBACKS. `list.map` / `filter` / `fold` lower a
# literal lambda's body in the frame, one loop activation per element: the
# param is a view of the element (an empty line), `bang`'s fresh concat
# moves into the result spine (`im` on the loop's line) and the spine moves
# out (`im`); `kept` and `total` consume a Bool and a scalar. Drills: the
# per-element text never reaches the spine, and the spine never leaves.
echo
echo "== structural leg, inlined callbacks  ⊳  proven checker (#2755) =="
run_structural spec/wasm_cross/witness_straightline.almd bang 0
run_structural spec/wasm_cross/witness_straightline.almd kept 0
run_structural spec/wasm_cross/witness_straightline.almd total 0
tamper_structural bang '4s/^im$/i/' "#2755 callback element"
tamper_structural kept '3s/^im$/i/' "#2755 filtered spine"

# ── #2755: a HEAP `fold` accumulator is a loop-carried owner (born with the
# seed, replaced by each step's fresh result, the old value released as the
# activation closes, the final value moving out), and `find`'s hit is a
# branch where the element's view shares into the fresh some-cell (`{|am}`).
# Drills: the final accumulator never leaves; the hit takes no credit to move.
echo
echo "== structural leg, heap fold + find  ⊳  proven checker (#2755) =="
run_structural spec/wasm_cross/witness_straightline.almd joined 0
run_structural spec/wasm_cross/witness_straightline.almd long_one 0
tamper_structural joined '8s/^im$/i/' "#2755 heap fold accumulator"
tamper_structural long_one '3s/^{|am}$/{|m}/' "#2755 find hit"

# ── #2755: `r ?? fallback`, a two-arm branch site with an owned join. The
# borrowed param holds no credit (an empty line); the fresh fallback moves
# into the join on the none arm (`{|im}`), the payload view takes its share
# and moves on the some arm (`{|am}`), the join moves out (`im`). Drill: the
# fallback never reaches the join.
echo
echo "== structural leg, unwrap-or  ⊳  proven checker (#2755) =="
run_structural spec/wasm_cross/witness_straightline.almd or_zero 0
tamper_structural or_zero 's/^{|im}$/{|i}/' "#2755 unwrap-or fallback"

# ── #2758: an `err(e)` RAISED from an effect body, and a call through a record
# FIELD. `raise`: the literal payload moves into the err block, the block
# leaves on the raising arm, the ok carrier on the other (`{|im}` each).
# `apply_op`: the record param and the field's Fn value are views (empty
# lines). Drills: the err block never leaves; the borrowed record is released.
echo
echo "== structural leg, effect raise + field callee  ⊳  proven checker (#2758) =="
run_structural spec/wasm_cross/witness_straightline.almd raise 0
run_structural spec/wasm_cross/witness_straightline.almd apply_op 0
tamper_structural raise '2s/^{|im}$/{|i}/' "#2758 raised err block"
tamper_structural apply_op '1s/^$/d/' "#2758 field callee record"

# ── #2758: a callback that RAISES instantiates the self-hosted
# `list.__fallible_map` — an ordinary call. The literal lambda is a closure
# value: its env is built here, lent to the lifted body and released (`id`).
# Drill: the env never released.
echo
echo "== structural leg, fallible HOF closure argument  ⊳  proven checker (#2758) =="
run_structural spec/wasm_cross/witness_straightline.almd raised_all 0
tamper_structural raised_all '1s/^id$/i/' "#2758 fallible HOF env"

# ── #2755: `fan.map` / `fan.any`'s sequential accumulator. Each element's
# Result carrier is born in its activation: it leaves as the whole result, or
# is released (`{id|im}`); `fan_bang`'s ok payload moves into the accumulator
# (`{|im}`). Drills: a consumed carrier never released; a payload never moved.
echo
echo "== structural leg, fan accumulator  ⊳  proven checker (#2755) =="
run_structural spec/wasm_cross/witness_straightline.almd fan_bang 0
run_structural spec/wasm_cross/witness_straightline.almd fan_first 0
tamper_structural fan_bang '4s/^{id|im}$/{i|im}/' "#2755 fan carrier"
tamper_structural fan_bang '6s/^{|im}$/{|i}/' "#2755 fan payload"

# ── #2755: `xs[i]` over a bound list is a VIEW of the element (shared out,
# `{|am}`); out of bounds ABORTS the process with nothing released — the
# abort terminal discharges it, and an aborting path a returning path extends
# needs no arm of its own (`check_line_prefix_safe`: `tagged_at`'s `t` aborts
# holding its block after `i`, a prefix of its returning `id`). Drills: the
# view never shared; `t` never released on the returning path.
echo
echo "== structural leg, list index + abort exit  ⊳  proven checker (#2755) =="
run_structural spec/wasm_cross/witness_straightline.almd elem_at 0
run_structural spec/wasm_cross/witness_straightline.almd tagged_at 0
tamper_structural elem_at '2s/^{|am}$/{|m}/' "#2755 index view"
tamper_structural tagged_at '2s/^{|id}$/{|i}/' "#2755 abort-path release"

# ── #2758: MODULE-SPACE LETS. main's prologue stores each top-let into its
# global: `ALPHA`'s literal moves in (`im`); `TBL`'s initializer (a
# `bytes.from_list` result, line 3) is copied into the global (`im`) and
# itself released (`id` — it stayed live until exit before #2758, #2967).
# Drill: that release is dropped.
echo
echo "== structural leg, module-space lets  ⊳  proven checker (#2758) =="
run_structural spec/wasm_cross/module_global_const.almd main 0
emit_structural spec/wasm_cross/module_global_const.almd main | sed '3s/^id$/i/' > /tmp/structural.tamper
if cmp -s /tmp/structural.tamper <(emit_structural spec/wasm_cross/module_global_const.almd main); then
  echo "FAIL structural-tamper(#2758 top-let): the drill changed nothing"; exit 1
fi
set +e; "$ROOT/proofs/checker" ownership /tmp/structural.tamper >/dev/null 2>&1; src_rc=$?; set -e
if [ "$src_rc" -ne 1 ]; then echo "FAIL structural-tamper(#2758 top-let): an unreleased initializer was accepted"; exit 1; fi
kernel_verify ownership /tmp/structural.tamper 1   || { echo "FAIL structural-tamper(#2758 top-let): the kernel accepted the leak"; exit 1; }
portable_agrees ownership /tmp/structural.tamper 1 || { echo "FAIL structural-tamper(#2758 top-let): almide-verify accepted the leak"; exit 1; }
echo "ok   structural-tamper(#2758 top-let): an initializer the copy left behind is rejected by the binary AND the kernel"

# ── #2758: the CALL-MODE witness of the structural leg. Per-frame
# certificates compose only if every call site hands each heap argument over
# as its callee's frame assumed (a move into a borrowed param leaks, a lend to
# an owned param frees twice). The emitter records each table fn's frame
# convention and each Named / linked site's actual hand-over; the stream is
# judged by CallModes.check_modes_cert. Drill: one site's move becomes a
# borrow against its callee's signature.
echo
echo "== structural leg, call modes  ⊳  proven checker (#2758) =="
(cd "$ROOT" && cargo run -q -p almide-wasm --example emit_call_modes -- spec/wasm_cross/witness_straightline.almd) > /tmp/structural.modes
set +e; "$ROOT/proofs/checker" call-modes /tmp/structural.modes >/tmp/gate.out 2>&1; src_rc=$?; set -e
if [ "$src_rc" -ne 0 ]; then echo "FAIL [structural] call-modes: rejected ($(cat /tmp/gate.out))"; exit 1; fi
kernel_verify call-modes /tmp/structural.modes 0   || { echo "FAIL [structural] call-modes: KERNEL oracle disagrees"; exit 1; }
portable_agrees call-modes /tmp/structural.modes 0 || { echo "FAIL [structural] call-modes: almide-verify disagrees"; exit 1; }
echo "ok   [structural] call-modes: every site of witness_straightline agrees with its callee (kernel + almide-verify agree)"
sed -E 's/\|([0-9]+) 1/|\1 0/' /tmp/structural.modes > /tmp/structural.modes.tamper
if cmp -s /tmp/structural.modes /tmp/structural.modes.tamper; then echo "FAIL structural-tamper(#2758 call modes): the drill changed nothing"; exit 1; fi
set +e; "$ROOT/proofs/checker" call-modes /tmp/structural.modes.tamper >/dev/null 2>&1; src_rc=$?; set -e
if [ "$src_rc" -ne 1 ]; then echo "FAIL structural-tamper(#2758 call modes): a mode mismatch was accepted"; exit 1; fi
kernel_verify call-modes /tmp/structural.modes.tamper 1   || { echo "FAIL structural-tamper(#2758 call modes): the kernel accepted the mismatch"; exit 1; }
portable_agrees call-modes /tmp/structural.modes.tamper 1 || { echo "FAIL structural-tamper(#2758 call modes): almide-verify accepted the mismatch"; exit 1; }
echo "ok   structural-tamper(#2758 call modes): a site that lends where its callee takes a credit is rejected by the binary AND the kernel"

# ── #2759: NAME TOTALITY and CAPABILITIES of the structural build, projected
# from the module bytes (crates/almide-wasm/src/cert_project.rs) and printed
# by the bundle producer the corpus sweep (proofs/structural-wall.sh) runs.
# Names: the function index space of the stock-WASI bytes and one frame's
# locals. Capabilities: `shed` is a plain `fn`, bounded by the console, and
# the program's call graph is judged by CapabilityReach (the reach computed
# in the proof). Drills: an index one past the function space (an undefined
# callee), one past `shed`'s locals, a file write reached from the plain fn,
# and the same write added to a plain fn's node of the call graph.
echo
echo "== structural leg, names + capabilities  ⊳  proven checker (#2759) =="
WS=spec/wasm_cross/witness_straightline.almd
bundle_one() { # fixture property function
  (cd "$ROOT" && cargo run -q -p almide-wasm --example emit_structural_bundle -- "$1" --only "$2" "$3")
}
run_structural_prop() { # fixture property function expected_exit
  bundle_one "$1" "$2" "$3" > /tmp/structural.prop
  set +e; "$ROOT/proofs/checker" "$2" /tmp/structural.prop >/tmp/gate.out 2>&1; local rc=$?; set -e
  if [ "$rc" -ne "$4" ]; then echo "FAIL [structural $2] $1::$3: got exit $rc want $4 ($(cat /tmp/gate.out))"; exit 1; fi
  kernel_verify "$2" /tmp/structural.prop "$4"   || { echo "FAIL [structural $2] $1::$3: KERNEL oracle disagrees"; exit 1; }
  portable_agrees "$2" /tmp/structural.prop "$4" || { echo "FAIL [structural $2] $1::$3: almide-verify disagrees"; exit 1; }
  echo "ok   [structural $2] $1::$3: witness '$(head -c 120 /tmp/structural.prop | tr '\n' '|')…' accepted (kernel + almide-verify agree)"
}
drill_structural_prop() { # property label — /tmp/structural.prop is the honest witness, /tmp/structural.tamper the drill
  if cmp -s /tmp/structural.prop /tmp/structural.tamper; then echo "FAIL structural-tamper($2): the drill changed nothing"; exit 1; fi
  set +e; "$ROOT/proofs/checker" "$1" /tmp/structural.tamper >/dev/null 2>&1; local rc=$?; set -e
  if [ "$rc" -ne 1 ]; then echo "FAIL structural-tamper($2): the binary accepted it"; exit 1; fi
  kernel_verify "$1" /tmp/structural.tamper 1   || { echo "FAIL structural-tamper($2): the kernel accepted it"; exit 1; }
  portable_agrees "$1" /tmp/structural.tamper 1 || { echo "FAIL structural-tamper($2): almide-verify accepted it"; exit 1; }
  echo "ok   structural-tamper($2): rejected by the binary, the kernel AND almide-verify"
}
# Append the first id past the defined range to the used side.
past_defined() { python3 -c 'import sys; d,u=sys.stdin.read().split("|",1); print(d+"|"+u.strip()+" "+str(len(d.split())), end="")'; }
run_structural_prop "$WS" names '(module:funcs)' 0
past_defined < /tmp/structural.prop > /tmp/structural.tamper
drill_structural_prop names "#2759 undefined callee"
run_structural_prop "$WS" names 'locals:shed' 0
past_defined < /tmp/structural.prop > /tmp/structural.tamper
drill_structural_prop names "#2759 undefined local"
run_structural_prop "$WS" caps shed 0
sed 's/$/ 4/' /tmp/structural.prop > /tmp/structural.tamper
drill_structural_prop caps "#2759 file write from a plain fn"
run_structural_prop "$WS" caps-transitive '(program)' 0
python3 - > /tmp/structural.tamper <<'PYEOF'
nodes = open("/tmp/structural.prop").read().split(";")
k = next(i for i, n in enumerate(nodes) if n.split("|")[0] == "0")  # a plain fn's node: the console only (#3248 dropped stdin)
d, direct, callees = nodes[k].split("|")
nodes[k] = "|".join([d, (direct + " 4").strip(), callees])
print(";".join(nodes), end="")
PYEOF
drill_structural_prop caps-transitive "#2759 call graph: file write from a plain fn"

# ── #3041: DECLARATIONS the source implies but the synthesized fn does not
# spell. `branch_lift_synth_0` is lifted out of an `effect fn`'s body and
# reads a file: it is bounded by its origin's `effect` (the `effect:origin`
# marker, not the ABI flag). `heavy` is a plain fn run inside a
# `fan.timeout` region: the fuel meter's deadline test reads the clock in
# its frame, and the declaration table charges that read to the region's
# opener (witness_decls.rs, the #3041 ruling), so its reach is empty. Drills:
# the synthesized fn bounded as the plain fn it is spelled as, and a clock
# read that is the frame's own.
FS=spec/wasm_cross/fs_read_text_utf8.almd
run_structural_prop "$FS" caps branch_lift_synth_0 0
sed 's/^[^|]*|/0|/' /tmp/structural.prop > /tmp/structural.tamper
drill_structural_prop caps "#3041 synthesized fn declared plain"
TO=spec/wasm_cross/fuel_timeout_ends.almd
run_structural_prop "$TO" caps heavy 0
sed 's/$/5/' /tmp/structural.prop > /tmp/structural.tamper
drill_structural_prop caps "#3041 a clock read of the frame's own"

# ── #2152: almide-verify against the extracted checker on witnesses NO
# producer wrote. The rows above only reach the shapes the emitters produce;
# the transcription must agree on the whole input space, malformed bytes
# included (a dangling `(`, a stray `x`, a zero-padded id, an out-of-range
# callee). A crash of the reference (exit 2) is a harness failure, never a
# verdict.
# Seeded, so a disagreement reproduces; every witness goes through the
# extracted checker one file at a time and through almide-verify as ONE
# certificate bundle — which also exercises the bundle reader on every shape.
echo
echo "== almide-verify  ⊳  extracted checker: seeded random differential (#2152) =="
set +e; portable_agrees ownership /tmp/tamper.witness 0; prc=$?; set -e
if [ "$prc" -eq 0 ]; then echo "FAIL tamper(iii): the almide-verify leg certified a WRONG verdict (drill broken)"; exit 1; fi
echo "ok   tamper(iii): a simulated almide-verify divergence is CAUGHT by the agreement leg"
python3 - "$ROOT/proofs/checker" "$VERIFY" <<'PYEOF'
import os, random, re, subprocess, sys, tempfile
checker, verifier = sys.argv[1], sys.argv[2]
rng = random.Random(2152)
PER_MODE = 200

# Ids stay below ~10^6: the extracted checker's `nat` is Peano (Extract.v maps
# no ExtrOcamlNatInt), so an id is built as that many successor cells and a
# 20-digit id never finishes parsing. almide-verify decides such ids exactly
# (normalized digit strings; its own unit tests cover values beyond u64) —
# only this comparison is bounded, by the reference's cost, not by semantics.
def nums(k, hi):
    out = []
    for _ in range(k):
        r = rng.random()
        if r < 0.05:
            out.append("0" * rng.randint(1, 3) + str(rng.randint(0, hi)))   # leading zeros
        elif r < 0.08:
            out.append(str(rng.randint(10**4, 10**6)))                      # multi-digit, out of range
        else:
            out.append(str(rng.randint(0, hi)))
    return rng.choice([" ", "  ", ","]).join(out)

def ops(n):
    # Mostly net-zero tokens, so bodies and arms balance often enough to
    # exercise the accept side; the singles break the balance the rest of the time.
    return "".join(rng.choice(["di", "di", "id", "ad", "b", "i", "d", "m"]) for _ in range(n))

def ownership():
    # Half byte soup (malformed nesting, stray markers), half the emitter's
    # shapes with random contents — the soup alone almost never balances,
    # and the accept side needs coverage too.
    if rng.random() < 0.5:
        return "".join(rng.choice("iiiidddaambrIDxXtT(){}[]||\n ") for _ in range(rng.randint(0, 28)))
    def item():
        r = rng.random()
        if r < 0.5:
            return ops(1)
        if r < 0.65:
            return "(" + ops(rng.randint(0, 3)) + ")"
        if r < 0.8:
            return "[" + ops(rng.randint(0, 3)) + "|" + ops(rng.randint(0, 2)) + "]"
        return "{" + ops(rng.randint(0, 3)) + rng.choice(["", "x", "t"]) + "|" + ops(rng.randint(0, 3)) + rng.choice(["", "", "x", "t"]) + "}"
    return "\n".join("i" + "".join(item() for _ in range(rng.randint(0, 4))) + rng.choice(["d", "m", "dd", ""])
                     for _ in range(rng.randint(1, 3)))

def subset():
    s = nums(rng.randint(0, 5), 6) + "|" + nums(rng.randint(0, 4), 6)
    if rng.random() < 0.1:
        s += rng.choice(["|3", ";1", "x", "\n2"])
    # No bar at all: a separator takes its place, so two ids never fuse into
    # one the Peano reference cannot build.
    return s.replace("|", " ") if rng.random() < 0.05 else s

def graph():
    k = rng.randint(1, 5)
    s = ";".join(nums(rng.randint(0, 3), 3) + "|" + nums(rng.randint(0, 2), 3) + "|" + nums(rng.randint(0, 3), k + 1)
                 for _ in range(k))
    return s + ";" if rng.random() < 0.1 else s

def modes():
    k = rng.randint(0, 4)
    sigs = ";".join(" ".join(str(rng.choice([0, 0, 1, 1, 2])) for _ in range(rng.randint(0, 3))) for _ in range(k))
    sites = ";".join(str(rng.randint(0, k + 1)) + "".join(" " + str(rng.choice([0, 1])) for _ in range(rng.randint(0, 3)))
                     for _ in range(rng.randint(0, 4)))
    return sigs + "|" + sites

GENS = [("ownership", ownership), ("names", subset), ("caps", subset), ("caps-transitive", graph), ("call-modes", modes)]
cases = [(mode, gen()) for mode, gen in GENS for _ in range(PER_MODE)]
want = []
with tempfile.TemporaryDirectory() as d:
    one = os.path.join(d, "w")
    bundle = bytearray(b"almide-certificate-bundle 1\nproducer proofs/gate.sh random differential\n")
    for i, (mode, w) in enumerate(cases):
        with open(one, "w") as f:
            f.write(w)
        rc = subprocess.run([checker, mode, one], capture_output=True).returncode
        if rc not in (0, 1):
            # A crash (the extracted checker exits 2 on Stack_overflow) is not
            # a verdict; counting it as REJECT would manufacture a mismatch.
            print(f"FAIL differential harness: the extracted checker gave no verdict (exit {rc}) on [{mode}] {w!r}")
            sys.exit(1)
        want.append(rc == 0)
        bundle += b"witness %s %d w%d\n" % (mode.encode(), len(w.encode()), i) + w.encode() + b"\n"
    path = os.path.join(d, "all.bundle")
    with open(path, "wb") as f:
        f.write(bundle)
    out = subprocess.run([verifier, "bundle", path], capture_output=True, text=True).stdout
got = {int(m.group(3)): m.group(1) == "ACCEPT"
       for m in re.finditer(r"^(ACCEPT|REJECT)\s+(\S+)\s+w(\d+)$", out, re.M)}
bad = [i for i in range(len(cases)) if got.get(i) != want[i]]
for i in bad[:10]:
    print(f"FAIL differential [{cases[i][0]}] {cases[i][1]!r}: extracted checker "
          f"{'ACCEPT' if want[i] else 'REJECT'}, almide-verify {got.get(i, 'no verdict')}")
if bad or len(got) != len(cases):
    print(f"FAIL almide-verify disagreed with the proven checker on {len(bad)}/{len(cases)} random witnesses")
    sys.exit(1)
for mode, _ in GENS:
    rows = [want[i] for i, c in enumerate(cases) if c[0] == mode]
    print(f"ok   [{mode}] {len(rows)} random witnesses ({sum(rows)} accept / {len(rows) - sum(rows)} reject): almide-verify agrees")
PYEOF

echo
echo "GATE OK: the kernel-proven checker re-verified per-build witnesses on THREE"
echo "properties (ownership + name totality + capability bound), AND a REAL .almd"
echo "program's ownership+name witnesses through the actual frontend (indicator ①"
echo "0→1) — with EVERY row's verdict independently certified by the Rocq KERNEL"
echo "(vm_compute on the witness bytes; binary/kernel divergence fails the build,"
echo "so the extraction pipeline is a fast path, not a trust root). Each accept ⟹"
echo "the property holds of the witnessed MIR, by the Coq theorems. (Whole-program"
echo "WASM-byte safety beyond the rc primitives is still the §3 renderer contract.)"
echo "almide-verify — the portable checker binary distributions run — gave the"
echo "extracted checker's verdict on every row and on the seeded random differential."
