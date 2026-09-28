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
# Two consumers, two claims (#1619, #2814):
#   - mutation-sweep.yml (develop pushes, nightly) runs the FULL set with no
#     budget: "every mutant in ci/mutations/ is caught". That is the standing
#     evidence, and the only run whose verdicts stamp proofs/mutation-score.toml.
#   - ci.yml's PR job runs the INCREMENTAL scope under a time budget: "every
#     mutant this PR could have un-killed that ran was caught, and the ones
#     that did not fit are NAMED and left to the sweep". A run that stops at
#     the budget says `PARTIAL: ran N of M in-scope mutants` — it is never a
#     silent full pass and never a cancellation.
#
#   bash scripts/check-mutation-gate.sh           # run the gate
#   bash scripts/check-mutation-gate.sh --plan    # print scope + order, build nothing
#
# Run from the repo root.

set -euo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."

start=$SECONDS
PLAN_ONLY=0
if [ "${1:-}" = "--plan" ]; then PLAN_ONLY=1; fi

if ! git diff --quiet; then
  echo "FAIL: working tree must be clean before the mutation gate" >&2
  exit 2
fi

# Scope (ratified 2026-08-20; CI wiring #1619): ALMIDE_MUTATION_SCOPE=
# incremental runs only the mutants whose patched files intersect the work
# in flight. The diff base is @{upstream} locally, or ALMIDE_MUTATION_BASE
# when set (CI passes the PR base ref — the checkout's upstream is not it).
#
# A change to the SHARED KILLER INFRASTRUCTURE puts EVERY mutant in scope: a
# PR cannot un-kill a mutant whose code and killers it did not touch, and
# this rule is what closes the "except through the shared test
# infrastructure" hole. The infrastructure is what the five net suites
# (below) compile and read: their four integration-test sources, the
# harness/ and gen/ modules they include, the crate manifest, and this
# runner. It is NOT every file under crates/almide-wasm/tests/ (#2814): the
# golden baselines there (alloc / size / witness ledgers) belong to other
# tests, change on nearly every wasm PR, and used to widen every such PR to
# all 41 mutants (~62 min against a 45-min cap — #2805 and #2806 were both
# cancelled at mutant 031/032, every mutant before the cutoff caught).
SCOPE="${ALMIDE_MUTATION_SCOPE:-full}"
CHANGED=""
RANGE=""
INFRA='^(crates/almide-wasm/tests/(backend_parity|fuzz_differential|alias_semantics|tail_calls)\.rs$|crates/almide-wasm/tests/(harness|gen)/|crates/almide-wasm/Cargo\.toml$|scripts/check-mutation-gate\.sh$)'
if [ "$SCOPE" = "incremental" ]; then
  if [ -n "${ALMIDE_MUTATION_BASE:-}" ]; then
    RANGE="${ALMIDE_MUTATION_BASE}...HEAD"
    CHANGED=$(git diff --name-only "$RANGE" | sort -u)
  else
    RANGE="@{upstream}...HEAD"
    CHANGED=$( (git diff --name-only "$RANGE" 2>/dev/null; git diff --name-only --cached) | sort -u)
  fi
  echo "incremental scope; changed files:"
  echo "$CHANGED" | sed 's/^/  /'
  if echo "$CHANGED" | grep -qE "$INFRA"; then
    echo "shared killer infrastructure changed — every mutant is in scope:"
    echo "$CHANGED" | grep -E "$INFRA" | sed 's/^/  /'
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

# Budget (#2814): ALMIDE_MUTATION_BUDGET_SECS > 0 stops STARTING mutants once
# the next one would, at the slowest per-mutant cost seen so far, cross the
# budget (measured from this script's start, the unmutated build included).
# The un-run remainder is printed by name. 0 / unset = no budget (the sweep).
BUDGET="${ALMIDE_MUTATION_BUDGET_SECS:-0}"

# --lib carries the direct invariant referees (the layout-order judge
# that replaced mutant 015's heap-adjacency kill after class-rounded
# allocation padded that corruption into silence).
#
# Killer-first (#1619 item 3), REGISTRY-FREE: the suites run one at a
# time and the loop stops at the FIRST red, so a caught mutant links only
# the suites up to its killer. The verdict is unchanged: "caught" still
# means "the net goes red", because ANY suite failing IS the net failing;
# only a SURVIVOR pays for all five. The ORDER is the recorded killer
# first (#2814): the suites named in proofs/mutation-score.toml's
# `killers` field — the sweep's own measured tally, re-checked against
# every sweep by its score job, not a hand-maintained map — by kill count,
# then the rest. With the record at "backend_parity 40, lib 1" that stops
# paying a --cfg test rebuild of the crate (~30 s) before the suite that
# kills 40 of 41 mutants. The killer is still rediscovered and PRINTED on
# every run, so the evidence stays live.
SUITES_ALL="lib backend_parity fuzz_differential alias_semantics tail_calls"
recorded=""
if [ -f proofs/mutation-score.toml ]; then
  recorded=$(sed -n 's/^killers *= *"\(.*\)"$/\1/p' proofs/mutation-score.toml \
    | tr ',' '\n' | awk 'NF >= 2 { print $2, $1 }' | sort -rn -k1,1 | awk '{ print $2 }')
fi
SUITES=""
for s in $recorded $SUITES_ALL; do
  case " $SUITES_ALL " in *" $s "*) ;; *) continue ;; esac
  case " $SUITES " in *" $s "*) continue ;; esac
  SUITES="${SUITES:+$SUITES }$s"
done
echo "net suite order (recorded killer first): $SUITES"

fail=0

# Run the net suites sequentially against the currently-applied mutant;
# echoes the killer. Returns 0 = some suite went red (caught), 1 = all
# suites green (survived).
net_catches() {
  local s
  for s in $SUITES; do
    local args
    if [ "$s" = "lib" ]; then args=(--lib); else args=(--test "$s"); fi
    if ! cargo test --release -p almide-wasm --locked "${args[@]}" >/dev/null 2>&1; then
      echo "$s"
      return 0
    fi
  done
  return 1
}

# ── Selection ────────────────────────────────────────────────────────────
# In scope = the shard's mutants that pass the file-level rule above (or all
# of them in full scope). Within the scope, ORDER by precision (#2814):
#   tier 1 — the mutant's changed lines sit inside a function the PR diff
#            changed (git's rust funcname driver, `diff -W`), or its patch
#            file itself changed;
#   tier 2 — the rest of the scope (same file, other functions; or the
#            whole set when the killer infrastructure changed).
# A budget therefore spends itself on the mutants the diff is most likely
# to have un-killed, and names the rest.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
: > "$tmp/pr_ranges"
if [ -n "$RANGE" ]; then
  printf '*.rs diff=rust\n' > "$tmp/attrs"
  # New-side ranges in HEAD coordinates, widened to whole functions.
  git -c core.attributesFile="$tmp/attrs" diff -W --no-color --no-ext-diff "$RANGE" -- '*.rs' 2>/dev/null \
    | awk '
        /^\+\+\+ /     { f = ($2 == "/dev/null") ? "" : substr($2, 3); next }
        /^@@ / && f != "" {
          s = substr($3, 2); n = split(s, p, ",")
          c = p[1] + 0; d = (n > 1) ? p[2] + 0 : 1
          e = (d == 0) ? c : c + d - 1
          print f, c, e
        }' > "$tmp/pr_ranges" || true
fi

# The mutant's changed lines in HEAD coordinates: apply, read `diff -U0`'s
# old side, revert. A patch that does not apply is left for the run loop to
# report (as tier 1, so a stale mutant is named before any budget stop).
mutant_overlaps() {
  local patch=$1
  # (An empty range file would also break awk's NR == FNR idiom below.)
  if [ ! -s "$tmp/pr_ranges" ]; then return 1; fi
  if ! git apply "$patch" 2>/dev/null; then return 0; fi
  git diff -U0 --no-color --no-ext-diff \
    | awk '
        /^--- a\// { f = substr($0, 7); next }
        /^@@ / {
          s = substr($2, 2); n = split(s, p, ",")
          a = p[1] + 0; b = (n > 1) ? p[2] + 0 : 1
          e = (b == 0) ? a : a + b - 1
          print f, a, e
        }' > "$tmp/mut_ranges"
  git apply -R "$patch"
  awk 'NR == FNR { f[NR] = $1; lo[NR] = $2; hi[NR] = $3; n = NR; next }
       { for (i = 1; i <= n; i++) if (f[i] == $1 && lo[i] <= $3 && $2 <= hi[i]) hit = 1 }
       END { exit hit ? 0 : 1 }' "$tmp/pr_ranges" "$tmp/mut_ranges"
}

tier1=""
tier2=""
idx=-1
for patch in ci/mutations/*.patch; do
  name=$(basename "$patch")
  idx=$((idx + 1))
  if [ $((idx % SHARDS)) -ne "$SHARD" ]; then
    continue
  fi
  patch_changed=0
  if echo "$CHANGED" | grep -qx "ci/mutations/$name"; then patch_changed=1; fi
  if [ "$SCOPE" = "incremental" ]; then
    patch_files=$(grep '^+++ b/' "$patch" | sed 's|+++ b/||' | sort -u)
    in_scope=0
    # A refreshed/added patch is ALWAYS in scope, even when its target
    # file is not in the diff (the stage-39 red: patches refreshed after
    # a split landed unverified because only ci/mutations/ changed).
    if [ "$patch_changed" = 1 ]; then in_scope=1; fi
    for f in $patch_files; do
      if echo "$CHANGED" | grep -qx "$f"; then in_scope=1; fi
    done
    if [ "$in_scope" = 0 ]; then
      echo "skip: $name (out of scope; CI full sweep covers it)"
      continue
    fi
  fi
  if [ -z "$RANGE" ]; then
    tier2="$tier2 $patch"
  elif [ "$patch_changed" = 1 ] || mutant_overlaps "$patch"; then
    tier1="$tier1 $patch"
  else
    tier2="$tier2 $patch"
  fi
done
if ! git diff --quiet; then
  echo "FAIL: tree dirty after scope selection — a revert failed" >&2
  exit 2
fi

ORDER="$tier1 $tier2"
M=0; for _ in $ORDER; do M=$((M + 1)); done
T1=0; for _ in $tier1; do T1=$((T1 + 1)); done
echo "in scope: $M mutant(s) — $T1 in a function the diff changed (run first), $((M - T1)) elsewhere in the scope"
for p in $tier1; do echo "  tier 1: $(basename "$p")"; done
for p in $tier2; do echo "  tier 2: $(basename "$p")"; done
if [ "$PLAN_ONLY" = 1 ]; then
  echo "mutation-gate: plan only (scope=$SCOPE, budget=${BUDGET}s), nothing built"
  exit 0
fi

# Build the unmutated net once before the loop: the cold compile (~6 min on
# a PR runner) is paid here, so the per-mutant cost the budget estimates
# from is the warm one — and an unmutated tree whose net does not BUILD is
# red here, instead of reading as every mutant "caught".
if [ "$M" -gt 0 ]; then
  build_args=(--lib)
  for s in $SUITES; do [ "$s" = "lib" ] || build_args+=(--test "$s"); done
  if ! cargo test --release -p almide-wasm --locked "${build_args[@]}" --no-run >/dev/null 2>&1; then
    echo "FAIL: the unmutated net does not build — no mutant verdict is meaningful" >&2
    exit 2
  fi
  echo "unmutated net built in $((SECONDS - start)) s"
fi

ran=0
caught=0
slowest=90
deferred=""
for patch in $ORDER; do
  name=$(basename "$patch")
  if [ "$BUDGET" -gt 0 ] && [ $((SECONDS - start + slowest)) -gt "$BUDGET" ]; then
    deferred="$deferred $name"
    continue
  fi
  t0=$SECONDS
  if ! git apply "$patch" 2>/dev/null; then
    echo "FAIL: $name no longer applies — refresh the mutant, do not retire it"
    fail=1
    ran=$((ran + 1))
    continue
  fi
  if killer=$(net_catches); then
    echo "ok:   $name caught (by $killer) [$((SECONDS - t0)) s]"
    caught=$((caught + 1))
  else
    echo "FAIL: $name SURVIVED — the net did not catch this mutant"
    fail=1
  fi
  git apply -R "$patch"
  ran=$((ran + 1))
  took=$((SECONDS - t0))
  if [ "$took" -gt "$slowest" ] || [ "$ran" = 1 ]; then slowest=$took; fi
done

if ! git diff --quiet; then
  echo "FAIL: tree dirty after gate — a revert failed" >&2
  exit 2
fi
# The per-mutant `ok:` / `FAIL:` lines above are what scripts/gen-mutation-score.sh
# reads to stamp proofs/mutation-score.toml (#2152 item 5); keep their shape.
# The `deferred:` lines below are not verdicts and that reader ignores them.
if [ -n "$deferred" ]; then
  for d in $deferred; do echo "deferred: $d (not run — budget; judged by mutation-sweep on develop)"; done
  verdict="PARTIAL: ran $ran of $M in-scope mutants ($caught caught) in $((SECONDS - start)) s against a ${BUDGET} s budget; the rest are judged by mutation-sweep on develop"
  if [ "${GITHUB_ACTIONS:-}" = "true" ]; then echo "::warning title=Mutation gate PARTIAL::$verdict"; fi
else
  verdict="ran all $M in-scope mutants ($caught caught) in $((SECONDS - start)) s"
fi
echo "mutation-gate: $verdict"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### Commissioned mutation gate — scope=$SCOPE shard $SHARD/$SHARDS"
    echo
    echo "$verdict."
    [ -z "$deferred" ] || echo "Deferred to mutation-sweep on develop:$(for d in $deferred; do printf ' `%s`' "$d"; done)"
  } >> "$GITHUB_STEP_SUMMARY"
fi
echo "mutation-gate: shard $SHARD/$SHARDS scope=$SCOPE done, exit $fail"
exit $fail
