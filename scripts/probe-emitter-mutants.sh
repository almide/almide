#!/usr/bin/env bash
# Probe only: which layer notices a source mutant of the wasm emitter?
#
# The mutants come from an unmodified mutation tool's listing of the emitter
# crate (probe-emitter-mutants-pick.py draws this shard's share of a seeded
# sample and writes one diff per mutant). For each mutant: apply the diff,
# rebuild the compiler and the witness producer, and re-emit every contract
# program. Where an emitted artifact differs from the unmutated compiler's:
#   - the compiler itself may refuse to build the program (its own checks),
#   - the proven witness checkers judge the mutant's witness bundle,
#   - the byte-level check judges module, bundle and anchor file together,
#   - the program is run, and stdout and exit code are compared.
# The two checkers are built once, before any mutant, and never rebuilt: they
# are separate crates, and a mutant must not reach its own judges.
#
#   scripts/probe-emitter-mutants.sh <out-dir> <mutants-dir>
# <mutants-dir> holds index.tsv (id, file, line, genre, name) and <id>.diff.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
OUT="${1:?usage: scripts/probe-emitter-mutants.sh <out-dir> <mutants-dir>}"
MUT="${2:?}"
mkdir -p "$OUT/base" "$OUT/work" "$OUT/again" "$OUT/bin" "$OUT/detail"
OUT="$(cd "$OUT" && pwd)"
MUT="$(cd "$MUT" && pwd)"
LIST="$OUT/fixtures.txt"
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt > "$LIST"
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
export ALMIDE="$TD/release/almide"
export PRODUCER="$TD/release/examples/emit_structural_bundle"
export VERIFY="$OUT/bin/almide-verify"
export ANCHORCHECK="$OUT/bin/almide-anchor-check"
# Seconds one compile, one witness emission or one program run may take.
export T_EMIT="${T_EMIT:-60}"
export T_RUN="${T_RUN:-30}"
# Seconds one mutant may take in all, after its build.
T_MUTANT="${T_MUTANT:-1500}"

build() {
  cargo build -q --release --bin almide \
    && cargo build -q --release -p almide-wasm --example emit_structural_bundle
}

name_of() { printf '%s' "$1" | tr '/' '_'; }
export -f name_of

# emit <almd> <dir>: module, witness bundle and anchor file, with what the
# compiler and the producer returned.
emit() {
  local f="$1" d="$2" n rc=0
  n="$(name_of "$f")"
  rm -f "$d/$n".*
  ( ulimit -v 6000000; timeout "$T_EMIT" "$ALMIDE" build "$f" --target wasm -o "$d/$n.wasm" > "$d/$n.cerr" 2>&1 < /dev/null ) || rc=$?
  if [ "$rc" -eq 0 ]; then
    ( ulimit -v 6000000; timeout "$T_EMIT" "$PRODUCER" "$f" --artifact "$d/$n.wasm" --anchors "$d/$n.anchors" > "$d/$n.bundle" 2> "$d/$n.perr" < /dev/null ) || rc=$((1000 + $?))
  fi
  echo "$rc" > "$d/$n.rc"
}
export -f emit

# behave <almd> <dir>: stdout and exit code of the program on the wasm target.
# No address-space limit here: the embedded runtime reserves far more than it
# touches.
behave() {
  local f="$1" d="$2" n
  n="$(name_of "$f")"
  timeout "$T_RUN" "$ALMIDE" run "$f" --target wasm > "$d/$n.out" 2> "$d/$n.err" < /dev/null
  echo "EXIT $?" >> "$d/$n.out"
}
export -f behave

emit_all() { xargs -P 4 -I{} bash -c 'emit "$1" "$2"' _ {} "$1" < "$2"; }
behave_all() { xargs -P 4 -I{} bash -c 'behave "$1" "$2"' _ {} "$1" < "$2"; }

# ---------------------------------------------------------------- baseline
build || { echo "baseline build failed"; exit 1; }
cargo build -q --release -p almide-verify -p almide-anchor-check || { echo "checker build failed"; exit 1; }
cp "$TD/release/almide-verify" "$TD/release/almide-anchor-check" "$OUT/bin/"
# The driver judges a build's certificate itself when it finds a verifier
# beside its own executable or on PATH, and then refuses to write or run a
# module the verifier rejects. Each layer is to be asked separately here, so
# the driver must find none.
no_verifier() {
  rm -f "$TD/release/almide-verify"
  if command -v almide-verify > /dev/null 2>&1; then echo "a verifier is on PATH: $(command -v almide-verify)"; return 1; fi
}
no_verifier || exit 1
echo "verifier beside the compiler or on PATH: none"
t0=$(date +%s)
# A bundle names its artifact by path, so the unmutated compiler and every
# mutant emit into the same directory (work/); base/ is a copy of what the
# unmutated compiler left there. Emitting anywhere else makes every bundle
# differ from the baseline by that path alone.
emit_all "$OUT/work" "$LIST"
behave_all "$OUT/work" "$LIST"
zero=$(cat "$OUT"/work/*.out | grep -c '^EXIT 0$')
echo "baseline behaviour: $zero program(s) exit 0, $(cat "$OUT"/work/*.out | grep '^EXIT ' | grep -vc '^EXIT 0$') exit non-zero"
[ "$zero" -ge 800 ] || { echo "too few programs run to completion under the unmutated compiler; the comparison would be empty"; exit 1; }
bad=0
while IFS= read -r f; do
  n="$(name_of "$f")"
  [ "$(cat "$OUT/work/$n.rc")" = 0 ] || { bad=$((bad + 1)); echo "baseline does not emit: $f"; continue; }
  "$VERIFY" bundle "$OUT/work/$n.bundle" > /dev/null 2>&1 || { bad=$((bad + 1)); echo "baseline: proven checkers reject $f"; }
  "$ANCHORCHECK" "$OUT/work/$n.wasm" "$OUT/work/$n.bundle" "$OUT/work/$n.anchors" > /dev/null 2>&1 || { bad=$((bad + 1)); echo "baseline: byte-level check rejects $f"; }
done < "$LIST"
cp -a "$OUT/work/." "$OUT/base/"
# The comparison below reads "differs from the baseline" as "the mutant
# changed it", which holds only if emitting twice gives the same bytes: emit
# a slice again with the unmutated compiler and require it to be identical.
head -60 "$LIST" > "$OUT/slice.txt"
emit_all "$OUT/work" "$OUT/slice.txt"
same=0
while IFS= read -r f; do
  n="$(name_of "$f")"
  if cmp -s "$OUT/base/$n.wasm" "$OUT/work/$n.wasm" && cmp -s "$OUT/base/$n.bundle" "$OUT/work/$n.bundle" && cmp -s "$OUT/base/$n.anchors" "$OUT/work/$n.anchors"; then same=$((same + 1)); else echo "not reproducible: $f"; fi
done < "$OUT/slice.txt"
echo "emission reproducible on $same of $(wc -l < "$OUT/slice.txt") program(s)"
[ "$same" -eq "$(wc -l < "$OUT/slice.txt")" ] || { echo "the unmutated compiler does not emit the same bytes twice; a difference would not mean a mutant caused it"; exit 1; }
echo "baseline: $(wc -l < "$LIST") programs in $(( $(date +%s) - t0 )) s, $bad problem(s)"
[ "$bad" -eq 0 ] || { echo "the unmutated compiler does not pass its own corpus here; stopping"; exit 1; }

# ---------------------------------------------------------------- mutants
RES="$OUT/results.tsv"
printf 'id\tfile\tline\tgenre\tstatus\tprograms\tcompile_fail\twasm_diff\trecord_only_diff\tproven_reject\tbyte_reject\tbehaviour_diff\tbehaviour_run\tseconds\tfirst_compile_fail\tfirst_proven\tfirst_byte\tfirst_behaviour\tname\n' > "$RES"

one_mutant() {
  local id="$1" file="$2" line="$3" genre="$4" name="$5"
  local d="$OUT/work" t0 status=ok
  t0=$(date +%s)
  row() { printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$file" "$line" "$genre" "$@" "$name" >> "$RES"; }
  if ! git apply "$MUT/$id.diff" 2> "$OUT/apply.err"; then
    row no-apply 0 0 0 0 0 0 0 0 0 - - - -
    return
  fi
  if ! build 2> "$OUT/build.err"; then
    cp "$OUT/build.err" "$OUT/detail/$id.build.err"
    row no-build 0 0 0 0 0 0 0 0 "$(( $(date +%s) - t0 ))" - - - -
    git apply -R "$MUT/$id.diff"
    return
  fi
  no_verifier || { row verifier-present 0 0 0 0 0 0 0 0 0 - - - -; git apply -R "$MUT/$id.diff"; return; }
  rm -rf "$d"; mkdir -p "$d"
  local deadline=$(( $(date +%s) + T_MUTANT ))
  # A mutant that hangs the compiler would take an hour to say so: emit a
  # first slice, and stop if most of it timed out.
  head -40 "$LIST" > "$OUT/slice.txt"
  tail -n +41 "$LIST" > "$OUT/rest.txt"
  emit_all "$d" "$OUT/slice.txt"
  local hung
  hung=$(cat "$d"/*.rc | grep -c -E '^(124|1124)$' || true)
  if [ "$hung" -ge 20 ]; then
    row hang 40 "$hung" 0 0 0 0 0 0 "$(( $(date +%s) - t0 ))" - - - -
    git apply -R "$MUT/$id.diff"
    return
  fi
  emit_all "$d" "$OUT/rest.txt"
  local programs=0 cfail=0 wdiff=0 rdiff=0 prej=0 brej=0 bdiff=0 brun=0
  local fc="-" fp="-" fb="-" ft="-"
  : > "$OUT/changed.txt"
  while IFS= read -r f; do
    local n; n="$(name_of "$f")"
    programs=$((programs + 1))
    local rc; rc="$(cat "$d/$n.rc" 2>/dev/null || echo 999)"
    if [ "$rc" != 0 ]; then
      cfail=$((cfail + 1))
      [ "$fc" != "-" ] || fc="$f: rc $rc: $(grep -m1 . "$d/$n.cerr" "$d/$n.perr" 2>/dev/null | head -1 | cut -c1-200 | tr '\t' ' ')"
      continue
    fi
    local w=0 r=0
    cmp -s "$OUT/base/$n.wasm" "$d/$n.wasm" || w=1
    cmp -s "$OUT/base/$n.bundle" "$d/$n.bundle" || r=1
    cmp -s "$OUT/base/$n.anchors" "$d/$n.anchors" || r=1
    [ "$w" -eq 1 ] || [ "$r" -eq 1 ] || continue
    if [ "$w" -eq 1 ]; then wdiff=$((wdiff + 1)); echo "$f" >> "$OUT/changed.txt"; else rdiff=$((rdiff + 1)); fi
    if ! "$VERIFY" bundle "$d/$n.bundle" > "$d/$n.verify" 2>&1; then
      prej=$((prej + 1))
      [ "$fp" != "-" ] || fp="$f: $(grep -m1 -E '^REJECT|REJECTED|rejected' "$d/$n.verify" | cut -c1-200 | tr '\t' ' ')"
    fi
    if ! "$ANCHORCHECK" "$d/$n.wasm" "$d/$n.bundle" "$d/$n.anchors" > "$d/$n.anchor" 2>&1; then
      brej=$((brej + 1))
      [ "$fb" != "-" ] || fb="$f: $(grep -m1 -E 'REJECT|error' "$d/$n.anchor" | cut -c1-200 | tr '\t' ' ')"
    fi
  done < "$LIST"
  # Behaviour, for the programs whose module changed. Stop counting once
  # three differ: a mutant that hangs every program would otherwise cost
  # T_RUN for each of them.
  if [ -s "$OUT/changed.txt" ]; then
    while [ -s "$OUT/changed.txt" ] && [ "$bdiff" -lt 3 ] && [ "$(date +%s)" -lt "$deadline" ]; do
      head -24 "$OUT/changed.txt" > "$OUT/batch.txt"
      tail -n +25 "$OUT/changed.txt" > "$OUT/changed.next"; mv "$OUT/changed.next" "$OUT/changed.txt"
      behave_all "$d" "$OUT/batch.txt"
      while IFS= read -r f; do
        local n; n="$(name_of "$f")"
        brun=$((brun + 1))
        if ! cmp -s "$OUT/base/$n.out" "$d/$n.out"; then
          bdiff=$((bdiff + 1))
          [ "$ft" != "-" ] || ft="$f: $(tail -1 "$d/$n.out") | $(head -1 "$d/$n.err" | cut -c1-160 | tr '\t' ' ')"
        fi
      done < "$OUT/batch.txt"
    done
    [ ! -s "$OUT/changed.txt" ] || [ "$bdiff" -ge 3 ] || status=behaviour-unfinished
  fi
  row "$status" "$programs" "$cfail" "$wdiff" "$rdiff" "$prej" "$brej" "$bdiff" "$brun" "$(( $(date +%s) - t0 ))" "$fc" "$fp" "$fb" "$ft"
  if [ "$wdiff" -gt 0 ] || [ "$rdiff" -gt 0 ] || [ "$cfail" -gt 0 ]; then
    { echo "== $id $file:$line $genre"; echo "$name"; cat "$MUT/$id.diff"; echo "-- compile: $fc"; echo "-- proven: $fp"; echo "-- byte: $fb"; echo "-- behaviour: $ft"; } > "$OUT/detail/$id.txt"
  fi
  git apply -R "$MUT/$id.diff"
}

while IFS=$'\t' read -r id file line genre name; do
  [ -n "$id" ] || continue
  one_mutant "$id" "$file" "$line" "$genre" "$name"
  if ! git diff --quiet -- crates; then
    echo "tree not clean after mutant $id; restoring"
    git checkout -q -- crates
  fi
  tail -1 "$RES" | cut -f1-14
done < "$MUT/index.tsv"
echo "== done: $(($(wc -l < "$RES") - 1)) mutant(s)"
cut -f5 "$RES" | tail -n +2 | sort | uniq -c
