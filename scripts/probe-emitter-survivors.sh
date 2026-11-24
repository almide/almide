#!/usr/bin/env bash
# Probe only: does the project's own test suite, run on the wasm target, notice
# a source mutant of the emitter that nothing in the first two stages noticed?
#
# The first stage (probe-emitter-mutants.sh) asked the three layers about the
# 1,059 contract programs; the second (probe-emitter-gates.sh) asked the
# emitter crate's allocation ledger and witness floor. This stage asks the
# merge gate's own wasm test job — `almide test spec/ --target wasm` — one
# file at a time, so that a file's verdict can be compared with its verdict
# under the unmutated compiler.
#
# Per mutant: apply the diff, rebuild the compiler, run every test file, and
# write one row: how many files end differently from the baseline, how many
# newly fail, and the first of them. A file that ends differently in two runs
# of the unmutated compiler is left out of every comparison.
#
#   scripts/probe-emitter-survivors.sh <out-dir> <mutants-dir> [<order-file>]
# <mutants-dir> holds index.tsv (id, file, line, genre, name) and <id>.diff.
set -uo pipefail
export LC_ALL=C

OUT="$1"
MUT="$2"
ORDER="${3:-}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p "$OUT/detail" "$OUT/base" "$OUT/base2" "$OUT/work"
OUT="$(cd "$OUT" && pwd)"
MUT="$(cd "$MUT" && pwd)"
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
export ALMIDE="$TD/release/almide"
# Seconds one test file may take, and seconds after which no further mutant is
# started (the job's own limit is beyond it).
export T_FILE="${T_FILE:-180}"
T_SHARD="${T_SHARD:-16200}"
START=$(date +%s)

# Every file `almide test spec/` would find: one with a top-level test block.
TESTS="$OUT/tests.txt"
grep -r -l -E '^test "' --include='*.almd' spec | sort > "$TESTS"
echo "test files: $(wc -l < "$TESTS")"

name_of() { printf '%s' "$1" | tr '/' '_'; }
export -f name_of

# one <file> <dir>: the file's tests on the wasm target; its exit code, and
# the tail of what it printed.
one() {
  local f="$1" d="$2" n rc=0
  n="$(name_of "$f")"
  timeout "$T_FILE" "$ALMIDE" test "$f" --target wasm > "$d/$n.out" 2>&1 < /dev/null || rc=$?
  echo "$rc" > "$d/$n.rc"
  if [ "$rc" -eq 0 ]; then rm -f "$d/$n.out"; else tail -12 "$d/$n.out" > "$d/$n.tail"; rm -f "$d/$n.out"; fi
}
export -f one

# run_all <dir>: every test file into a directory of its own run.
run_all() { mkdir -p "$1"; xargs -P 4 -I{} bash -c 'one "$1" "$2"' _ {} "$1" < "$TESTS"; }

build() { cargo build -q --release --locked --bin almide 2> "$OUT/build.err"; }

# ---------------------------------------------------------------- baseline
t0=$(date +%s)
build || { echo "baseline build failed"; tail -40 "$OUT/build.err"; exit 1; }
echo "baseline build: $(( $(date +%s) - t0 )) s"
"$ALMIDE" --version
t0=$(date +%s)
run_all "$OUT/base"
echo "baseline tests, first run: $(( $(date +%s) - t0 )) s"
run_all "$OUT/base2"
# stable.txt: the files that end the same way in both baseline runs.
: > "$OUT/stable.txt"; : > "$OUT/unstable.txt"
while IFS= read -r f; do
  n="$(name_of "$f")"
  if [ "$(cat "$OUT/base/$n.rc")" = "$(cat "$OUT/base2/$n.rc")" ]; then echo "$f" >> "$OUT/stable.txt"; else echo "$f" >> "$OUT/unstable.txt"; fi
done < "$TESTS"
pass=0; fail=0
while IFS= read -r f; do
  if [ "$(cat "$OUT/base/$(name_of "$f").rc")" = 0 ]; then pass=$((pass + 1)); else fail=$((fail + 1)); fi
done < "$OUT/stable.txt"
echo "baseline: $pass file(s) pass, $fail fail, $(wc -l < "$OUT/unstable.txt") end differently in two runs (left out)"
{
  printf 'file\trc\n'
  while IFS= read -r f; do printf '%s\t%s\n' "$f" "$(cat "$OUT/base/$(name_of "$f").rc")"; done < "$TESTS"
} > "$OUT/baseline.tsv"
[ "$pass" -ge 300 ] || { echo "too few test files pass under the unmutated compiler; the comparison would be empty"; exit 1; }

# ---------------------------------------------------------------- mutants
RES="$OUT/results.tsv"
printf 'id\tfile\tline\tgenre\tstatus\tbuild_s\ttest_s\tchanged\tnewly_failing\tnewly_passing\ttimeouts\tfirst\tfirst_line\tname\n' > "$RES"

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
  NONE=$'0\t0\t0\t0\t-\t-'
  if [ $(( $(date +%s) - START )) -gt "$T_SHARD" ]; then
    row out-of-time 0 0 "$NONE"; echo "$id out-of-time"; continue
  fi
  if ! git apply "$MUT/$id.diff" 2> "$OUT/apply.err"; then
    row no-apply 0 0 "$NONE"; echo "$id no-apply"; continue
  fi
  t0=$(date +%s)
  if build; then
    tb=$(( $(date +%s) - t0 ))
    t0=$(date +%s)
    run_all "$OUT/work/$id"
    tt=$(( $(date +%s) - t0 ))
    changed=0; nf=0; np=0; to=0; first=-; firstline=-
    : > "$OUT/detail/$id.tsv"
    while IFS= read -r f; do
      n="$(name_of "$f")"
      b="$(cat "$OUT/base/$n.rc")"; m="$(cat "$OUT/work/$id/$n.rc" 2>/dev/null || echo missing)"
      [ "$b" != "$m" ] || continue
      changed=$((changed + 1))
      [ "$m" != 124 ] || to=$((to + 1))
      if [ "$b" = 0 ]; then
        nf=$((nf + 1))
        if [ "$first" = - ]; then
          first="$f"
          firstline="$(grep -m1 -i -E 'fail|error|trap|panic|unreachable|expected|assert' "$OUT/work/$id/$n.tail" 2>/dev/null | tr '\t' ' ' | cut -c1-200)"
          [ -n "$firstline" ] || firstline="$(tail -1 "$OUT/work/$id/$n.tail" 2>/dev/null | tr '\t' ' ' | cut -c1-200)"
          [ -n "$firstline" ] || firstline=-
        fi
      elif [ "$m" = 0 ]; then np=$((np + 1)); fi
      printf '%s\t%s\t%s\n' "$f" "$b" "$m" >> "$OUT/detail/$id.tsv"
      [ ! -f "$OUT/work/$id/$n.tail" ] || { echo "== $f (baseline $b, mutant $m)"; cat "$OUT/work/$id/$n.tail"; } >> "$OUT/detail/$id.tails.txt"
    done < "$OUT/stable.txt"
    row ok "$tb" "$tt" "$(printf '%s\t%s\t%s\t%s\t%s\t%s' "$changed" "$nf" "$np" "$to" "$first" "$firstline")"
    { echo "== $id $file:$line $genre"; echo "$name"; cat "$MUT/$id.diff"; } > "$OUT/detail/$id.txt"
    echo "$id ok build ${tb}s tests ${tt}s: changed $changed, newly failing $nf, newly passing $np, timeouts $to, first $first"
  else
    row no-build "$(( $(date +%s) - t0 ))" 0 "$NONE"
    echo "$id no-build"; tail -5 "$OUT/build.err"
  fi
  git apply -R "$MUT/$id.diff"
done 3< "$LIST"

echo "== summary"
awk -F'\t' 'NR > 1 { k = $5 "\t" ($9 > 0 ? "noticed" : "not noticed"); c[k]++ } END { for (k in c) print c[k] "\t" k }' "$RES" | sort
echo "elapsed: $(( $(date +%s) - START )) s"
