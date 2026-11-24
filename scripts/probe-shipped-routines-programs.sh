#!/usr/bin/env bash
# Probe only, not a gate: the kernel check of the shipped runtime routines
# (proofs/check-shipped-routines.sh), run on the repository's own larger
# programs — examples, tools, benchmark sources — which no gate selects.
#
# The structural wall runs that check on the certified fixtures and the stdlib
# test modules. This asks the same question of the programs outside both
# corpora: does every runtime routine their anchor file names decode, as
# shipped, to the proved tree?
#
#   scripts/probe-shipped-routines-programs.sh <out-dir>
#
# Writes <out-dir>/programs.txt (the list), built.tsv (program, produced |
# why not), shipped-dump (the check's input), and shipped/verdicts.tsv.
# Needs coqc on PATH.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-shipped-routines-programs.sh <out-dir>}"
mkdir -p "$OUT/work"
OUT="$(cd "$OUT" && pwd)"
W="$OUT/work"

cargo build -q --release --locked --bin almide || exit 1
cargo build -q --release --locked -p almide-wasm --example emit_structural_bundle || exit 1
cargo build -q --release --locked -p almide-anchor-check || exit 1
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
ALMIDE="$TD/release/almide"
PRODUCER="$TD/release/examples/emit_structural_bundle"
ANCHORCHECK="$TD/release/almide-anchor-check"
# The driver must not find a verifier beside itself: it would then refuse to
# write a module whose certificate the verifier rejects, and this probe asks
# about the routines of every module that builds.
rm -f "$TD/release/almide-verify"
if command -v almide-verify > /dev/null 2>&1; then
  echo "a verifier is on PATH: $(command -v almide-verify); this probe expects none"; exit 1
fi

# The same list as the witness census of the larger programs.
git ls-files 'examples/*.almd' 'examples/**/*.almd' 'research/benchmark/exercises/**/*.almd' 'research/benchmark/perf/**/*.almd' \
  'research/benchmark/lang-bench/**/*.almd' 'tools/almide-gates/src/main.almd' 'tools/stdlib-crawler/main.almd' \
  | xargs grep -lE '^(effect )?fn main\(' | sort > "$OUT/programs.txt"
echo "programs: $(wc -l < "$OUT/programs.txt") listed"

: > "$OUT/built.tsv"
: > "$OUT/shipped-dump"
n=0
while IFS= read -r f; do
  n=$((n + 1))
  a="$W/m$n"
  if ! timeout 300 "$ALMIDE" build "$f" --target wasm -o "$a.wasm" > "$a.err" 2>&1 < /dev/null; then
    printf '%s\tno module\t%s\n' "$f" "$(grep -m1 . "$a.err" | cut -c1-160 | tr '\t' ' ')" >> "$OUT/built.tsv"; continue
  fi
  if ! timeout 300 "$PRODUCER" "$f" --artifact "$a.wasm" --anchors "$a.anchors" > "$a.bundle" 2> "$a.err" < /dev/null; then
    printf '%s\tno anchors\t%s\n' "$f" "$(grep -m1 . "$a.err" | cut -c1-160 | tr '\t' ' ')" >> "$OUT/built.tsv"; continue
  fi
  if "$ANCHORCHECK" --proved-bytes "$a.wasm" "$a.anchors" > "$a.shipped" 2> "$a.err"; then
    { printf 'MODULE %s\n' "$f"; cat "$a.shipped"; } >> "$OUT/shipped-dump"
    printf '%s\tproduced\t%s\n' "$f" "$(grep -c '^SHIPPED ' "$a.shipped")" >> "$OUT/built.tsv"
  else
    printf '%s\troutines not read\t%s\n' "$f" "$(head -1 "$a.shipped" | cut -c1-160 | tr '\t' ' ')" >> "$OUT/built.tsv"
  fi
done < "$OUT/programs.txt"
echo "== built"
cut -f2 "$OUT/built.tsv" | sort | uniq -c
awk -F'\t' '$2 != "produced" { print "   " $2 ": " $1 " — " $3 }' "$OUT/built.tsv"

# The proof files the shipped-form decoders need, in dependency order.
echo "== proof files"
( cd "$ROOT/proofs" && for v in OwnershipChecker.v RuntimeModel.v LargeList.v LargeTree.v StructuralRuntime.v StructuralAlloc.v StructuralDecode.v StructuralShipped.v; do
    coqc -Q . AlmideTrust "$v" > "$OUT/coqc-$v.log" 2>&1 || { echo "coqc failed on $v"; tail -5 "$OUT/coqc-$v.log"; exit 1; }
  done ) || exit 1
echo "compiled"

echo "== the kernel check"
t0=$(date +%s)
rc=0
bash "$ROOT/proofs/check-shipped-routines.sh" "$OUT/shipped-dump" "$OUT/shipped" || rc=$?
echo "check exit $rc in $(( $(date +%s) - t0 )) s"
echo "== verdicts"
cut -f3 "$OUT/shipped/verdicts.tsv" | sort | uniq -c
awk -F'\t' '$3 == "FAILED" { print "   FAILED: " $1 " (" $2 ")" }' "$OUT/shipped/verdicts.tsv"
exit 0
