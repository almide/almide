#!/usr/bin/env bash
# Negative controls for the kernel-conformance drift gate (Survey 4 law 4,
# the swift verifier-fail pattern): prove `conformancegen --check` FIRES.
#
# A drift gate that has only ever been seen passing is indistinguishable
# from `exit 0`. This script hands it three corrupted copies of the corpus
# — a flipped byte in an .expected, a flipped byte in an .almd, a deleted
# program — and fails loudly if any of them slips through. A pristine copy
# is the positive control (so a broken harness cannot pass vacuously).
#
# Needs the Lean toolchain (elan); run from the repo root or scripts/.
# CI home: the lean-proofs job, right after the in-sync check.
set -euo pipefail
cd "$(dirname "$0")/.."

CORPUS=proofs/kernel-conformance

run_check() {
  (cd crates/almide-edit-belt && lake exe conformancegen --check "$1" >/dev/null 2>&1)
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fresh_copy() {
  rm -f "$tmp"/*
  cp "$CORPUS"/gen_* "$tmp/"
}

# Positive control: the pristine copy must pass.
fresh_copy
if ! run_check "$tmp"; then
  echo "FAIL: pristine corpus copy did not pass --check — harness broken, negative results below would be meaningless" >&2
  exit 1
fi

# Negative 1: one flipped byte in an .expected trace must fail.
fresh_copy
printf 'X' | dd of="$tmp/gen_00.expected" bs=1 seek=0 conv=notrunc status=none
if run_check "$tmp"; then
  echo "FAIL: --check passed a corrupted .expected — the drift gate is blind to trace edits" >&2
  exit 1
fi

# Negative 2: one appended byte in an .almd program must fail.
fresh_copy
printf 'X' >>"$tmp/gen_00.almd"
if run_check "$tmp"; then
  echo "FAIL: --check passed a corrupted .almd — the drift gate is blind to program edits" >&2
  exit 1
fi

# Negative 3: a deleted program must fail (no silent shrink).
fresh_copy
rm "$tmp/gen_47.almd" "$tmp/gen_47.expected"
if run_check "$tmp"; then
  echo "FAIL: --check passed a corpus with a deleted program — the gate tolerates silent shrink" >&2
  exit 1
fi

echo "conformance negative controls: 1 positive + 3 negatives all behaved"
