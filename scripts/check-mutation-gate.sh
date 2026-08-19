#!/usr/bin/env bash
# V-6 (RESEARCH-verification.md, roc's *_mutation_check shape): the
# verification of the verifier, as a STANDING gate instead of per-slice
# manual evidence. Each pre-authored mutant in ci/mutations/ is applied to
# the wasm backend; the net (release-shape parity + differential fuzz +
# alias referee) must go RED; the mutant is then reverted.
#
# Doctrine:
#   - a mutant that SURVIVES (net stays green) fails this gate — the net
#     lost a tooth;
#   - a patch that no longer APPLIES also fails — code drift must refresh
#     the mutant, never silently retire it (roc discipline).
#
# Run from the repo root: bash scripts/check-mutation-gate.sh

set -euo pipefail
cd "$(dirname "$0")/.."

if ! git diff --quiet; then
  echo "FAIL: working tree must be clean before the mutation gate" >&2
  exit 2
fi

SUITES=(--test backend_parity --test fuzz_differential --test alias_semantics --test tail_calls)
fail=0

for patch in ci/mutations/*.patch; do
  name=$(basename "$patch")
  if ! git apply "$patch" 2>/dev/null; then
    echo "FAIL: $name no longer applies — refresh the mutant, do not retire it"
    fail=1
    continue
  fi
  if cargo test --release -p almide-wasm --locked "${SUITES[@]}" >/dev/null 2>&1; then
    echo "FAIL: $name SURVIVED — the net did not catch this mutant"
    fail=1
  else
    echo "ok:   $name caught"
  fi
  git apply -R "$patch"
done

if ! git diff --quiet; then
  echo "FAIL: tree dirty after gate — a revert failed" >&2
  exit 2
fi
exit $fail
