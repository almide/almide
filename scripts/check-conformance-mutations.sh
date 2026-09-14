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
#   m5  emit every wasm print twice in the STRUCTURAL emitter (wasm-only
#       divergence — needs a wasm runtime present, hence the hard
#       precondition below). It lives in crates/almide-wasm because that is
#       the leg the runner builds on (the default since 0.60 — 48/48 corpus
#       programs take it); its incumbent-side form (almide-mir
#       render_wasm_c.rs) survived seven nights after the route flip and was
#       retired in #1845. A mutant on a leg the corpus never builds is not
#       evidence, and the incumbent carries none.
#
# Protocol per mutant: git apply → build the runner (a mutant that does not
# even compile is killed by rustc, not by the corpus, and is reported as its
# own failure) → run the corpus runner → expect FAILURE → git apply -R. The
# unmutated tree is the positive control. ~10 min of rebuilds — nightly /
# workflow_dispatch CI, not a PR gate.
#
# Every build here goes to the gate's OWN target dir (#1845, second effect):
# the runner shells out to the `almide` binary cargo rebuilds for it, and
# under the shared target/ the last mutant's binary outlived the gate —
# sources reverted, target/release/almide not — so every later gate in the
# same tree measured a compiler that doubled its wasm prints (three phantom
# regressions). target/mutations is read by nothing else, so a mutant binary
# left there is inert by construction, and target/release is never touched.
#
# The dir is passed as `--target-dir`, NEVER exported as CARGO_TARGET_DIR
# (#2207): the runner shells out to `almide run`, which builds each corpus
# program with cargo in its own scratch project and looks for the binary at
# <scratch>/target/debug/almide-out. An exported CARGO_TARGET_DIR is
# inherited by that cargo too, which then writes the binary under
# <scratch>/target/mutations/ instead — "expected binary not found", 48/48
# programs, on every cold-cache run (CI, eleven nights from 2026-09-03; a
# laptop with a warm IR-keyed build cache never invokes cargo and so never
# saw it). `--target-dir` reaches only the cargo that builds the runner.
set -uo pipefail
cd "$(dirname "$0")/.."

MUTATIONS_TARGET_DIR="${CARGO_TARGET_DIR:-target}/mutations"
unset CARGO_TARGET_DIR
RUNNER=(cargo test --release --target-dir "$MUTATIONS_TARGET_DIR" --test kernel_conformance_test)
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

# Every runner invocation is logged in full (the positive control to the
# console as well): a gate that swallows the runner's output can only say
# "did not pass", and that verdict stood unexplained for eleven nights
# (2026-09-03 → 09-13) because nobody could see WHICH program diverged.
# On a positive-control failure the log's tail also lands in the job
# summary when the gate runs under GitHub Actions.
RUNNER_LOG="$MUTATIONS_TARGET_DIR/conformance-mutations.log"
mkdir -p "$MUTATIONS_TARGET_DIR"
run_runner() {
  # $1 = log file, rest = extra runner args; stdout+stderr go to the log.
  local log="$1"
  shift
  "${RUNNER[@]}" "$@" >"$log" 2>&1
}
report_failure() {
  # $1 = log file, $2 = headline for the job summary.
  local log="$1" title="$2"
  echo "---- runner output (last 80 lines of $log) ----" >&2
  tail -n 80 "$log" >&2
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
      echo "## conformance mutation gate: $title"
      echo
      echo '```'
      tail -n 80 "$log"
      echo '```'
    } >>"$GITHUB_STEP_SUMMARY"
  fi
}

echo "== positive control: unmutated tree must pass the corpus runner (target dir $MUTATIONS_TARGET_DIR) =="
if ! run_runner "$RUNNER_LOG"; then
  echo "FAIL: the unmutated tree does not pass the corpus runner — fix that before judging mutants" >&2
  report_failure "$RUNNER_LOG" "positive control failed (unmutated tree)"
  exit 1
fi
grep -E "^kernel_conformance:|^test result:" "$RUNNER_LOG" || true

survived=()
unbuilt=()
total=0
for patch in "$PATCH_DIR"/m*.patch; do
  name=$(basename "$patch")
  total=$((total + 1))
  echo "== mutant $name =="
  if ! git apply "$patch"; then
    echo "FAIL: $name no longer applies — the mutated code moved; regenerate the patch against the current tree" >&2
    exit 1
  fi
  applied="$patch"
  mutant_log="$MUTATIONS_TARGET_DIR/conformance-mutations-${name%.patch}.log"
  if ! run_runner "$mutant_log" --no-run; then
    unbuilt+=("$name")
    echo "   DID NOT BUILD (rustc rejected the mutant — that is not a corpus kill)"
    report_failure "$mutant_log" "$name did not build"
  elif run_runner "$mutant_log"; then
    survived+=("$name")
    echo "   SURVIVED (runner stayed green under the seeded bug)"
  else
    echo "   killed"
  fi
  git apply -R "$patch"
  applied=""
done

if [ "$total" -eq 0 ]; then
  echo "FAIL: no mutants found under $PATCH_DIR — an empty net catches nothing" >&2
  exit 1
fi

if [ "${#unbuilt[@]}" -gt 0 ]; then
  echo "FAIL: ${#unbuilt[@]} mutant(s) did not build: ${unbuilt[*]}" >&2
  echo "Re-seed them so the compiler builds and the CORPUS is what kills them." >&2
  exit 1
fi

if [ "${#survived[@]}" -gt 0 ]; then
  echo "FAIL: ${#survived[@]} mutant(s) survived: ${survived[*]}" >&2
  echo "The conformance corpus does not protect those emit paths — extend the generator or the corpus before trusting it." >&2
  exit 1
fi

echo "conformance mutation gate: $total/$total mutants killed (built under $MUTATIONS_TARGET_DIR)"
