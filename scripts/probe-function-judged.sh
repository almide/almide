#!/usr/bin/env bash
# Probe only, not a gate: in how many functions of the contract programs'
# shipped modules does the kernel judge EVERY load and store
# (proofs/GuardFunction.v: the fresh-block rule and the window rule, asked
# about strings the kernel cuts out of the function body itself)?
#
#   scripts/probe-function-judged.sh <out-dir>
#
# Writes <out-dir>/summary.txt, modules.tsv and functions.tsv
# (scripts/probe-function-judged.py join). Needs coqc on PATH and the release
# binaries built.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-function-judged.sh <out-dir>}"
mkdir -p "$OUT/m" "$OUT/work"
OUT="$(cd "$OUT" && pwd)"
W="$OUT/m"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
ALMIDE="$TD/almide"
PRODUCER="$TD/examples/emit_structural_bundle"
ANCHORCHECK="$TD/almide-anchor-check"
PROOFS="$ROOT/proofs"
TOOL="$ROOT/scripts/probe-function-judged.py"
# The driver must not find a verifier beside itself: it would then refuse to
# write a module whose certificate the verifier rejects.
rm -f "$TD/almide-verify"

if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardFresh.v && coqc -Q . AlmideTrust GuardFunction.v) > "$OUT/coqc.log" 2>&1; then
  echo "the proofs do not compile:"; tail -n 80 "$OUT/coqc.log"; exit 1
fi
echo "== the proofs compile"

LIST="$OUT/list.txt"
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt > "$LIST"
[ -z "${ONLY_FIRST:-}" ] || { head -n "$ONLY_FIRST" "$LIST" > "$LIST.cut"; mv "$LIST.cut" "$LIST"; }
echo "contract programs: $(wc -l < "$LIST" | tr -d ' ')"

name_of() { printf '%s' "$1" | tr '/' '_'; }
# emit <almd>: the module, its anchor file, and the two cutters' proposals.
emit() {
  local f="$1" n
  n="$(name_of "$f")"
  if ! timeout 300 "$ALMIDE" build "$f" --target wasm -o "$W/$n.wasm" > "$W/$n.cerr" 2>&1 < /dev/null; then
    echo "$f" >> "$OUT/no-module.txt"; return
  fi
  if ! timeout 300 "$PRODUCER" "$f" --artifact "$W/$n.wasm" --anchors "$W/$n.anchors" > "$W/$n.bundle" 2> "$W/$n.perr" < /dev/null; then
    echo "$f" >> "$OUT/no-anchors.txt"; return
  fi
  timeout 300 "$ANCHORCHECK" --windows "$W/$n.wasm" > "$W/$n.windows" 2> "$W/$n.werr" || echo "$f" >> "$OUT/cutter-failed.txt"
  timeout 300 "$ANCHORCHECK" --fresh "$W/$n.wasm" "$W/$n.anchors" > "$W/$n.fresh" 2> "$W/$n.ferr" || echo "$f" >> "$OUT/cutter-failed.txt"
}
export -f name_of emit
export W OUT ALMIDE PRODUCER ANCHORCHECK

: > "$OUT/no-module.txt"; : > "$OUT/no-anchors.txt"; : > "$OUT/cutter-failed.txt"
t0=$(date +%s)
xargs -P 4 -I{} bash -c 'emit "$1"' _ {} < "$LIST"
echo "== emitted in $(( $(date +%s) - t0 )) s: $(ls "$W" | grep -c '\.wasm$') module(s); no module $(wc -l < "$OUT/no-module.txt" | tr -d ' '), no anchor file $(wc -l < "$OUT/no-anchors.txt" | tr -d ' '), a cutter failed $(sort -u "$OUT/cutter-failed.txt" | wc -l | tr -d ' ')"

: > "$OUT/modules-in.tsv"
while IFS= read -r f; do
  n="$(name_of "$f")"
  [ -s "$W/$n.wasm" ] && [ -f "$W/$n.fresh" ] && [ -f "$W/$n.windows" ] || continue
  printf '%s\t%s\t%s\t%s\n' "$f" "$W/$n.wasm" "$W/$n.fresh" "$W/$n.windows" >> "$OUT/modules-in.tsv"
done < "$LIST"
echo "== modules with both cutters' output: $(wc -l < "$OUT/modules-in.tsv" | tr -d ' ')"

# ask <stem>: compile every <stem>_<k>.v of the work directory, four at a time.
ask() {
  local stem="$1" t1
  t1=$(date +%s)
  find "$OUT/work" -maxdepth 1 -name "${stem}_*.v" | sort \
    | xargs -P 4 -I{} bash -c 'c="${1%.v}"; if coqc -Q "$PROOFS" AlmideTrust "$1" > "$c.out" 2> "$c.err"; then echo 0 > "$c.rc"; else echo 1 > "$c.rc"; fi' _ {}
  echo "== $stem: $(find "$OUT/work" -maxdepth 1 -name "${stem}_*.v" | wc -l | tr -d ' ') file(s) in $(( $(date +%s) - t1 )) s; failed to compile: $(grep -l 1 "$OUT/work/${stem}"_*.rc 2> /dev/null | wc -l | tr -d ' ')"
  for r in "$OUT/work/${stem}"_*.rc; do
    [ -e "$r" ] || continue
    [ "$(cat "$r")" = 0 ] || { echo "   ${r%.rc}.v:"; head -n 12 "${r%.rc}.err" | cut -c1-300 | sed 's/^/      /'; }
  done
}
export PROOFS

python3 -I "$TOOL" gen "$OUT/modules-in.tsv" "$OUT/work" "${CHUNK:-100}" || exit 1
ask Ask
python3 -I "$TOOL" controls "$OUT/work" "${CONTROLS:-60}" || exit 1
ask Control
python3 -I "$TOOL" checks "$OUT/work" "${CHUNK:-100}" || exit 1
ask Check
echo "== the tables"
python3 -I "$TOOL" join "$OUT/work" "$OUT" || exit 1
