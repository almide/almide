#!/usr/bin/env bash
# The corpus weight table generator (#2457): measures the wall of every
# spec/wasm_cross fixture in each corpus gate and renders
# proofs/corpus-weights.txt, the table `ALMIDE_CORPUS_SHARD=k/N` balances on
# (crates/almide-corpus/src/lib.rs, `partition_by_weight`).
#
# Why a table: the modulo slice halved each giant's COST but not its WALL
# (run_parity 6 vs 16 min at N=2 on develop run 35645124173) because the
# per-fixture cost of the interpreter legs spans four orders of magnitude and
# the heavy tail fell on one residue class. A slice is now the LPT partition
# over these measured weights. A fixture with no row gets the column's median,
# so a stale table cannot exclude a new fixture — it lands in some shard, and
# the coverage step (scripts/ci-corpus-shards.sh --coverage) still proves the
# union is the corpus. The table only has to be REFRESHED when the heavy tail
# moves: a new giant fixture, or a gate whose cost profile changed.
#
#   ALMIDE_BIN=target/release/almide bash scripts/gen-corpus-weights.sh
#   bash scripts/gen-corpus-weights.sh --render <dir>   # from an earlier measurement
#
# The three columns and what measures them (each gate records under
# ALMIDE_CORPUS_WEIGHTS_DIR, a `<column>.<gate>[.<k>-of-<N>].txt` of
# `stem<TAB>ms`; `--render` folds every file of a column, sorted by name, first
# row per stem wins):
#   run_parity  crates/almide-spine/tests/run_parity.rs — the serial interpreter walk
#   interp      the interp sweep (tests/wasm_runtime_test_parts/interp_leg.rs),
#               measured by the ledger binary; the oracle balances on it too
#   build       the native + wasm (+ wasm-opt) subprocess builds per fixture
#               (tests/wasm_runtime_test_parts/corpus.rs), measured by cross_target
# Absolute ms differ per machine; only the RATIOS matter to the partition —
# and they differ per machine too (locally run_parity's modulo halves were
# 98 / 88 s; the CI runner's were 6 / 16 min), so the committed table is
# rendered from CI's own measurement: every `Test Rust (solo …)` job records
# under its shard-partials dir, the `corpus-shard-*` artifacts carry the
# files, and `--render` over the downloaded set is the refresh:
#   gh run download <run-id> -p 'corpus-shard-*' -D /tmp/cw && \
#     bash scripts/gen-corpus-weights.sh --render /tmp/cw   # (files may sit one dir down: cp /tmp/cw/*/* /tmp/cw/)
# Requires wasmtime for the local measurement (memory: /opt/homebrew/bin off the sandbox PATH).
set -uo pipefail
export LC_ALL=C
export PATH="/opt/homebrew/bin:$PATH"
cd "$(dirname "$0")/.." || exit 2

OUT="proofs/corpus-weights.txt"
MANIFEST="crates/almide-spine/tests/golden/spec-run-manifest.txt"
COLUMNS="run_parity interp build"

render() {
  local dir="$1" col stems tmp
  tmp="$(mktemp -d)"
  for col in $COLUMNS; do
    # Every file of the column, in name order (interp_ledger before the
    # oracle's contended interp measurement; shard files after the whole).
    find "$dir" -name "$col.*.txt" | sort | xargs cat > "$tmp/$col.txt" 2>/dev/null
    [ -s "$tmp/$col.txt" ] || { echo "::error::no $col.*.txt under $dir — the gate that records that column did not run" >&2; return 1; }
  done
  dir="$tmp"
  # Every stem any column measured, plus the corpus and the manifest (a stem
  # nothing measured gets an empty row: the reader takes the median).
  stems="$(
    { for f in spec/wasm_cross/*.almd; do basename "$f" .almd; done
      grep -v '^[[:space:]]*#' "$MANIFEST" | grep -v '^[[:space:]]*$' | cut -f3 | xargs -n1 basename | sed 's/\.almd$//'
      for col in $COLUMNS; do cut -f1 "$dir/$col.txt"; done
    } | sort -u
  )"
  {
    echo "# corpus-weights — measured wall (ms) of every spec/wasm_cross fixture per"
    echo "# corpus gate (#2457). ALMIDE_CORPUS_SHARD=k/N reads ONE column (which one,"
    echo "# per gate: crates/almide-corpus/src/lib.rs, weight_column) and slices the"
    echo "# sorted corpus by LPT over it, so the N shards' walls are balanced instead"
    echo "# of the heavy tail landing on one residue class. A stem with an empty cell"
    echo "# takes the column's MEDIAN — a stale table cannot exclude a fixture, and"
    echo "# scripts/ci-corpus-shards.sh --coverage proves the union regardless."
    echo "# Absolute values are one machine's; the partition uses ratios."
    echo "# Regenerate: ALMIDE_BIN=target/release/almide bash scripts/gen-corpus-weights.sh"
    echo "# generated: $(date -u +%Y-%m-%d) on $(uname -s)/$(uname -m), $(nproc 2>/dev/null || sysctl -n hw.ncpu) cpus"
    printf '# columns:\tstem'
    for col in $COLUMNS; do printf '\t%s' "$col"; done
    echo
    while IFS= read -r stem; do
      printf '%s' "$stem"
      for col in $COLUMNS; do
        printf '\t%s' "$(awk -F'\t' -v s="$stem" '$1 == s { print $2; exit }' "$dir/$col.txt")"
      done
      echo
    done <<<"$stems"
  } > "$OUT"
  echo "wrote $OUT: $(grep -vc '^#' "$OUT") row(s)"
  for col in $COLUMNS; do
    printf '  %-10s %5s measured, %8s ms summed, top: %s\n' "$col" "$(wc -l < "$dir/$col.txt" | tr -d ' ')" \
      "$(awk -F'\t' '{ s += $2 } END { print s }' "$dir/$col.txt")" \
      "$(sort -t$'\t' -k2,2nr "$dir/$col.txt" | head -3 | awk -F'\t' '{ printf "%s=%s ", $1, $2 }')"
  done
}

case "${1:-}" in
  --render)
    render "$2"
    ;;
  "")
    BIN="${ALMIDE_BIN:-target/release/almide}"
    case "$BIN" in /*) ;; *) BIN="$PWD/$BIN" ;; esac
    [ -x "$BIN" ] || { echo "::error::ALMIDE_BIN not executable: $BIN (cargo build --release)" >&2; exit 2; }
    command -v wasmtime >/dev/null || { echo "::error::wasmtime not on PATH — the build column needs it" >&2; exit 2; }
    dir="$(mktemp -d)"
    export ALMIDE_BIN="$BIN" ALMIDE_EXPECT_TOOLS=1 ALMIDE_CORPUS_WEIGHTS_DIR="$dir"
    unset ALMIDE_CORPUS_SHARD ALMIDE_CORPUS_FILTER
    # Serially: the interp sweep takes every core, and a contended
    # measurement is a wrong ratio. The gates' verdicts are not the point
    # here, but a red one still exits non-zero — a weight measured on a
    # panicking fixture is not a weight.
    echo "== run_parity (serial walk) =="
    cargo test --release -p almide-spine --test run_parity || exit 1
    echo "== interp sweep (ledger binary) =="
    cargo test --release --test wasm_runtime_interp_ledger || exit 1
    echo "== builds (cross_target binary) =="
    cargo test --release --test wasm_runtime_cross_target || exit 1
    render "$dir"
    ;;
  *)
    echo "usage: $0 [--render <dir>]" >&2
    exit 2
    ;;
esac
