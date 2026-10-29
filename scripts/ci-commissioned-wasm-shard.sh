#!/usr/bin/env bash
# The test-name sharding of the commissioned wasm gates' release-shape step
# (`Commissioned wasm gates (structural leg, release, <shard>)` in
# .github/workflows/ci.yml).
#
# The step was ONE `cargo test --release` over six packages. cargo runs the
# test binaries one after another, and the corpus gates are each one #[test]
# walking ~1000 fixtures on one thread, so the step took the SUM of the gates:
# 26 min on 2026-10-24 (build 4.6 + tests 21), 69 min on 2026-10-29
# (build 9.8 + tests 58.4) — over the job's 70-minute timeout on the
# develop→main run. Seven tests were 3,050 s of the 3,507 s.
#
# Each shard is a matrix job running the SAME cargo command (the same build,
# so the shards share one rust-cache entry) with a different libtest filter:
#
#   a named shard   `--exact <its tests>`         only the gates it owns
#   rest            `--exact --skip <every one>`  everything else
#
# The partition cannot drop a test: `rest` skips exactly the names the other
# shards own, read from the one table below, so a test is in a named shard or
# in `rest`. A renamed or deleted gate stops matching both its shard's filter
# and `rest`'s skip, so it runs in `rest`; `verify` then fails the named
# shard, whose log no longer shows it pass. `verify rest` fails when the
# workflow's matrix and this table name different shards.
#
#   bash scripts/ci-commissioned-wasm-shard.sh args   <shard>   libtest args
#   bash scripts/ci-commissioned-wasm-shard.sh verify <shard> <log>
#   bash scripts/ci-commissioned-wasm-shard.sh shards           shard names
#
# Packing (seconds on ubuntu-latest, develop push run 40814655073): ratchet
# 678 + 137, ledgers 437 + 414, parity 477 + 340, wasi 344 + 341, rest 340.
set -euo pipefail

shard_tests() {
  case "$1" in
    # size_ratchet.rs (its isolation pass already fans out over the cores);
    # native_result_ownership.rs.
    ratchet) echo "corpus_sizes_hold_the_baseline stdlib_results_are_owned_by_their_binder" ;;
    # alloc_ledger.rs; witness_floor.rs.
    ledgers) echo "corpus_allocation_watermarks_hold structural_witnesses_balance_and_hold_the_floor" ;;
    # almide-spine run_parity.rs; surface_matrix.rs.
    parity) echo "wasm_cross_fixtures_run_identically_on_the_interpreter exercised_surface_matches_golden" ;;
    # wasi_gate.rs (stock wasmtime); backend_parity.rs.
    wasi) echo "corpus_reproduces_on_stock_wasmtime corpus_burn_up" ;;
    *) return 1 ;;
  esac
}
NAMED="ratchet ledgers parity wasi"

case "${1:-}" in
  shards)
    echo "$NAMED rest"
    ;;
  args)
    shard="${2:?shard}"
    if [ "$shard" = rest ]; then
      printf -- '--exact'
      for s in $NAMED; do
        for t in $(shard_tests "$s"); do printf -- ' --skip %s' "$t"; done
      done
      echo
    else
      tests="$(shard_tests "$shard")" || { echo "unknown shard: $shard" >&2; exit 2; }
      echo "--exact $tests"
    fi
    ;;
  verify)
    shard="${2:?shard}"; log="${3:?log}"
    if [ "$shard" = rest ]; then
      # `rest` skips every named shard's tests, so a named shard the matrix
      # does not run would leave its gates in NO job. The matrix must list
      # exactly the shards this table has.
      want="[$(echo $NAMED rest | sed 's/ /, /g')]"
      got="$(grep -E '^ +shard: \[' "$(dirname "$0")/../.github/workflows/ci.yml" | grep -E 'ratchet|rest' | sed -E 's/^ +shard: //')"
      [ "$got" = "$want" ] || { echo "::error::the commissioned-wasm-gates matrix is \`$got\`, this table's shards are \`$want\`"; exit 1; }
      echo "rest: the matrix runs every shard ($want)"
      exit 0
    fi
    tests="$(shard_tests "$shard")" || { echo "unknown shard: $shard" >&2; exit 2; }
    missing=0
    for t in $tests; do
      if ! grep -Eq "^test $t \.\.\. ok\$" "$log"; then
        echo "::error::shard $shard: \`$t\` did not run and pass — renamed or removed? Update the table in scripts/ci-commissioned-wasm-shard.sh"
        missing=$((missing + 1))
      fi
    done
    [ "$missing" -eq 0 ] || exit 1
    echo "shard $shard: every named gate ran and passed ($tests)"
    ;;
  *)
    echo "usage: $0 args|verify|shards …" >&2
    exit 2
    ;;
esac
