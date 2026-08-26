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

# Scope (ratified 2026-08-20): ALMIDE_MUTATION_SCOPE=incremental runs only
# the mutants whose patched files intersect the work in flight (committed
# ahead of origin/greenfield + staged); CI's mutation-gate job always runs
# the FULL sweep on every push, so no verification is lost — the landing
# cycle just stops re-proving untouched mutants locally.
SCOPE="${ALMIDE_MUTATION_SCOPE:-full}"
CHANGED=""
if [ "$SCOPE" = "incremental" ]; then
  CHANGED=$( (git diff --name-only @{upstream}...HEAD 2>/dev/null; git diff --name-only --cached) | sort -u)
  echo "incremental scope; changed files:"
  echo "$CHANGED" | sed 's/^/  /'
fi

# --lib carries the direct invariant referees (the layout-order judge
# that replaced mutant 015's heap-adjacency kill after class-rounded
# allocation padded that corruption into silence).
SUITES=(--lib --test backend_parity --test fuzz_differential --test alias_semantics --test tail_calls)
fail=0

for patch in ci/mutations/*.patch; do
  name=$(basename "$patch")
  if [ "$SCOPE" = "incremental" ]; then
    patch_files=$(grep '^+++ b/' "$patch" | sed 's|+++ b/||' | sort -u)
    in_scope=0
    # A refreshed/added patch is ALWAYS in scope, even when its target
    # file is not in the diff (the stage-39 red: patches refreshed after
    # a split landed unverified because only ci/mutations/ changed).
    if echo "$CHANGED" | grep -qx "ci/mutations/$name"; then in_scope=1; fi
    for f in $patch_files; do
      if echo "$CHANGED" | grep -qx "$f"; then in_scope=1; fi
    done
    if [ "$in_scope" = 0 ]; then
      echo "skip: $name (out of scope; CI full sweep covers it)"
      continue
    fi
  fi
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
