#!/usr/bin/env bash
# Build the runnable certificate checker FROM the Coq proof: compile the proof,
# extract `check` to OCaml, link the thin tokenizer, and run it on a balanced
# and a faulty certificate. Demonstrates the proven checker validating real
# bytes (accept ⟹ no double-free ∧ no leak, by check_sound).
set -euo pipefail
cd "$(dirname "$0")"

COQC="${COQC:-$(command -v coqc)}"

echo "== compile + extract the proven checker =="
"$COQC" -Q . AlmideTrust Subset.v >/dev/null
"$COQC" -Q . AlmideTrust OwnershipChecker.v >/dev/null
"$COQC" -Q . AlmideTrust NameTotality.v >/dev/null
"$COQC" -Q . AlmideTrust CapabilityBound.v >/dev/null
"$COQC" -Q . AlmideTrust CapabilityReach.v >/dev/null
"$COQC" -Q . AlmideTrust CallModes.v >/dev/null
"$COQC" -Q . AlmideTrust Extract.v >/dev/null

echo "== link the runnable checker (extracted check_cert, parser internalized) =="
ocamlopt -w -a checker.mli checker.ml driver.ml -o checker

echo "== run the proven checker on real certificates (one object per line) =="
printf 'ID\nIIDD\n' > /tmp/balanced.cert     # two balanced objects → ACCEPT
printf 'IIDD\nIDD\n' > /tmp/double_free.cert  # 2nd object double-frees → REJECT
printf 'ID\nIID\n'  > /tmp/leak.cert          # 2nd object leaks → REJECT
printf 'IR\nIADR\n' > /tmp/perceus.cert       # perceus: reuse-release on a UNIQUE object (rc=1) → ACCEPT
printf 'R\n'        > /tmp/reuse_uaf.cert      # reuse with nothing owned (rc=0) → REJECT
printf 'IARD\n'    > /tmp/shared_reuse.cert    # reuse of a SHARED object (rc=2): balances but unsound → REJECT
printf 'IIDD\n'    > /tmp/stack.cert          # operand-STACK balance (push push pop pop) ≡ the same fold → ACCEPT
printf 'IDD\n'     > /tmp/stack_uflow.cert     # operand-stack UNDERFLOW (pop below empty) → REJECT
printf 'I(DI)M\n'  > /tmp/loop_acc.cert        # heap-loop-carried accumulator slot: acquire, loop[drop-old acquire-new], move-out → ACCEPT
printf 'I(I)M\n'   > /tmp/loop_leak.cert       # loop body LEAKS (acquire each iter, never release) → REJECT
printf 'I(D)M\n'   > /tmp/loop_drain.cert      # loop body DRAINS (release each iter, never acquire) → REJECT
printf 'I[ID|]M\n' > /tmp/filter_slot.cert     # conditional-loop (filter) slot: then[drop-old acquire-new] / else[] both net 0 → ACCEPT
printf 'I[I|]M\n'  > /tmp/filter_then_leak.cert # filter THEN branch leaks (net +1) → REJECT
printf 'I[ID|D]M\n'> /tmp/filter_else_drain.cert # filter ELSE branch drains (net −1) → REJECT

run() { # path expected_exit
  set +e; ./checker ownership "$1" >/tmp/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat /tmp/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat /tmp/checker.out))"; exit 1; fi
}
run /tmp/balanced.cert 0
run /tmp/double_free.cert 1
run /tmp/leak.cert 1
run /tmp/perceus.cert 0
run /tmp/reuse_uaf.cert 1
run /tmp/shared_reuse.cert 1
run /tmp/stack.cert 0
run /tmp/stack_uflow.cert 1
run /tmp/loop_acc.cert 0
run /tmp/loop_leak.cert 1
run /tmp/loop_drain.cert 1
run /tmp/filter_slot.cert 0
run /tmp/filter_then_leak.cert 1
run /tmp/filter_else_drain.cert 1

# TRANSITIVE capability witness (call graph): functions ';'-separated, each
# `allowed|direct|callee-indices`. accept ⟹ every function's transitive reach ⊆ declared.
printf '1 2|2|1;1|1|' > /tmp/caps_tr_ok.cert    # main{allow 1,2; use 2; calls helper} helper{allow 1; use 1} → ACCEPT
printf '1 2|2|1;0|0|' > /tmp/caps_tr_bad.cert    # helper reaches undeclared network (cap 0) ∉ main's allowlist → REJECT
runt() { # path expected_exit  (caps-transitive mode)
  set +e; ./checker caps-transitive "$1" >/tmp/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat /tmp/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat /tmp/checker.out))"; exit 1; fi
}
runt /tmp/caps_tr_ok.cert 0
runt /tmp/caps_tr_bad.cert 1

# CALL-MODE signature witness (brick 2c): `<sigs>|<sites>`, functions/sites
# ';'-separated, modes as nats (0 = borrow, 1 = move), each site
# `<callee-index> <actual modes…>`. accept ⟹ every call site used exactly its
# callee's declared param modes (the compositionality ground fact).
printf '0 1;1|1 1' > /tmp/modes_ok.cert    # fn1 declares [move]; a site calls fn1 with [move] → ACCEPT
printf '0|0 1'     > /tmp/modes_bad.cert    # fn0 declares [borrow]; a site calls fn0 with [move] → REJECT (the double-free pairing)
printf '0|5 0'     > /tmp/modes_unknown.cert # a site names an out-of-range callee → conservative REJECT
runm() { # path expected_exit  (call-modes mode)
  set +e; ./checker call-modes "$1" >/tmp/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat /tmp/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat /tmp/checker.out))"; exit 1; fi
}
runm /tmp/modes_ok.cert 0
runm /tmp/modes_bad.cert 1
runm /tmp/modes_unknown.cert 1

echo
echo "CHECKER OK: the kernel-proven check accepts the balanced certificate and"
echo "rejects double-free / leak (incl. heap-loop-carried accumulator certs via the"
echo "loop-aware check_cert_lc) — the proof now runs on real bytes. The transitive"
echo "capability checker (check_prog_cert) accepts a bounded call graph and rejects"
echo "one whose callee reaches an undeclared capability."
