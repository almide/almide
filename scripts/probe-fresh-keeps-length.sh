#!/usr/bin/env bash
# Probe only: the fresh-block rule with one more test (a store must leave the
# block's length word alone), asked about every contract program. Not a gate.
#
#   scripts/probe-fresh-keeps-length.sh <out-dir>
set -uo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="$1"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
PRODUCER="$TD/examples/emit_structural_bundle"
ANCHORCHECK="$TD/almide-anchor-check"
if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardFresh.v) > "$OUT/coqc.log" 2>&1; then
  echo "the proofs do not compile:"; tail -n 60 "$OUT/coqc.log"; exit 1
fi
echo "== the proofs compile; what the two theorems assume:"
grep -B1 -A4 -E "Closed under the global context|Axioms:" "$OUT/coqc.log" | head -n 30
: > "$OUT/fresh-dump"; : > "$OUT/fresh.err"
n=0; bad=0
while IFS= read -r f; do
  case "$f" in ''|'#'*) continue ;; esac
  n=$((n + 1))
  # The module is the one the ordinary build command writes; the producer is
  # given it and writes the anchor file beside its bundle.
  if ! "$TD/almide" build "$f" --target wasm -o "$OUT/m.wasm" > "$OUT/m.err" 2>&1; then
    bad=$((bad + 1)); [ "$bad" -gt 5 ] || { echo "the build refused $f: $(head -n 1 "$OUT/m.err" | cut -c1-200)"; }; continue
  fi
  if ! "$PRODUCER" "$f" --artifact "$OUT/m.wasm" --anchors "$OUT/m.anchors" > "$OUT/m.bundle" 2> "$OUT/m.err"; then
    bad=$((bad + 1)); [ "$bad" -gt 5 ] || { echo "the producer refused $f: $(head -n 1 "$OUT/m.err" | cut -c1-200)"; }; continue
  fi
  printf 'MODULE %s\n' "$f" >> "$OUT/fresh-dump"
  "$ANCHORCHECK" --fresh "$OUT/m.wasm" "$OUT/m.anchors" >> "$OUT/fresh-dump" 2>> "$OUT/fresh.err" || { bad=$((bad + 1)); echo "the cutter failed on $f"; }
done < crates/almide-wasm/tests/golden/witness-fixtures.txt
echo "== programs: $n ; failed: $bad ; proposals: $(grep -c '^FRESH' "$OUT/fresh-dump")"
printf 'fixtures 0\n' > "$OUT/floor.txt"
rc=0
SHIPPED_FRESH_FLOOR="$OUT/floor.txt" bash proofs/check-shipped-fresh.sh "$OUT/fresh-dump" "$OUT/fresh" fixtures > "$OUT/check.out" 2>&1 || rc=$?
echo "== check-shipped-fresh.sh: exit $rc"
tail -n 12 "$OUT/check.out"
if [ -f "$OUT/fresh/verdicts.tsv" ]; then
  echo "== accepted accesses: $(grep -c 'ok$' "$OUT/fresh/verdicts.tsv") ; refused: $(grep -c 'REFUSED$' "$OUT/fresh/verdicts.tsv")"
  grep 'REFUSED$' "$OUT/fresh/verdicts.tsv" | head -n 60
fi
