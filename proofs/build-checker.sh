#!/usr/bin/env bash
# Build the runnable certificate checker FROM the Coq proof: compile the proof,
# extract `check` to OCaml, link the thin tokenizer, and run it on a balanced
# and a faulty certificate. Demonstrates the proven checker validating real
# bytes (accept ⟹ no double-free ∧ no leak, by check_sound).
set -euo pipefail
cd "$(dirname "$0")"

COQC="${COQC:-$(command -v coqc)}"

echo "== compile + extract the proven checker =="
"$COQC" -Q . AlmideTrust OwnershipChecker.v >/dev/null
"$COQC" -Q . AlmideTrust Extract.v >/dev/null

echo "== link the runnable checker (extracted check + tokenizer) =="
ocamlopt -w -a checker.mli checker.ml driver.ml -o checker

echo "== run the proven checker on real certificates =="
printf 'IIDD\n' > /tmp/balanced.cert       # +1 +1 -1 -1  → balanced
printf 'IDD\n'  > /tmp/double_free.cert     # +1 -1 -1     → double-free
printf 'IID\n'  > /tmp/leak.cert            # +1 +1 -1     → leak

run() { # path expected_exit
  set +e; ./checker "$1" >/tmp/checker.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then echo "ok   $(basename "$1"): $(cat /tmp/checker.out) (exit $rc)";
  else echo "FAIL $(basename "$1"): got exit $rc want $2 ($(cat /tmp/checker.out))"; exit 1; fi
}
run /tmp/balanced.cert 0
run /tmp/double_free.cert 1
run /tmp/leak.cert 1

echo
echo "CHECKER OK: the kernel-proven check accepts the balanced certificate and"
echo "rejects double-free / leak — the proof now runs on real bytes."
