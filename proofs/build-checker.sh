#!/usr/bin/env bash
# Build the runnable certificate checker FROM the Coq proof: compile the proof,
# extract `check` to OCaml, link the thin tokenizer, and run it on a balanced
# and a faulty certificate. Demonstrates the proven checker validating real
# bytes (accept ⟹ no double-free ∧ no leak, by check_sound).
set -euo pipefail
# Private scratch dir per run: fixed /tmp paths let two concurrent runs overwrite each
# other's witnesses and tamper files, and a drill then fails on the other run's bytes.
GATE_TMP="$(mktemp -d "${TMPDIR:-/tmp}/almide-checker.XXXXXX")"; export GATE_TMP; trap 'rm -rf "$GATE_TMP"' EXIT
cd "$(dirname "$0")"

COQC="${COQC:-$(command -v coqc)}"

echo "== compile + extract the proven checker =="
"$COQC" -Q . AlmideTrust Subset.v >/dev/null
"$COQC" -Q . AlmideTrust OwnershipChecker.v >/dev/null
"$COQC" -Q . AlmideTrust NameTotality.v >/dev/null
"$COQC" -Q . AlmideTrust CapabilityBound.v >/dev/null
"$COQC" -Q . AlmideTrust CapabilityReach.v >/dev/null
"$COQC" -Q . AlmideTrust CallModes.v >/dev/null
"$COQC" -Q . AlmideTrust MemorySafe.v >/dev/null
"$COQC" -Q . AlmideTrust SpatialCert.v >/dev/null
"$COQC" -Q . AlmideTrust BoundsCert.v >/dev/null
"$COQC" -Q . AlmideTrust Certified.v >/dev/null
"$COQC" -Q . AlmideTrust InitCert.v >/dev/null
"$COQC" -Q . AlmideTrust Extract.v >/dev/null

echo "== link the runnable checker (extracted check_cert, parser internalized) =="
ocamlopt -w -a checker.mli checker.ml driver.ml -o checker

echo "== run the proven checker on real certificates (one object per line) =="
printf 'ID\nIIDD\n' > $GATE_TMP/balanced.cert     # two balanced objects → ACCEPT
printf 'IIDD\nIDD\n' > $GATE_TMP/double_free.cert  # 2nd object double-frees → REJECT
printf 'ID\nIID\n'  > $GATE_TMP/leak.cert          # 2nd object leaks → REJECT
printf 'IR\nIADR\n' > $GATE_TMP/perceus.cert       # perceus: reuse-release on a UNIQUE object (rc=1) → ACCEPT
printf 'R\n'        > $GATE_TMP/reuse_uaf.cert      # reuse with nothing owned (rc=0) → REJECT
printf 'IARD\n'    > $GATE_TMP/shared_reuse.cert    # reuse of a SHARED object (rc=2): balances but unsound → REJECT
printf 'IIDD\n'    > $GATE_TMP/stack.cert          # operand-STACK balance (push push pop pop) ≡ the same fold → ACCEPT
printf 'IDD\n'     > $GATE_TMP/stack_uflow.cert     # operand-stack UNDERFLOW (pop below empty) → REJECT
printf 'I(DI)M\n'  > $GATE_TMP/loop_acc.cert        # heap-loop-carried accumulator slot: acquire, loop[drop-old acquire-new], move-out → ACCEPT
printf 'I(I)M\n'   > $GATE_TMP/loop_leak.cert       # loop body LEAKS (acquire each iter, never release) → REJECT
printf 'I(D)M\n'   > $GATE_TMP/loop_drain.cert      # loop body DRAINS (release each iter, never acquire) → REJECT
printf 'I[ID|]M\n' > $GATE_TMP/filter_slot.cert     # conditional-loop (filter) slot: then[drop-old acquire-new] / else[] both net 0 → ACCEPT
printf 'I[I|]M\n'  > $GATE_TMP/filter_then_leak.cert # filter THEN branch leaks (net +1) → REJECT
printf 'I[ID|D]M\n'> $GATE_TMP/filter_else_drain.cert # filter ELSE branch drains (net −1) → REJECT
printf 'IBD\n'     > $GATE_TMP/borrow_live.cert      # 5b: borrow of a LIVE owned object (+0) → ACCEPT
printf 'IDB\n'     > $GATE_TMP/borrow_uaf.cert       # 5b: borrow AFTER the last release = use-after-free → REJECT
printf 'B\n'       > $GATE_TMP/borrow_nothing.cert   # 5b: borrow with nothing ever owned → REJECT
printf 'I{I|I}DD\n'> $GATE_TMP/branch_agree.cert     # 5a: one-shot branch, arms AGREE at net +1 (heap-result if) → ACCEPT
printf 'I{I|}D\n'  > $GATE_TMP/branch_disagree.cert  # 5a: arms DISAGREE (+1 vs 0) → REJECT
printf '{I|D}\n'   > $GATE_TMP/branch_cross.cert     # 5a: cross-arm compensation (flat-balanced, runtime-unsafe) → REJECT
printf 'IDAM\n'    > $GATE_TMP/resurrect.cert        # #3229: an OWNED object freed, then aliased + moved out (balanced, use-after-free) → REJECT
printf 'AMAM\n'    > $GATE_TMP/param_realias.cert    # #3229: a borrowed param's line re-aliased at 0 (the caller holds it) → ACCEPT

run() { # path expected_exit
  set +e; ./checker ownership "$1" >$GATE_TMP/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat $GATE_TMP/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat $GATE_TMP/checker.out))"; exit 1; fi
}
run $GATE_TMP/balanced.cert 0
run $GATE_TMP/double_free.cert 1
run $GATE_TMP/leak.cert 1
run $GATE_TMP/perceus.cert 0
run $GATE_TMP/reuse_uaf.cert 1
run $GATE_TMP/shared_reuse.cert 1
run $GATE_TMP/stack.cert 0
run $GATE_TMP/stack_uflow.cert 1
run $GATE_TMP/loop_acc.cert 0
run $GATE_TMP/loop_leak.cert 1
run $GATE_TMP/loop_drain.cert 1
run $GATE_TMP/filter_slot.cert 0
run $GATE_TMP/filter_then_leak.cert 1
run $GATE_TMP/filter_else_drain.cert 1
run $GATE_TMP/borrow_live.cert 0
run $GATE_TMP/borrow_uaf.cert 1
run $GATE_TMP/borrow_nothing.cert 1
run $GATE_TMP/branch_agree.cert 0
run $GATE_TMP/branch_disagree.cert 1
run $GATE_TMP/branch_cross.cert 1
run $GATE_TMP/resurrect.cert 1
run $GATE_TMP/param_realias.cert 0

# TRANSITIVE capability witness (call graph): functions ';'-separated, each
# `allowed|direct|callee-indices`. accept ⟹ every function's transitive reach ⊆ declared.
printf '1 2|2|1;1|1|' > $GATE_TMP/caps_tr_ok.cert    # main{allow 1,2; use 2; calls helper} helper{allow 1; use 1} → ACCEPT
printf '1 2|2|1;0|0|' > $GATE_TMP/caps_tr_bad.cert    # helper reaches undeclared network (cap 0) ∉ main's allowlist → REJECT
runt() { # path expected_exit  (caps-transitive mode)
  set +e; ./checker caps-transitive "$1" >$GATE_TMP/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat $GATE_TMP/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat $GATE_TMP/checker.out))"; exit 1; fi
}
runt $GATE_TMP/caps_tr_ok.cert 0
runt $GATE_TMP/caps_tr_bad.cert 1

# CALL-MODE signature witness (brick 2c): `<sigs>|<sites>`, functions/sites
# ';'-separated, modes as nats (0 = borrow, 1 = move), each site
# `<callee-index> <actual modes…>`. accept ⟹ every call site used exactly its
# callee's declared param modes (the compositionality ground fact).
printf '0 1;1|1 1' > $GATE_TMP/modes_ok.cert    # fn1 declares [move]; a site calls fn1 with [move] → ACCEPT
printf '0|0 1'     > $GATE_TMP/modes_bad.cert    # fn0 declares [borrow]; a site calls fn0 with [move] → REJECT (the double-free pairing)
printf '0|5 0'     > $GATE_TMP/modes_unknown.cert # a site names an out-of-range callee → conservative REJECT
runm() { # path expected_exit  (call-modes mode)
  set +e; ./checker call-modes "$1" >$GATE_TMP/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat $GATE_TMP/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat $GATE_TMP/checker.out))"; exit 1; fi
}
runm $GATE_TMP/modes_ok.cert 0
runm $GATE_TMP/modes_bad.cert 1
runm $GATE_TMP/modes_unknown.cert 1

# BOUNDS witness (#4005): `;`-separated straight-line stretches, each a bounds
# program (c = check, a = read, w = write, s = the slot's index changed, L{..}
# loop, B{..|..} branch). accept ⟹ every run is Spatial (check_bounds_cert_sound).
printf 'c0 a0 c1 a1 w0 w1' > $GATE_TMP/bounds_flip.cert   # #3345 fannkuch flip: two checks decide four accesses → ACCEPT
printf 'c0 s0 a0'          > $GATE_TMP/bounds_stale.cert  # index changed after the check, check elided → REJECT
printf 'c0 ; a0'           > $GATE_TMP/bounds_cross.cert  # a fact carried across a stretch boundary → REJECT
printf 'c0 a0 x'           > $GATE_TMP/bounds_junk.cert   # malformed tail → REJECT (never read as a prefix)
runb() { # path expected_exit  (bounds mode)
  set +e; ./checker bounds "$1" >$GATE_TMP/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat $GATE_TMP/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat $GATE_TMP/checker.out))"; exit 1; fi
}
runb $GATE_TMP/bounds_flip.cert 0
runb $GATE_TMP/bounds_stale.cert 1
runb $GATE_TMP/bounds_cross.cert 1
runb $GATE_TMP/bounds_junk.cert 1

# INIT witness (#4013): `;`-separated construction sites, each `h H w W n N r R`
# then the stored slots `s I`. accept ⟹ every site's window leaves its block
# initialized over its whole extent (check_init_cert_sound).
printf 'h12 w8 n3 r0 s0 s1 s2' > $GATE_TMP/init_literal.cert  # a three-element list literal, every slot stored → ACCEPT
printf 'h12 w8 n3 r0 s0 s2'    > $GATE_TMP/init_missing.cert  # slot 1 never stored → REJECT
printf 'h12 w8 n2 r0 s0 s1 s2' > $GATE_TMP/init_past.cert     # a store past the last slot → REJECT
printf 'h12 w8 n1 r0 s0 x'     > $GATE_TMP/init_junk.cert     # malformed tail → REJECT
runi() { # path expected_exit  (init mode)
  set +e; ./checker init "$1" >$GATE_TMP/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat $GATE_TMP/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat $GATE_TMP/checker.out))"; exit 1; fi
}
runi $GATE_TMP/init_literal.cert 0
runi $GATE_TMP/init_missing.cert 1
runi $GATE_TMP/init_past.cert 1
runi $GATE_TMP/init_junk.cert 1
printf 'h12 w8 v4 r0 f4 t8'  > $GATE_TMP/init_fill.cert      # a range's loop fill: counts down the count it was sized from, one slot per step → ACCEPT
printf 'h12 w8 v4 r0 f4 t16' > $GATE_TMP/init_fill_skip.cert # the cursor skips a slot per step → REJECT
runi $GATE_TMP/init_fill.cert 0
runi $GATE_TMP/init_fill_skip.cert 1

echo
echo "CHECKER OK: the kernel-proven check accepts the balanced certificate and"
echo "rejects double-free / leak (incl. heap-loop-carried accumulator certs via the"
echo "loop-aware check_cert_lc) — the proof now runs on real bytes. The transitive"
echo "capability checker (check_prog_cert) accepts a bounded call graph and rejects"
echo "one whose callee reaches an undeclared capability."
