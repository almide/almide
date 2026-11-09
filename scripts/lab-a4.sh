#!/usr/bin/env bash
# #4005 A4 lab (probe only): build every certified fixture and stdlib test
# module, run the linear prover and the per-function census, and keep the
# outputs for offline reading.
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
  echo "$f" > "$d/$n.name"
  if grep -q '^prims .' "$d/$n.anchors"; then
    timeout 900 "$LINPROVE" "$d/$n.wasm" "$d/$n.anchors" > "$d/$n.lcert" 2> "$d/$n.lprove" || echo "LINPROVE rc=$?" >> "$d/$n.lprove"
  fi
  lin=(); [ -s "$d/$n.lcert" ] && lin=(--lin "$d/$n.lcert")
  timeout 900 "$ANCHORCHECK" --census --census-fns "${lin[@]}" "$d/$n.wasm" "$d/$n.bundle" "$d/$n.anchors" > "$d/$n.census" 2>&1 || true
}
export -f one
export OUT
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt | awk '{print "fx", NR, $0}' > "$OUT/jobs"
LC_ALL=C ls spec/stdlib/*_test.almd | awk '{print "st", NR, $0}' >> "$OUT/jobs"
xargs -P "$(nproc)" -L 1 bash -c 'one "$0" "$1" "$2"' < "$OUT/jobs"
echo "lab: $(ls "$OUT"/fx/*.census 2>/dev/null | wc -l) fixture census, $(ls "$OUT"/st/*.census 2>/dev/null | wc -l) stdlib-test census"
python3 scripts/lab-a4-summary.py "$OUT" | tee "$OUT/summary.txt"
