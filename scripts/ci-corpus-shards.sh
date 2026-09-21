#!/usr/bin/env bash
# The fixture-range sharding of the corpus giants (#2381): the two steps that
# make `ALMIDE_CORPUS_SHARD=k/N` safe, run in the coverage job after every
# `Test Rust (solo <leg> k/N)` job has uploaded its partials.
#
# A partition that drops a fixture does not fail — it goes GREEN, faster,
# with less tested. So (1) every sharded gate writes the fixture list it
# ACTUALLY WALKED (not a list computed here from the same formula — that
# would only prove the formula agrees with itself), and this script asserts
# the N lists are a partition of the corpus: nothing missing, nothing walked
# twice. And (2) the two gates whose assertion is a whole-corpus count or a
# whole-corpus NAME set (run_parity's shrink-only ceilings; the bridge-
# fallback ledger) never judged a partial against the whole in their shard —
# they left partials, and `merge/N` here sums / unions them and judges with
# the gate's own code (crates/almide-corpus/src/lib.rs says why per gate).
#
#   bash scripts/ci-corpus-shards.sh --coverage <partials-dir> <N>
#   bash scripts/ci-corpus-shards.sh --merge    <partials-dir> <N> <nextest-archive>
#   bash scripts/ci-corpus-shards.sh --gates
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2

# The gates that walk spec/wasm_cross directly (one fixture list each) and
# the one that walks the run manifest. Keep in step with the `leg` matrix of
# `test-rust-solo` in .github/workflows/ci.yml: a gate listed there but not
# here is sharded and never covered.
CROSS_GATES="wasm_runtime_cross_target wasm_runtime_interp_oracle wasm_runtime_opt_parity interp_abstain_ledger interp_bridge_fallback_ledger"
MANIFEST_GATE="run_parity"
MANIFEST="crates/almide-spine/tests/golden/spec-run-manifest.txt"

expected_cross() {
  ls spec/wasm_cross/*.almd | xargs -n1 basename | sed 's/\.almd$//' | LC_ALL=C sort
}
expected_manifest() {
  grep -v '^[[:space:]]*#' "$MANIFEST" | grep -v '^[[:space:]]*$' | cut -f3 | LC_ALL=C sort
}

# cover <gate> <dir> <N> <expected-file>
cover() {
  local gate="$1" dir="$2" n="$3" want="$4" k f
  local union; union="$(mktemp)"
  for k in $(seq 1 "$n"); do
    f="$dir/$gate.fixtures.$k-of-$n.txt"
    if [ ! -f "$f" ]; then
      echo "CORPUS SHARD FAIL: $gate shard $k/$n left no fixture list ($f) — the job did not run its gate, or ALMIDE_CORPUS_SHARD_DIR was unset" >&2
      return 1
    fi
    printf '  %-32s shard %s/%s: %s fixture(s)\n' "$gate" "$k" "$n" "$(wc -l < "$f" | tr -d ' ')"
    cat "$f" >> "$union"
  done
  local dupes; dupes="$(LC_ALL=C sort "$union" | uniq -d)"
  if [ -n "$dupes" ]; then
    echo "CORPUS SHARD FAIL: $gate — fixture(s) walked by more than one shard:" >&2
    echo "$dupes" | sed 's/^/    /' >&2
    return 1
  fi
  if ! diff -u "$want" <(LC_ALL=C sort "$union") >&2; then
    echo "CORPUS SHARD FAIL: $gate — the $n shards' union is not the corpus. A line" >&2
    echo "with '-' is a fixture NO shard walked (a green run that tests less);" >&2
    echo "'+' is a name outside the corpus." >&2
    return 1
  fi
  rm -f "$union"
}

case "${1:-}" in
  --gates)
    echo "$CROSS_GATES $MANIFEST_GATE"
    ;;
  --coverage)
    dir="$2"; n="$3"
    [ "$n" -ge 2 ] || { echo "N must be >= 2 (N=1 is the unsharded gate)" >&2; exit 2; }
    want_cross="$(mktemp)"; expected_cross > "$want_cross"
    want_manifest="$(mktemp)"; expected_manifest > "$want_manifest"
    [ -s "$want_cross" ] && [ -s "$want_manifest" ] || { echo "empty corpus or manifest — the glob moved?" >&2; exit 2; }
    fail=0
    for g in $CROSS_GATES; do cover "$g" "$dir" "$n" "$want_cross" || fail=1; done
    cover "$MANIFEST_GATE" "$dir" "$n" "$want_manifest" || fail=1
    [ "$fail" = 0 ] || exit 1
    echo "corpus shard coverage OK: $(wc -l < "$want_cross" | tr -d ' ') fixtures × $(echo $CROSS_GATES | wc -w | tr -d ' ') gates and $(wc -l < "$want_manifest" | tr -d ' ') manifest rows, each walked by exactly one of $n shards"
    ;;
  --merge)
    dir="$2"; n="$3"; archive="$4"
    # The two gates with a merge form, run from the same archive the shards
    # replayed, under `merge/N`: they read the partials and judge the whole.
    # `--extract-to .`/`--workspace-remap .` as the shards (ci-test-shard.sh).
    expr='(binary_id(=almide::wasm_runtime_interp_ledger) & test(=interp_bridge_fallback_ledger)) | binary_id(=almide-spine::run_parity)'
    echo "== merge/$n over $dir: $expr =="
    ALMIDE_CORPUS_SHARD="merge/$n" ALMIDE_CORPUS_SHARD_DIR="$dir" \
      exec cargo nextest run --archive-file "$archive" --extract-to . --workspace-remap . --no-fail-fast --no-capture -E "$expr"
    ;;
  *)
    echo "usage: $0 --coverage <dir> <N> | --merge <dir> <N> <archive> | --gates" >&2
    exit 2
    ;;
esac
