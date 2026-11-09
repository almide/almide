#!/usr/bin/env bash
# #4005 A4 lab (probe only): the structural wall's per-artifact pipeline
# without the Rocq verdicts — build, bundle, both linear provers, their
# translation checks, almide-verify on both certificates, and the anchor
# check with the per-function census — over both legs, then the heap ledger.
set -uo pipefail
ROOT="$(pwd)"
OUT="${LAB_OUT:-$ROOT/lab-out}"
mkdir -p "$OUT/fx" "$OUT/st"
cargo build -q --bin almide && cargo build -q -p almide-wasm --example emit_structural_bundle && cargo build -q -p almide-verify -p almide-anchor-check || exit 1
cargo build -q --release -p almide-lin-prove || exit 1
TD="${CARGO_TARGET_DIR:-$ROOT/target}/debug"
export ALMIDE="$TD/almide" PRODUCER="$TD/examples/emit_structural_bundle" ANCHORCHECK="$TD/almide-anchor-check" VERIFY="$TD/almide-verify"
export LINPROVE="${CARGO_TARGET_DIR:-$ROOT/target}/release/almide-lin-prove"
one() {
  local mode="$1" n="$2" f="$3" d="$OUT/$1"
  if [ "$mode" = st ]; then
    "$PRODUCER" --tests "$f" --write-artifact "$d/$n.wasm" --anchors "$d/$n.anchors" > "$d/$n.bundle" 2> "$d/$n.err" || { echo "$f" > "$d/$n.skip"; return; }
  else
    "$ALMIDE" build "$f" --target wasm -o "$d/$n.wasm" > "$d/$n.err" 2>&1 || { echo "$f" > "$d/$n.skip"; return; }
    "$PRODUCER" "$f" --artifact "$d/$n.wasm" --anchors "$d/$n.anchors" > "$d/$n.bundle" 2> "$d/$n.err" || { echo "$f" > "$d/$n.skip"; return; }
  fi
  printf '%s\t%s\n' "$n" "$f" > "$d/$n.idx"
  if grep -q '^prims .' "$d/$n.anchors"; then
    timeout 900 "$LINPROVE" "$d/$n.wasm" "$d/$n.anchors" > "$d/$n.lcert" 2> "$d/$n.lprove" || echo "LINPROVE rc=$?" >> "$d/$n.lprove"
    "$ANCHORCHECK" --lin-verify "$d/$n.wasm" "$d/$n.anchors" "$d/$n.lcert" > "$d/$n.lin" 2>&1 || echo "LINVERIFY FAILED" >> "$d/$n.lin"
    "$VERIFY" bounds "$d/$n.lcert" > "$d/$n.lverify" 2>&1 || echo "VERIFY rc=$?" >> "$d/$n.lverify"
  fi
  if grep -q '^routines .' "$d/$n.anchors"; then
    local t0; t0=$(date +%s%N)
    timeout 900 "$LINPROVE" --routines "$d/$n.wasm" "$d/$n.anchors" "$d/$n.rsel" > "$d/$n.rcert" 2> "$d/$n.rprove" || echo "RPROVE rc=$?" >> "$d/$n.rprove"
    echo "rtime $(( ($(date +%s%N) - t0) / 1000000 ))" >> "$d/$n.rprove"
    "$ANCHORCHECK" --lin-verify-routines "$d/$n.wasm" "$d/$n.anchors" "$d/$n.rcert" "$d/$n.rsel" > "$d/$n.rlin" 2>&1 || echo "RLINVERIFY FAILED" >> "$d/$n.rlin"
    "$VERIFY" bounds "$d/$n.rcert" > "$d/$n.rverify" 2>&1 || echo "VERIFY rc=$?" >> "$d/$n.rverify"
  fi
  census=(--census --census-fns)
  if [ -s "$d/$n.lcert" ]; then census+=(--lin "$d/$n.lcert"); fi
  if [ -f "$d/$n.rsel" ]; then census+=(--rlin "$d/$n.rcert" "$d/$n.rsel"); fi
  timeout 900 "$ANCHORCHECK" "${census[@]}" "$d/$n.wasm" "$d/$n.bundle" "$d/$n.anchors" > "$d/$n.anchor" 2>&1 || mv "$d/$n.anchor" "$d/$n.anchor-rejected"
}
export -f one
export OUT
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt | awk '{print "fx", NR, $0}' > "$OUT/jobs"
LC_ALL=C ls spec/stdlib/*_test.almd | awk '{print "st", NR, $0}' >> "$OUT/jobs"
xargs -P "$(nproc)" -L 1 bash -c 'one "$0" "$1" "$2"' < "$OUT/jobs"
for leg in fx st; do
  cat "$OUT/$leg"/*.idx | sort -n > "$OUT/$leg/index"
  python3 proofs/lib/heap_ledger.py measure "$OUT/$leg" > "$OUT/heap-$leg.tsv" 2> "$OUT/heap-$leg.summary" || echo "MEASURE FAILED $leg"
  cat "$OUT/heap-$leg.summary"
done
echo "== failures"
grep -l 'FAILED\|rc=' "$OUT"/*/*.lin "$OUT"/*/*.rlin "$OUT"/*/*.lverify "$OUT"/*/*.rverify "$OUT"/*/*.lprove "$OUT"/*/*.rprove 2>/dev/null | head -50
python3 scripts/lab-a4-summary.py "$OUT" | tee "$OUT/summary.txt"
