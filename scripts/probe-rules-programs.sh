#!/usr/bin/env bash
# Probe only, not a gate: the three kernel-checked rules of the shipped-access
# gates (proofs/check-shipped-windows.sh, check-shipped-fresh.sh,
# check-shipped-loops.sh), run on the repository's own larger programs —
# examples, tools, benchmark sources — which no gate selects and on which the
# rules were not developed.
#
#   scripts/probe-rules-programs.sh <out-dir>
#
# Writes <out-dir>/programs.txt (the list), built.tsv (program, module or why
# not, the byte-level check's exit code, the census line), the three dumps and
# the three verdict tables. Needs coqc on PATH and the release binaries built.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:?usage: scripts/probe-rules-programs.sh <out-dir>}"
mkdir -p "$OUT/work"
OUT="$(cd "$OUT" && pwd)"
W="$OUT/work"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
ALMIDE="$TD/almide"
PRODUCER="$TD/examples/emit_structural_bundle"
ANCHORCHECK="$TD/almide-anchor-check"
# The driver must not find a verifier beside itself: it would then refuse to
# write a module whose certificate the verifier rejects, and this probe asks
# about every module that builds.
rm -f "$TD/almide-verify"
if command -v almide-verify > /dev/null 2>&1; then
  echo "a verifier is on PATH: $(command -v almide-verify); this probe expects none"; exit 1
fi
if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardFresh.v && coqc -Q . AlmideTrust GuardLoop.v) > "$OUT/coqc.log" 2>&1; then
  echo "the proofs do not compile:"; tail -n 60 "$OUT/coqc.log"; exit 1
fi
echo "== the proofs compile"

# The same list as the witness census of the larger programs.
git ls-files 'examples/*.almd' 'examples/**/*.almd' 'research/benchmark/exercises/**/*.almd' 'research/benchmark/perf/**/*.almd' \
  'research/benchmark/lang-bench/**/*.almd' 'tools/almide-gates/src/main.almd' 'tools/stdlib-crawler/main.almd' \
  | xargs grep -lE '^(effect )?fn main\(' | sort > "$OUT/programs.txt"
echo "programs: $(wc -l < "$OUT/programs.txt") listed"

: > "$OUT/built.tsv"
for d in window fresh loops; do : > "$OUT/$d-dump"; : > "$OUT/$d-dump.accepted"; done
n=0
while IFS= read -r f; do
  n=$((n + 1))
  a="$W/m$n"
  if ! timeout 300 "$ALMIDE" build "$f" --target wasm -o "$a.wasm" > "$a.err" 2>&1 < /dev/null; then
    printf '%s\tno module\t-\t%s\n' "$f" "$(grep -m1 . "$a.err" | cut -c1-160 | tr '\t' ' ')" >> "$OUT/built.tsv"; continue
  fi
  if ! timeout 300 "$PRODUCER" "$f" --artifact "$a.wasm" --anchors "$a.anchors" > "$a.bundle" 2> "$a.err" < /dev/null; then
    printf '%s\tno anchors\t-\t%s\n' "$f" "$(grep -m1 . "$a.err" | cut -c1-160 | tr '\t' ' ')" >> "$OUT/built.tsv"; continue
  fi
  rc=0
  timeout 300 "$ANCHORCHECK" "$a.wasm" "$a.bundle" "$a.anchors" > "$a.check" 2>&1 || rc=$?
  census="$("$ANCHORCHECK" --census "$a.wasm" "$a.bundle" "$a.anchors" 2>&1 | grep -m1 '^CENSUS' | tr '\t' ' ')"
  printf '%s\tmodule\t%s\t%s\n' "$f" "$rc" "$census" >> "$OUT/built.tsv"
  ok=1
  "$ANCHORCHECK" --windows "$a.wasm" > "$a.windows" 2> "$a.windows-err" || ok=0
  "$ANCHORCHECK" --fresh "$a.wasm" "$a.anchors" > "$a.fresh" 2> "$a.fresh-err" || ok=0
  "$ANCHORCHECK" --loops "$a.wasm" "$a.anchors" > "$a.loops" 2> "$a.loops-err" || ok=0
  [ "$ok" = 1 ] || echo "a cutter failed on $f: $(cat "$a.windows-err" "$a.fresh-err" "$a.loops-err" | head -n 2 | cut -c1-200)"
  for d in window fresh loops; do
    src="$a.${d}s"; [ "$d" = fresh ] && src="$a.fresh"; [ "$d" = loops ] && src="$a.loops"
    { printf 'MODULE %s\n' "$f"; hew "$src"; } >> "$OUT/$d-dump"
    [ "$rc" = 0 ] && { printf 'MODULE %s\n' "$f"; hew "$src"; } >> "$OUT/$d-dump.accepted"
  done
done < "$OUT/programs.txt"
echo "== built: $(awk -F'\t' '$2 == "module"' "$OUT/built.tsv" | wc -l | tr -d ' ') of $n ; the byte-level check accepts $(awk -F'\t' '$2 == "module" && $3 == 0' "$OUT/built.tsv" | wc -l | tr -d ' ')"
awk -F'\t' '$2 != "module"' "$OUT/built.tsv" | cut -c1-220

# The census: accesses of every module that built, and of those the check accepts.
python3 -I - "$OUT/built.tsv" <<'PY'
import re, sys
for label, want in (("every module that built", None), ("modules the byte-level check accepts", "0")):
    tot = [0] * 10; rows = 0
    for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
        c = line.rstrip("\n").split("\t")
        if len(c) < 4 or c[1] != "module" or (want is not None and c[2] != want): continue
        n = [int(x) for x in re.findall(r"\d+", c[3])]
        if len(n) == 10:
            rows += 1; tot = [a + b for a, b in zip(tot, n)]
    print(f"== census, {label}: {rows} module(s); accesses {tot[0] + tot[5]} (functions with a witness {tot[0]}, other {tot[5]}); located {tot[1] + tot[6]}; header words {tot[3] + tot[8]}; bulk {tot[4] + tot[9]}")
PY

printf 'programs 0\n' > "$OUT/floor1.txt"
printf 'programs 0 9999999 9999999\n' > "$OUT/floor3.txt"
for set in all accepted; do
  suf=""; [ "$set" = accepted ] && suf=".accepted"
  echo "==== $set modules"
  rc=0; SHIPPED_WINDOWS_FLOOR="$OUT/floor1.txt" bash proofs/check-shipped-windows.sh "$OUT/window-dump$suf" "$OUT/windows-$set" programs > "$OUT/windows-$set.out" 2>&1 || rc=$?
  echo "== windows: exit $rc"; tail -n 6 "$OUT/windows-$set.out" | cut -c1-300
  rc=0; SHIPPED_FRESH_FLOOR="$OUT/floor1.txt" bash proofs/check-shipped-fresh.sh "$OUT/fresh-dump$suf" "$OUT/fresh-$set" programs > "$OUT/fresh-$set.out" 2>&1 || rc=$?
  echo "== fresh blocks: exit $rc"; tail -n 6 "$OUT/fresh-$set.out" | cut -c1-300
  rc=0; SHIPPED_LOOPS_FLOOR="$OUT/floor3.txt" bash proofs/check-shipped-loops.sh "$OUT/loops-dump$suf" "$OUT/loops-$set" programs > "$OUT/loops-$set.out" 2>&1 || rc=$?
  echo "== loops: exit $rc"; tail -n 8 "$OUT/loops-$set.out" | cut -c1-300
  for d in windows fresh loops; do
    v="$OUT/$d-$set/verdicts.tsv"
    [ -f "$v" ] && echo "   $d verdicts: $(wc -l < "$v" | tr -d ' ') row(s); last column: $(awk -F'\t' '{print $NF}' "$v" | sort | uniq -c | tr '\n' ' ' | cut -c1-200)"
  done
done
