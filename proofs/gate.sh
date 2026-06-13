#!/usr/bin/env bash
# END-TO-END FLIGHT-GRADE GATE (critical-path brick 3): the UNTRUSTED compiler
# (almide-mir) emits an ownership certificate per build; the KERNEL-PROVEN
# checker re-verifies it. accept ⟹ the build is memory-safe (no double-free, no
# leak) — by `check_all_sound`, proven in Coq. The compiler may be buggy; if its
# certificate is wrong, the proven checker rejects.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "== build the kernel-proven checker from the Coq proof =="
"$ROOT/proofs/build-checker.sh" >/dev/null

emit() { (cd "$ROOT" && cargo run -q -p almide-mir --example emit_cert -- "$1"); }

run() { # scenario expected_exit
  emit "$1" > /tmp/compiler.cert
  set +e; "$ROOT/proofs/checker" /tmp/compiler.cert >/tmp/gate.out 2>&1; local rc=$?; set -e
  if [ "$rc" -eq "$2" ]; then
    echo "ok   $1: compiler cert $(cat /tmp/compiler.cert | tr '\n' '|') -> $(cat /tmp/gate.out)"
  else
    echo "FAIL $1: got exit $rc want $2 ($(cat /tmp/gate.out))"; exit 1
  fi
}

echo "== compiler output  ⊳  proven checker =="
run balanced 0
run leak 1

echo
echo "GATE OK: the untrusted compiler's ownership certificate was re-verified by"
echo "the kernel-proven checker (accept ⟹ memory-safe, by check_all_sound)."
