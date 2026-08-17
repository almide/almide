#!/usr/bin/env bash
# Mutation gate for the kernel-conformance harness (Survey 4 law 1 — roc's
# ci/lambda_mono_mutation_check.sh, imported): five seeded emit bugs, each
# of which the 48-program corpus runner MUST catch. A mutant that survives
# means the corpus is not actually load-bearing for that emit path —
# "48 programs pass" only counts as evidence if 48 programs can FAIL.
#
# The mutants (proofs/conformance-mutations/):
#   m1  swap Ok/Err match-arm patterns in native codegen (wrong arm taken)
#   m2  drop the Err turbofish (reintroduces almide#1428 — invalid Rust)
#   m3  disable the v1 ok-lift return override (reintroduces almide#1429)
#   m4  reverse statement render order (trace order flips)
#   m5  emit every wasm print twice (wasm-only divergence — needs a wasm
#       runtime present, hence the hard precondition below)
#
# Protocol per mutant: git apply → run the corpus runner (which rebuilds
# the compiler) → expect FAILURE → git apply -R. The unmutated tree is the
# positive control. ~10 min of rebuilds — nightly / workflow_dispatch CI,
# not a PR gate.
set -uo pipefail
cd "$(dirname "$0")/.."

RUNNER=(cargo test --release --test kernel_conformance_test)
PATCH_DIR=proofs/conformance-mutations

# m5 diverges only on the wasm leg: without a wasm runtime the runner
# skips that leg and the mutant would "survive" for the wrong reason.
if ! command -v wasmtime >/dev/null 2>&1 && ! command -v node >/dev/null 2>&1; then
  echo "FAIL: no wasm runtime (wasmtime/node) on PATH — the wasm-leg mutant (m5) cannot be judged" >&2
  exit 1
fi

if ! git diff --quiet; then
  echo "FAIL: working tree has unstaged changes — the gate applies/reverts patches and refuses to mix with them" >&2
  exit 1
fi

applied=""
cleanup() {
  if [ -n "$applied" ]; then
    git apply -R "$applied" 2>/dev/null || echo "WARN: could not revert $applied — working tree needs manual cleanup" >&2
  fi
}
trap cleanup EXIT

echo "== positive control: unmutated tree must pass the corpus runner =="
if ! "${RUNNER[@]}" >/dev/null 2>&1; then
  echo "FAIL: the unmutated tree does not pass the corpus runner — fix that before judging mutants" >&2
  exit 1
fi

survived=()
for patch in "$PATCH_DIR"/m*.patch; do
  name=$(basename "$patch")
  echo "== mutant $name =="
  if ! git apply "$patch"; then
    echo "FAIL: $name no longer applies — the mutated code moved; regenerate the patch against the current tree" >&2
    exit 1
  fi
  applied="$patch"
  if "${RUNNER[@]}" >/dev/null 2>&1; then
    survived+=("$name")
    echo "   SURVIVED (runner stayed green under the seeded bug)"
  else
    echo "   killed"
  fi
  git apply -R "$patch"
  applied=""
done

if [ "${#survived[@]}" -gt 0 ]; then
  echo "FAIL: ${#survived[@]} mutant(s) survived: ${survived[*]}" >&2
  echo "The conformance corpus does not protect those emit paths — extend the generator or the corpus before trusting it." >&2
  exit 1
fi

echo "conformance mutation gate: 5/5 mutants killed"
