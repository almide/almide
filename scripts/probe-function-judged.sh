#!/usr/bin/env bash
# Probe only, not a gate: in how many functions of shipped modules does the
# kernel judge EVERY load and store (proofs/GuardFunction.v: the fresh-block
# rule and the window rule, asked about strings the kernel cuts out of the
# function body itself)?
#
#   scripts/probe-function-judged.sh corpus <out-dir>
#       the contract programs, built here.
#   scripts/probe-function-judged.sh pairs <out-dir> <held-checkout> <sets.tsv> [<replays-dir>]
#       modules that earlier runs kept, each against the module without the
#       mutant or the defect.
#       <held-checkout>: the tree the mutants were applied to, with its
#         compiler and witness producer built (target/release); it emits the
#         unmutated modules.
#       <sets.tsv>: set name, a directory holding (at any depth)
#         `emut/modules/<mutant id>/<program name>.wasm`, and optionally the
#         mutant ids to take (comma separated), one set per line.
#       <replays-dir>: `<replay>/<n>.base/` and `<replay>/<n>.bug/`, each with
#         `a.anchors.wasm` and `a.anchors` (a replayed defect: the module
#         without and with it, both with their own anchor file).
#
# Writes <out-dir>/summary.txt, modules.tsv and functions.tsv
# (scripts/probe-function-judged.py join) and, for pairs, mutants.tsv and
# mutants-programs.tsv (compare). Needs coqc on PATH and the release binaries
# built.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
MODE="${1:?usage: scripts/probe-function-judged.sh corpus|pairs <out-dir> ...}"
OUT="${2:?usage: scripts/probe-function-judged.sh corpus|pairs <out-dir> ...}"
mkdir -p "$OUT/m" "$OUT/work"
OUT="$(cd "$OUT" && pwd)"
W="$OUT/m"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
ANCHORCHECK="$TD/almide-anchor-check"
PROOFS="$ROOT/proofs"
TOOL="$ROOT/scripts/probe-function-judged.py"
[ -x "$ANCHORCHECK" ] || { echo "the cutter is not built: $ANCHORCHECK"; exit 1; }

if ! (cd proofs && coqc -Q . AlmideTrust GuardWindow.v && coqc -Q . AlmideTrust GuardFresh.v && coqc -Q . AlmideTrust GuardFunction.v) > "$OUT/coqc.log" 2>&1; then
  echo "the proofs do not compile:"; tail -n 80 "$OUT/coqc.log"; exit 1
fi
echo "== the proofs compile"

name_of() { printf '%s' "$1" | tr '/' '_'; }
# emit <tree> <almd> <stem>: the module and its anchor file, from the compiler
# built in <tree>.
emit() {
  local tree="$1" f="$2" stem="$3"
  ( cd "$tree" \
    && timeout 300 ./target/release/almide build "$f" --target wasm -o "$stem.wasm" > "$stem.cerr" 2>&1 < /dev/null \
    && timeout 300 ./target/release/examples/emit_structural_bundle "$f" --artifact "$stem.wasm" --anchors "$stem.anchors" > "$stem.bundle" 2> "$stem.perr" < /dev/null ) \
    || echo "$f" >> "$OUT/not-emitted.txt"
}
# cut <module> <anchors or -> <stem>: the two cutters' proposals.
cut_props() {
  local m="$1" a="$2" stem="$3"
  timeout 300 "$ANCHORCHECK" --windows "$m" > "$stem.windows" 2> "$stem.werr" || echo "$m" >> "$OUT/cutter-failed.txt"
  if [ "$a" != "-" ] && [ -s "$a" ]; then
    timeout 300 "$ANCHORCHECK" --fresh "$m" "$a" > "$stem.fresh" 2> "$stem.ferr" || echo "$m" >> "$OUT/cutter-failed.txt"
  else
    : > "$stem.fresh"
  fi
}
export -f name_of emit cut_props
export W OUT ANCHORCHECK TOOL
: > "$OUT/not-emitted.txt"; : > "$OUT/cutter-failed.txt"; : > "$OUT/modules-in.tsv"

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

judge_all() {
  echo "== modules to read: $(wc -l < "$OUT/modules-in.tsv" | tr -d ' ')"
  python3 -I "$TOOL" gen "$OUT/modules-in.tsv" "$OUT/work" "${CHUNK:-100}" || exit 1
  ask Ask
  python3 -I "$TOOL" controls "$OUT/work" "${CONTROLS:-60}" || exit 1
  ask Control
  python3 -I "$TOOL" checks "$OUT/work" "${CHUNK:-100}" || exit 1
  ask Check
  echo "== the tables"
  python3 -I "$TOOL" join "$OUT/work" "$OUT" || exit 1
}

if [ "$MODE" = corpus ]; then
  # The driver must not find a verifier beside itself: it would then refuse
  # to write a module whose certificate the verifier rejects.
  rm -f "$TD/almide-verify"
  LIST="$OUT/list.txt"
  grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt > "$LIST"
  [ -z "${ONLY_FIRST:-}" ] || { head -n "$ONLY_FIRST" "$LIST" > "$LIST.cut"; mv "$LIST.cut" "$LIST"; }
  echo "contract programs: $(wc -l < "$LIST" | tr -d ' ')"
  one() {
    local f="$1" n
    n="$(name_of "$f")"
    emit "$ROOT" "$f" "$W/$n"
    [ -s "$W/$n.wasm" ] && [ -s "$W/$n.anchors" ] && cut_props "$W/$n.wasm" "$W/$n.anchors" "$W/$n"
  }
  export -f one
  export ROOT
  t0=$(date +%s)
  xargs -P 4 -I{} bash -c 'one "$1"' _ {} < "$LIST"
  echo "== emitted in $(( $(date +%s) - t0 )) s: $(ls "$W" | grep -c '\.wasm$') module(s); not emitted $(wc -l < "$OUT/not-emitted.txt" | tr -d ' '), a cutter failed $(sort -u "$OUT/cutter-failed.txt" | wc -l | tr -d ' ')"
  while IFS= read -r f; do
    n="$(name_of "$f")"
    [ -s "$W/$n.wasm" ] && [ -f "$W/$n.fresh" ] && [ -f "$W/$n.windows" ] || continue
    printf '%s\t%s\t%s\t%s\n' "$f" "$W/$n.wasm" "$W/$n.fresh" "$W/$n.windows" >> "$OUT/modules-in.tsv"
  done < "$LIST"
  judge_all
  exit 0
fi

[ "$MODE" = pairs ] || { echo "unknown mode: $MODE"; exit 2; }
HELD="$(cd "${3:?the held checkout}" && pwd)"
SETS="${4:?sets.tsv}"
REPLAYS="${5:-}"
[ -x "$HELD/target/release/almide" ] && [ -x "$HELD/target/release/examples/emit_structural_bundle" ] || { echo "the held checkout has no built compiler"; exit 1; }
rm -f "$HELD/target/release/almide-verify"
mkdir -p "$W/base" "$W/mut" "$W/replay"

# jobs.tsv: set, mutant id, program name, module
: > "$OUT/jobs.tsv"
while IFS=$'\t' read -r set dir ids; do
  [ -n "$set" ] || continue
  find "$dir" -type f -path '*/emut/modules/*/*.wasm' | sort | while IFS= read -r m; do
    id="$(basename "$(dirname "$m")")"
    if [ -n "${ids:-}" ]; then case ",$ids," in *",$id,"*) ;; *) continue ;; esac; fi
    printf '%s\t%s\t%s\t%s\n' "$set" "$id" "$(basename "$m" .wasm)" "$m"
  done >> "$OUT/jobs.tsv"
  echo "set $set: $(awk -F'\t' -v s="$set" '$1 == s' "$OUT/jobs.tsv" | wc -l | tr -d ' ') kept module(s) of $(awk -F'\t' -v s="$set" '$1 == s { print $2 }' "$OUT/jobs.tsv" | sort -u | wc -l | tr -d ' ') mutant(s)"
done < "$SETS"

# The unmutated module of every program a kept module belongs to.
cut -f3 "$OUT/jobs.tsv" | sort -u > "$OUT/names.txt"
grep -v -E '^(#|$)' "$HELD/crates/almide-wasm/tests/golden/witness-fixtures.txt" | while IFS= read -r f; do
  printf '%s\t%s\n' "$(name_of "$f")" "$f"
done | sort > "$OUT/all-names.tsv"
join -t "$(printf '\t')" "$OUT/names.txt" "$OUT/all-names.tsv" > "$OUT/base-list.tsv"
echo "programs behind the kept modules: $(wc -l < "$OUT/base-list.tsv" | tr -d ' ') of $(wc -l < "$OUT/names.txt" | tr -d ' ') name(s)"
base_one() {
  local n="$1" f="$2"
  emit "$HELD" "$f" "$W/base/$n"
  [ -s "$W/base/$n.wasm" ] && [ -s "$W/base/$n.anchors" ] && cut_props "$W/base/$n.wasm" "$W/base/$n.anchors" "$W/base/$n"
}
mut_one() {
  local set="$1" id="$2" n="$3" m="$4" stem
  stem="$W/mut/$set.$id.$n"
  if [ -s "$W/base/$n.wasm" ] && [ -s "$W/base/$n.anchors" ] \
     && python3 -I "$TOOL" alloc-like "$W/base/$n.wasm" "$W/base/$n.anchors" "$m" "$stem.anchors" > "$stem.alloc" 2>&1; then
    cut_props "$m" "$stem.anchors" "$stem"
  else
    cut_props "$m" - "$stem"
  fi
}
export -f base_one mut_one
export HELD
t0=$(date +%s)
tr '\t' '\n' < "$OUT/base-list.tsv" | xargs -P 4 -n 2 bash -c 'base_one "$1" "$2"' _
echo "== unmutated modules emitted in $(( $(date +%s) - t0 )) s: $(ls "$W/base" | grep -c '\.wasm$'); not emitted $(wc -l < "$OUT/not-emitted.txt" | tr -d ' ')"
t0=$(date +%s)
tr '\t' '\n' < "$OUT/jobs.tsv" | xargs -P 4 -n 4 bash -c 'mut_one "$1" "$2" "$3" "$4"' _
echo "== kept modules cut in $(( $(date +%s) - t0 )) s; allocator: $(cat "$W"/mut/*.alloc 2> /dev/null | cut -d' ' -f1 | sort | uniq -c | tr '\n' ' ')"

while IFS=$'\t' read -r n f; do
  [ -s "$W/base/$n.wasm" ] && [ -f "$W/base/$n.fresh" ] || continue
  printf 'base|%s\t%s\t%s\t%s\n' "$n" "$W/base/$n.wasm" "$W/base/$n.fresh" "$W/base/$n.windows" >> "$OUT/modules-in.tsv"
done < "$OUT/base-list.tsv"
while IFS=$'\t' read -r set id n m; do
  stem="$W/mut/$set.$id.$n"
  [ -f "$stem.windows" ] || continue
  printf '%s|%s|%s\t%s\t%s\t%s\n' "$set" "$id" "$n" "$m" "$stem.fresh" "$stem.windows" >> "$OUT/modules-in.tsv"
done < "$OUT/jobs.tsv"

# The replayed defects: both modules carry their own anchor file.
if [ -n "$REPLAYS" ] && [ -d "$REPLAYS" ]; then
  k=0
  for d in "$REPLAYS"/*/*.base; do
    [ -d "$d" ] || continue
    n="$(basename "$d" .base)"
    b="$(dirname "$d")/$n.bug"
    r="$(basename "$(dirname "$d")")"
    [ -s "$d/a.anchors.wasm" ] && [ -s "$d/a.anchors" ] && [ -s "$b/a.anchors.wasm" ] && [ -s "$b/a.anchors" ] || { echo "replay incomplete: $r $n"; continue; }
    k=$((k + 1))
    cut_props "$d/a.anchors.wasm" "$d/a.anchors" "$W/replay/$r.$n.base"
    cut_props "$b/a.anchors.wasm" "$b/a.anchors" "$W/replay/$r.$n.bug"
    printf 'replay|%s|%s|base\t%s\t%s\t%s\n' "$r" "$n" "$d/a.anchors.wasm" "$W/replay/$r.$n.base.fresh" "$W/replay/$r.$n.base.windows" >> "$OUT/modules-in.tsv"
    printf 'replay|%s|%s\t%s\t%s\t%s\n' "$r" "$n" "$b/a.anchors.wasm" "$W/replay/$r.$n.bug.fresh" "$W/replay/$r.$n.bug.windows" >> "$OUT/modules-in.tsv"
  done
  echo "== replayed defects: $k pair(s)"
fi
echo "== a cutter failed on $(sort -u "$OUT/cutter-failed.txt" | wc -l | tr -d ' ') module(s)"

judge_all
echo "== with and without the mutant or the defect"
python3 -I "$TOOL" compare "$OUT" || exit 1
