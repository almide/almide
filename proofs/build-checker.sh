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
"$COQC" -Q . AlmideTrust Extract.v >/dev/null

echo "== link the runnable checker (extracted check_cert, parser internalized) =="
ocamlopt -w -a checker.mli checker.ml driver.ml -o checker

echo "== run the proven checker on real certificates (one object per line) =="
printf 'ID\nIIDD\n' > /tmp/balanced.cert     # two balanced objects → ACCEPT
printf 'IIDD\nIDD\n' > /tmp/double_free.cert  # 2nd object double-frees → REJECT
printf 'ID\nIID\n'  > /tmp/leak.cert          # 2nd object leaks → REJECT
printf 'IR\nIADR\n' > /tmp/perceus.cert       # perceus: reuse-release + alias/drop/reuse → ACCEPT
printf 'R\n'        > /tmp/reuse_uaf.cert      # reuse with nothing owned → REJECT
printf 'IIDD\n'    > /tmp/stack.cert          # operand-STACK balance (push push pop pop) ≡ the same fold → ACCEPT
printf 'IDD\n'     > /tmp/stack_uflow.cert     # operand-stack UNDERFLOW (pop below empty) → REJECT

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
run /tmp/stack.cert 0
run /tmp/stack_uflow.cert 1

echo
echo "CHECKER OK: the kernel-proven check accepts the balanced certificate and"
echo "rejects double-free / leak — the proof now runs on real bytes."
