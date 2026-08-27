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
export LC_ALL=C
cd "$(dirname "$0")/.."

if ! git diff --quiet; then
  echo "FAIL: working tree must be clean before the mutation gate" >&2
  exit 2
fi

# Scope (ratified 2026-08-20; CI wiring #1619): ALMIDE_MUTATION_SCOPE=
# incremental runs only the mutants whose patched files intersect the work
# in flight. The diff base is @{upstream} locally, or ALMIDE_MUTATION_BASE
# when set (CI passes the PR base ref — the checkout's upstream is not it).
# A change to the SHARED KILLER INFRASTRUCTURE (the killer suites under
# crates/almide-wasm/tests/, or this runner itself) puts EVERY mutant in
# scope: a PR cannot un-kill a mutant whose code and killers it did not
# touch, and this rule is what closes the "except through the shared test
# infrastructure" hole. The FULL sweep's standing evidence is the
# mutation-sweep workflow (develop pushes uncancelable + nightly), so no
# verification is lost — PRs just stop re-proving untouched mutants.
SCOPE="${ALMIDE_MUTATION_SCOPE:-full}"
CHANGED=""
if [ "$SCOPE" = "incremental" ]; then
  if [ -n "${ALMIDE_MUTATION_BASE:-}" ]; then
    CHANGED=$(git diff --name-only "${ALMIDE_MUTATION_BASE}...HEAD" | sort -u)
  else
    CHANGED=$( (git diff --name-only @{upstream}...HEAD 2>/dev/null; git diff --name-only --cached) | sort -u)
  fi
  echo "incremental scope; changed files:"
  echo "$CHANGED" | sed 's/^/  /'
  if echo "$CHANGED" | grep -qE '^(crates/almide-wasm/tests/|scripts/check-mutation-gate\.sh$)'; then
    echo "shared killer infrastructure changed — every mutant is in scope"
    SCOPE=full
  fi
fi

# Sharding (#1619): the full sweep fans out across CI jobs by position —
# mutant i runs on the job where i % ALMIDE_MUTATION_SHARDS ==
# ALMIDE_MUTATION_SHARD. Positional, not weighted: every mutant costs one
# rebuild + one net run, so modulo IS the balanced split. Defaults run
# everything in one process (local behavior unchanged).
SHARDS="${ALMIDE_MUTATION_SHARDS:-1}"
SHARD="${ALMIDE_MUTATION_SHARD:-0}"

# --lib carries the direct invariant referees (the layout-order judge
# that replaced mutant 015's heap-adjacency kill after class-rounded
# allocation padded that corruption into silence).
SUITES=(--lib --test backend_parity --test fuzz_differential --test alias_semantics --test tail_calls)
fail=0

idx=-1
for patch in ci/mutations/*.patch; do
  name=$(basename "$patch")
  idx=$((idx + 1))
  if [ $((idx % SHARDS)) -ne "$SHARD" ]; then
    continue
  fi
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
