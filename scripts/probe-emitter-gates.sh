#!/usr/bin/env bash
# Probe only: do the emitter crate's own corpus ledgers notice a source mutant
# of the emitter? For each mutant in <mutants-dir> the mutant is applied, the
# two release-only corpus tests of crates/almide-wasm are rebuilt and run, and
# one row is written:
#
#   alloc_ledger   corpus_allocation_watermarks_hold — per fixture, the final
#                  heap offset and the allocator's counts (both pinned exactly)
#                  and the blocks live at exit (shrink-only: a new leak fails)
#   witness_floor  structural_witnesses_balance_and_hold_the_floor — the
#                  ownership witnesses the portable checker accepts, the set of
#                  fully certified fixtures and the count of functions with
#                  ownership events (grow-only)
#
# Neither test reads a shipped module's bytes against its certificate; the two
# are the project's own answer to "an allocation changed" and "less is
# witnessed than before". The row says which of their sub-verdicts fired
# (scripts/probe-emitter-gates-rows.py names the columns).
#
#   scripts/probe-emitter-gates.sh <out-dir> <mutants-dir> [<order-file>]
# <mutants-dir> holds index.tsv (id, file, line, genre, name) and <id>.diff.
# <order-file> lists ids one per line; this shard's mutants run in that order,
# so the ones named first are judged even if the time allowance runs out.
set -uo pipefail

OUT="$1"
MUT="$2"
ORDER="${3:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p "$OUT/detail"
OUT="$(cd "$OUT" && pwd)"
MUT="$(cd "$MUT" && pwd)"
ROWS="$ROOT/scripts/probe-emitter-gates-rows.py"
# Seconds one test may take, and seconds after which no further mutant is
# started (the job's own limit is beyond it).
T_TEST="${T_TEST:-1800}"
T_SHARD="${T_SHARD:-16200}"
START=$(date +%s)

# build_tests: build the two test binaries and write bins.tsv (target, path).
build_tests() {
  cargo test --release --locked -p almide-wasm --test alloc_ledger --test witness_floor --no-run --message-format=json 2> "$OUT/build.err" \
    | python3 -I -c '
import json, sys
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and "test" in m.get("target", {}).get("kind", []):
        print(m["target"]["name"] + "\t" + m["executable"])
' > "$OUT/bins.tsv"
  local rc=$?
  [ "$rc" -eq 0 ] && [ "$(wc -l < "$OUT/bins.tsv")" -eq 2 ]
}

# run_one <target> <test>: the test binary itself, from the crate's directory
# as cargo would run it; --nocapture so the floor's own count line is kept.
run_one() {
  local bin
  bin="$(awk -F'\t' -v n="$1" '$1 == n { print $2 }' "$OUT/bins.tsv")"
  ( cd "$ROOT/crates/almide-wasm" && timeout "$T_TEST" "$bin" --exact "$2" --nocapture ) > "$OUT/$1.out" 2>&1 < /dev/null
  echo $? > "$OUT/$1.rc"
}

run_both() {
  rm -f "$OUT"/alloc_ledger.out "$OUT"/alloc_ledger.rc "$OUT"/witness_floor.out "$OUT"/witness_floor.rc
  run_one alloc_ledger corpus_allocation_watermarks_hold &
  run_one witness_floor structural_witnesses_balance_and_hold_the_floor &
  wait
}

# ---------------------------------------------------------------- baseline
t0=$(date +%s)
build_tests || { echo "baseline build failed"; tail -40 "$OUT/build.err"; exit 1; }
echo "baseline build: $(( $(date +%s) - t0 )) s"
cat "$OUT/bins.tsv"
t0=$(date +%s)
run_both
base="$(python3 -I "$ROWS" "$OUT" "$OUT/detail/_baseline")"
echo "baseline tests: $(( $(date +%s) - t0 )) s"
echo "baseline row: $base"
grep -h -E '^\[witness-floor\]|^test result' "$OUT/alloc_ledger.out" "$OUT/witness_floor.out" || true
printf '%s\n' "$base" > "$OUT/detail/_baseline.row.tsv"
ls_status="$(printf '%s' "$base" | cut -f1)"
fl_status="$(printf '%s' "$base" | cut -f12)"
if [ "$ls_status" != pass ] || [ "$fl_status" != pass ]; then
  echo "the unmutated tree does not pass both tests here (ledger: $ls_status, floor: $fl_status); a failure would not mean a mutant caused it"
  tail -30 "$OUT/alloc_ledger.out"; tail -30 "$OUT/witness_floor.out"
  exit 1
fi

# ---------------------------------------------------------------- mutants
RES="$OUT/results.tsv"
printf 'id\tfile\tline\tgenre\tstatus\tbuild_s\ttest_s\tledger_status\tledger_rc\tledger_drifts\tleak\tlive_less\twatermark\tcount\tarmed\trefusal\tunrefusal\tledger_other\tfloor_status\tfloor_rc\tfloor_kind\tadmitted\twitnessed\tstdlib\tdeclined\tcertified\tmanifest\tmodes_checked\tmodes_rejected\tfirst_ledger\tfirst_floor\tname\n' > "$RES"
NONE=$'-\t-\t0\t0\t0\t0\t0\t0\t0\t0\t0\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-\t-'

LIST="$OUT/todo.tsv"
if [ -n "$ORDER" ] && [ -f "$ORDER" ]; then
  python3 -I -c '
import sys
order = {l.strip(): i for i, l in enumerate(open(sys.argv[1], encoding="utf-8")) if l.strip()}
rows = [l for l in open(sys.argv[2], encoding="utf-8") if l.strip()]
rows.sort(key=lambda l: order.get(l.split("\t")[0], len(order)))
sys.stdout.write("".join(rows))
' "$ORDER" "$MUT/index.tsv" > "$LIST"
else
  cp "$MUT/index.tsv" "$LIST"
fi
echo "this shard: $(wc -l < "$LIST") mutant(s)"

while IFS=$'\t' read -r -u 3 id file line genre name; do
  [ -n "$id" ] || continue
  row() { printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$file" "$line" "$genre" "$1" "$2" "$3" "$4" "$name" >> "$RES"; }
  if [ $(( $(date +%s) - START )) -gt "$T_SHARD" ]; then
    row out-of-time 0 0 "$NONE"
    echo "$id out-of-time"
    continue
  fi
  if ! git apply "$MUT/$id.diff" 2> "$OUT/apply.err"; then
    row no-apply 0 0 "$NONE"
    echo "$id no-apply"
    continue
  fi
  t0=$(date +%s)
  if build_tests; then
    tb=$(( $(date +%s) - t0 ))
    t0=$(date +%s)
    run_both
    tt=$(( $(date +%s) - t0 ))
    cols="$(python3 -I "$ROWS" "$OUT" "$OUT/detail/$id")"
    row ok "$tb" "$tt" "$cols"
    { echo "== $id $file:$line $genre"; echo "$name"; cat "$MUT/$id.diff"; } > "$OUT/detail/$id.txt"
    echo "$id ok build ${tb}s tests ${tt}s: $(printf '%s' "$cols" | cut -f1,3,4,12,14)"
  else
    row no-build "$(( $(date +%s) - t0 ))" 0 "$NONE"
    echo "$id no-build"
  fi
  git apply -R "$MUT/$id.diff"
done 3< "$LIST"

echo "== summary"
cut -f5,8,19 "$RES" | tail -n +2 | sort | uniq -c
echo "elapsed: $(( $(date +%s) - START )) s"
