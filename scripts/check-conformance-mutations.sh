#!/usr/bin/env bash
# Mutation gate for the kernel-conformance harness (Survey 4 law 1 — roc's
# ci/lambda_mono_mutation_check.sh, imported): five seeded emit bugs, each
# of which the 48-program corpus runner MUST catch. A mutant that survives
# means the corpus is not actually load-bearing for that emit path —
# "48 programs pass" only counts as evidence if 48 programs can FAIL.
#
# The mutants (proofs/conformance-mutations/). Every one sits on the path
# the corpus runner actually takes: all 48 programs (and the trace fixture)
# render natively through the v1 trust-spine (ALMIDE_VERIFIED_DEBUG=1 says
# `native: v1 trust-spine render` for each), so a mutant in the v0 walker
# (crates/almide-codegen) is dead code here — m1, m2 and m4 lived there and
# survived until they were moved onto their v1 successors:
#   m1  swap which Ok/Err match arm runs: the v1 carrier's tag read
#       (`ResTag`, render_native_op_families.rs) renders `is_ok()` for
#       `is_err()`. Successor of the v0 walker's `pattern_ok`/`pattern_err`
#       swap. Every corpus match takes the wrong arm.
#   m2  render the Err carrier's string payload BORROWED (`&str` where the
#       carrier holds an owned AlmideString) in both v1 `ResMakeErrStr` arms
#       (the moving one in render_native_loop.rs, the copying one in
#       render_native_op_families.rs): invalid Rust, E0308 — the class of
#       almide#1428, an Err construction whose rendered type disagrees with
#       its carrier. The v0 form dropped the Err turbofish; v1 spells no
#       turbofish, and dropping its `: Result<i64, AlmideString>` annotation
#       instead is an EQUIVALENT mutant (rustc infers the type from the
#       carrier's i64-typed readers — measured: 48/48 still pass), so it
#       cannot be the seeded bug.
#   m3  flip the v1 native ok-lift's carrier to Err (`ResMakeOk`, the
#       lifted effect fn's ok path — the live successor of the almide#1429
#       return override, which no program reaches since the lowering builds
#       the carrier in MIR, #3737). The corpus prints a constant per arm, so a
#       mutant must change WHICH arm runs to be observable: a lost ok VALUE
#       (`Ok(0)`) survives by construction — and so does a lost Err payload.
#   m4  reverse a function body's statement order — in the v1 lowering
#       (`lower_body_into`, lower/mod_p3.rs), the order the render emits.
#       Successor of the v0 walker's `render_fn_body_str` reversal. A body
#       whose reversed statements read a binding before it exists walls and
#       takes the standard codegen (correct output); the bodies that still
#       lower print their trace out of order, which the corpus sees. Reversing
#       the RENDER's op walk instead walls every program (48/48 fall back), so
#       it would survive for the wrong reason.
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
