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
# Per mutant, detail/<id>.rows.tsv holds one row per program the mutant
# refused or changed (probe-emitter-mutants-rows.py names the columns), and
# detail/<id>.behave.tsv the programs that were run.
#
#   scripts/probe-emitter-mutants.sh <out-dir> <mutants-dir>
# <mutants-dir> holds index.tsv (id, file, line, genre, name) and <id>.diff.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
OUT="${1:?usage: scripts/probe-emitter-mutants.sh <out-dir> <mutants-dir>}"
MUT="${2:?}"
mkdir -p "$OUT/base" "$OUT/work" "$OUT/bin" "$OUT/detail"
OUT="$(cd "$OUT" && pwd)"
MUT="$(cd "$MUT" && pwd)"
LIST="$OUT/fixtures.txt"
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt > "$LIST"
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
ROWS="$ROOT/scripts/probe-emitter-mutants-rows.py"
WIN="$ROOT/scripts/probe-emitter-windows.py"
# A mutant's changed modules are kept (for rules written later) when it
# changes at most this many programs.
KEEP_MODULES="${KEEP_MODULES:-100}"
export ALMIDE="$TD/release/almide"
export PRODUCER="$TD/release/examples/emit_structural_bundle"
export VERIFY="$OUT/bin/almide-verify"
export ANCHORCHECK="$OUT/bin/almide-anchor-check"
export BASE="$OUT/base"
# Seconds one compile, one witness emission, one checker or one program run
# may take.
export T_EMIT="${T_EMIT:-60}"
export T_CHECK="${T_CHECK:-60}"
export T_RUN="${T_RUN:-30}"
# Address space, in kB, that one compile or witness emission, and one checker,
# may reserve. Four run at a time, so four times the larger of the two has to
# fit in the machine. The checkers were not limited in the first runs.
export V_EMIT="${V_EMIT:-6000000}"
export V_CHECK="${V_CHECK:-unlimited}"
# Seconds one mutant may take in all, after its build.
T_MUTANT="${T_MUTANT:-1500}"
# Of the first 40 programs, how many may time out in the compiler before the
# mutant is recorded as one that hangs it.
HANG_OF_40="${HANG_OF_40:-4}"

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
  ( ulimit -v "$V_EMIT"; timeout "$T_EMIT" "$ALMIDE" build "$f" --target wasm -o "$d/$n.wasm" > "$d/$n.cerr" 2>&1 < /dev/null ) || rc=$?
  if [ "$rc" -eq 0 ]; then
    ( ulimit -v "$V_EMIT"; timeout "$T_EMIT" "$PRODUCER" "$f" --artifact "$d/$n.wasm" --anchors "$d/$n.anchors" > "$d/$n.bundle" 2> "$d/$n.perr" < /dev/null ) || rc=$((1000 + $?))
  fi
  echo "$rc" > "$d/$n.rc"
}
export -f emit

# ask <almd> <dir>: both checkers on what was emitted there, each one's whole
# output and exit code kept. The byte-level check also prints its census, so
# that an acceptance can be read for how much it located.
ask() {
  local f="$1" d="$2" n
  n="$(name_of "$f")"
  ( ulimit -v "$V_CHECK"; timeout "$T_CHECK" "$VERIFY" bundle "$d/$n.bundle" > "$d/$n.verify" 2>&1 < /dev/null )
  echo $? > "$d/$n.vrc"
  ( ulimit -v "$V_CHECK"; timeout "$T_CHECK" "$ANCHORCHECK" --census "$d/$n.wasm" "$d/$n.bundle" "$d/$n.anchors" > "$d/$n.anchor" 2>&1 < /dev/null )
  echo $? > "$d/$n.arc"
}
export -f ask

# windows <almd> <dir>: the guarded-access windows the cutter proposes over
# every load and store of the module, from the module alone (no witness, no
# anchor file). Whether a proposal is accepted is the kernel's verdict, taken
# per distinct byte string by the job that follows the shards.
windows() {
  local f="$1" d="$2" n
  n="$(name_of "$f")"
  ( ulimit -v "$V_CHECK"; timeout "$T_CHECK" "$ANCHORCHECK" --windows "$d/$n.wasm" > "$d/$n.windows" 2>&1 < /dev/null )
  echo $? > "$d/$n.wrc"
}
export -f windows

# judge <almd> <dir>: what a mutant changed for this program (module, bundle
# without the line naming the module's hash, anchor file), and both checkers'
# answers where anything changed.
judge() {
  local f="$1" d="$2" n w=0 b=0 a=0
  n="$(name_of "$f")"
  [ "$(cat "$d/$n.rc" 2>/dev/null)" = 0 ] || return 0
  cmp -s "$BASE/$n.wasm" "$d/$n.wasm" || w=1
  cmp -s <(grep -v '^artifact ' "$BASE/$n.bundle") <(grep -v '^artifact ' "$d/$n.bundle") || b=1
  cmp -s "$BASE/$n.anchors" "$d/$n.anchors" || a=1
  echo "$w $b $a" > "$d/$n.cmp"
  [ "$w" = 0 ] || windows "$f" "$d"
  [ "$w$b$a" != 000 ] || return 0
  ask "$f" "$d"
}
export -f judge

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
ask_all() { xargs -P 4 -I{} bash -c 'ask "$1" "$2"' _ {} "$1" < "$2"; }
windows_all() { xargs -P 4 -I{} bash -c 'windows "$1" "$2"' _ {} "$1" < "$2"; }
judge_all() { xargs -P 4 -I{} bash -c 'judge "$1" "$2"' _ {} "$1" < "$2"; }
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
# A program whose output does not end in a newline has its exit line joined
# to the last output line, so match the line's end, not the whole line.
zero=$(cat "$OUT"/work/*.out | grep -c 'EXIT 0$')
echo "baseline behaviour: $zero program(s) exit 0, $(cat "$OUT"/work/*.out | grep -E 'EXIT [0-9]+$' | grep -vc 'EXIT 0$') exit non-zero"
[ "$zero" -ge 800 ] || { echo "too few programs run to completion under the unmutated compiler; the comparison would be empty"; exit 1; }
ask_all "$OUT/work" "$LIST"
windows_all "$OUT/work" "$LIST"
bad=0
while IFS= read -r f; do
  n="$(name_of "$f")"
  [ "$(cat "$OUT/work/$n.rc")" = 0 ] || { bad=$((bad + 1)); echo "baseline does not emit: $f"; continue; }
  [ "$(cat "$OUT/work/$n.vrc")" = 0 ] || { bad=$((bad + 1)); echo "baseline: proven checkers do not certify $f (exit $(cat "$OUT/work/$n.vrc"))"; }
  [ "$(cat "$OUT/work/$n.arc")" = 0 ] || { bad=$((bad + 1)); echo "baseline: byte-level check does not accept $f (exit $(cat "$OUT/work/$n.arc"))"; }
done < "$LIST"
cp -a "$OUT/work/." "$OUT/base/"
python3 -I "$ROWS" --baseline "$OUT" "$LIST" "$OUT/detail/_baseline.rows.tsv" || { echo "baseline rows could not be written"; exit 1; }
python3 -I "$WIN" "$OUT" "$LIST" --baseline || { echo "the cutter gave no ACCESSES line for some baseline module; a missing window would not mean a mutant caused it"; exit 1; }
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
printf 'id\tfile\tline\tgenre\tstatus\tprograms\tc_timeout\tc_compiler\tc_producer\twasm_diff\tbundle_only\tanchors_only\tp_reject\tp_incomplete\tp_other\tb_reject\tb_other\tfirst_compile_fail\tfirst_proven\tfirst_byte\tbehaviour_diff\tbehaviour_run\tseconds\tfirst_behaviour\tname\n' > "$RES"
# What counts.tsv holds for a mutant that was judged: twelve counts, then the
# first message of the compiler, the proven checkers and the byte-level check.
NOTHING=$'0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t-\t-\t-'

one_mutant() {
  local id="$1" file="$2" line="$3" genre="$4" name="$5"
  local d="$OUT/work" t0 status=ok
  t0=$(date +%s)
  # row <status> <counts and first messages> <behaviour differs> <behaviour run> <first behaviour>
  row() { printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$file" "$line" "$genre" "$1" "$2" "$3" "$4" "$(( $(date +%s) - t0 ))" "$5" "$name" >> "$RES"; }
  if ! git apply "$MUT/$id.diff" 2> "$OUT/apply.err"; then
    row no-apply "$NOTHING" 0 0 -
    return
  fi
  if ! build 2> "$OUT/build.err"; then
    cp "$OUT/build.err" "$OUT/detail/$id.build.err"
    row no-build "$NOTHING" 0 0 -
    git apply -R "$MUT/$id.diff"
    return
  fi
  no_verifier || { row verifier-present "$NOTHING" 0 0 -; git apply -R "$MUT/$id.diff"; return; }
  rm -rf "$d"; mkdir -p "$d"
  local deadline=$(( $(date +%s) + T_MUTANT ))
  # A mutant that hangs the compiler would take an hour to say so: emit a
  # first slice, and stop if enough of it timed out.
  head -40 "$LIST" > "$OUT/slice.txt"
  tail -n +41 "$LIST" > "$OUT/rest.txt"
  emit_all "$d" "$OUT/slice.txt"
  local hung
  hung=$(cat "$d"/*.rc | grep -c -E '^(124|1124)$' || true)
  if [ "$hung" -ge "$HANG_OF_40" ]; then
    row hang $'40\t'"$hung"$'\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t-\t-\t-' 0 0 -
    git apply -R "$MUT/$id.diff"
    return
  fi
  emit_all "$d" "$OUT/rest.txt"
  judge_all "$d" "$LIST"
  if ! python3 -I "$ROWS" "$OUT" "$LIST" "$OUT/rows.tsv"; then
    row no-rows "$NOTHING" 0 0 -
    git apply -R "$MUT/$id.diff"
    return
  fi
  python3 -I "$WIN" "$OUT" "$LIST" "$id" || true
  if [ -s "$OUT/windows/$id.tsv" ] && [ "$(wc -l < "$OUT/windows/$id.tsv")" -le "$KEEP_MODULES" ]; then
    mkdir -p "$OUT/modules/$id"
    while IFS=$'\t' read -r f _rest; do
      cp "$d/$(name_of "$f").wasm" "$OUT/modules/$id/" 2> /dev/null || true
    done < "$OUT/windows/$id.tsv"
  fi
  local bdiff=0 brun=0 ft="-" tail_of
  # Behaviour, for the programs whose module changed. Stop counting once
  # three differ: a mutant that hangs every program would otherwise cost
  # T_RUN for each of them.
  : > "$OUT/behave.tsv"
  # BEHAVE_FIRST: a file of programs to run before the rest of the changed
  # ones, for a question about particular programs under a mutant that
  # changes hundreds. Their outputs are kept beside the detail rows.
  if [ -s "$OUT/changed.txt" ] && [ -n "${BEHAVE_FIRST:-}" ] && [ -s "$BEHAVE_FIRST" ]; then
    { grep -F -x -f "$BEHAVE_FIRST" "$OUT/changed.txt" || true
      grep -F -x -v -f "$BEHAVE_FIRST" "$OUT/changed.txt" || true; } > "$OUT/changed.next"
    mv "$OUT/changed.next" "$OUT/changed.txt"
  fi
  if [ -s "$OUT/changed.txt" ]; then
    while [ -s "$OUT/changed.txt" ] && [ "$bdiff" -lt 3 ] && [ "$(date +%s)" -lt "$deadline" ]; do
      head -24 "$OUT/changed.txt" > "$OUT/batch.txt"
      tail -n +25 "$OUT/changed.txt" > "$OUT/changed.next"; mv "$OUT/changed.next" "$OUT/changed.txt"
      behave_all "$d" "$OUT/batch.txt"
      while IFS= read -r f; do
        local n; n="$(name_of "$f")"
        brun=$((brun + 1))
        if cmp -s "$OUT/base/$n.out" "$d/$n.out"; then
          printf '%s\tsame\t-\n' "$f" >> "$OUT/behave.tsv"
        else
          bdiff=$((bdiff + 1))
          tail_of="$(tail -c 60 "$d/$n.out" | tr '\t\n' '  ') | $(head -1 "$d/$n.err" | cut -c1-160 | tr '\t' ' ')"
          printf '%s\tdiffers\t%s\n' "$f" "$tail_of" >> "$OUT/behave.tsv"
          [ "$ft" != "-" ] || ft="$f: $tail_of"
        fi
      done < "$OUT/batch.txt"
    done
    [ ! -s "$OUT/changed.txt" ] || [ "$bdiff" -ge 3 ] || status=behaviour-unfinished
  fi
  row "$status" "$(cat "$OUT/counts.tsv")" "$bdiff" "$brun" "$ft"
  if [ "$(wc -l < "$OUT/rows.tsv")" -gt 1 ]; then
    cp "$OUT/rows.tsv" "$OUT/detail/$id.rows.tsv"
    [ ! -s "$OUT/behave.tsv" ] || cp "$OUT/behave.tsv" "$OUT/detail/$id.behave.tsv"
    if [ -n "${BEHAVE_FIRST:-}" ] && [ -s "$BEHAVE_FIRST" ]; then
      mkdir -p "$OUT/detail/$id.first"
      while IFS= read -r f; do
        local n; n="$(name_of "$f")"
        [ -f "$d/$n.out" ] || continue
        head -c 4000 "$OUT/base/$n.out" > "$OUT/detail/$id.first/$n.base.out" 2> /dev/null || true
        head -c 4000 "$d/$n.out" > "$OUT/detail/$id.first/$n.mutant.out"
        head -c 2000 "$d/$n.err" > "$OUT/detail/$id.first/$n.mutant.err" 2> /dev/null || true
      done < "$BEHAVE_FIRST"
    fi
    { echo "== $id $file:$line $genre"; echo "$name"; cat "$MUT/$id.diff"; } > "$OUT/detail/$id.txt"
  fi
  git apply -R "$MUT/$id.diff"
}

while IFS=$'\t' read -r id file line genre name; do
  [ -n "$id" ] || continue
  echo "-- $id: $(awk '/MemAvailable/ { print int($2 / 1024) }' /proc/meminfo) MB of memory available"
  one_mutant "$id" "$file" "$line" "$genre" "$name"
  if ! git diff --quiet -- crates; then
    echo "tree not clean after mutant $id; restoring"
    git checkout -q -- crates
  fi
  tail -1 "$RES" | cut -f1-17
done < "$MUT/index.tsv"
echo "== done: $(($(wc -l < "$RES") - 1)) mutant(s)"
cut -f5 "$RES" | tail -n +2 | sort | uniq -c
