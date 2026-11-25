#!/usr/bin/env bash
# Probe only: programs written to make one surviving source mutant of the
# emitter visible, run against that mutant and against the others read as
# harmful.
#
# The programs are scripts/probe-emitter-kill/*.almd. Each is compiled and run
# twice per compiler: by the driver's own embedded runtime
# (`almide run --target wasm`) and, built to a file, by a stock runtime
# (`wasmtime run`). A mutant NOTICES a program when the compile fails where the
# unmutated compiler's did not, or stdout or the exit code differ.
#
#   scripts/probe-emitter-kill.sh <out-dir> <mutants-dir>
# <mutants-dir> holds index.tsv (id, file, line, genre, name) and <id>.diff.
set -uo pipefail
export LC_ALL=C

OUT="$1"
MUT="$2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
MUT="$(cd "$MUT" && pwd)"
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
ALMIDE="$TD/release/almide"
T_RUN="${T_RUN:-180}"
PROGS="$(ls scripts/probe-emitter-kill/*.almd)"

PRODUCER="$TD/release/examples/emit_structural_bundle"
VERIFY="$OUT/bin/almide-verify"
ANCHORCHECK="$OUT/bin/almide-anchor-check"

# The compiler and the witness producer: both hold the emitter, so a mutant
# rebuilds both.
build() {
  cargo build -q --release --locked --bin almide 2> "$OUT/build.err" \
    && cargo build -q --release --locked -p almide-wasm --example emit_structural_bundle 2>> "$OUT/build.err"
}

# judge <dir>: per program, the module with the witness bundle and the anchor
# file this compiler emits for it, and what the proved checkers and the
# byte-level check answer (<dir>/judge/<name>.vrc and .arc hold their exit
# codes, .verify and .anchor their whole output).
judge() {
  local d="$1/judge" p n rc v a
  mkdir -p "$d"
  for p in $INSPECT $PROGS; do
    n="$(basename "$p" .almd)"
    rc=0; v=-; a=-
    timeout "$T_RUN" "$ALMIDE" build "$p" --target wasm -o "$d/$n.wasm" > "$d/$n.cerr" 2>&1 < /dev/null || rc=$?
    if [ "$rc" -eq 0 ]; then
      timeout "$T_RUN" "$PRODUCER" "$p" --artifact "$d/$n.wasm" --anchors "$d/$n.anchors" > "$d/$n.bundle" 2> "$d/$n.perr" < /dev/null || rc=$((1000 + $?))
    fi
    if [ "$rc" -eq 0 ]; then
      v=0; a=0
      timeout "$T_RUN" "$VERIFY" bundle "$d/$n.bundle" > "$d/$n.verify" 2>&1 < /dev/null || v=$?
      timeout "$T_RUN" "$ANCHORCHECK" --census "$d/$n.wasm" "$d/$n.bundle" "$d/$n.anchors" > "$d/$n.anchor" 2>&1 < /dev/null || a=$?
    fi
    echo "$rc" > "$d/$n.rc"; echo "$v" > "$d/$n.vrc"; echo "$a" > "$d/$n.arc"
    printf -- '-- %s: emit %s, proved checkers %s, byte-level check %s\n' "$n" "$rc" "$v" "$a"
    [ "$a" = 0 ] || [ "$a" = - ] || grep -v '^census' "$d/$n.anchor" | head -n 6 | cut -c1-240
  done
}

# INSPECT: programs (the ones above, and any contract programs named here)
# whose module each compiler also builds and prints as text, so that what a
# mutant changes in the emitted code can be read side by side.
INSPECT="${INSPECT:-}"

# inspect <dir>: <dir>/inspect/<name>.wasm and .wat per program.
inspect() {
  local d="$1/inspect" p n
  [ -n "$INSPECT" ] || return 0
  mkdir -p "$d"
  for p in $INSPECT $PROGS; do
    n="$(basename "$p" .almd)"
    if timeout "$T_RUN" "$ALMIDE" build "$p" --target wasm -o "$d/$n.wasm" > "$d/$n.build" 2>&1 < /dev/null; then
      wasm-tools print "$d/$n.wasm" > "$d/$n.wat" 2> "$d/$n.print.err" || echo "inspect: $n does not print"
    else
      echo "inspect: $n does not build"
    fi
  done
}

# inspect_diff <id>: which inspected modules the mutant changes, and how.
inspect_diff() {
  local id="$1" p n
  [ -n "$INSPECT" ] || return 0
  echo "== $id: inspected modules"
  for p in $INSPECT $PROGS; do
    n="$(basename "$p" .almd)"
    if cmp -s "$OUT/base/inspect/$n.wasm" "$OUT/$id/inspect/$n.wasm"; then
      echo "-- $n: module identical"
    else
      echo "-- $n: module differs ($(diff "$OUT/base/inspect/$n.wat" "$OUT/$id/inspect/$n.wat" | grep -c '^[<>]') changed line(s))"
      diff -U 12 "$OUT/base/inspect/$n.wat" "$OUT/$id/inspect/$n.wat" > "$OUT/$id/inspect/$n.diff"
      head -n 400 "$OUT/$id/inspect/$n.diff"
    fi
  done
}

# run_all <dir>: per program, <name>.embedded and <name>.stock hold stdout and
# a last line `EXIT <code>`; a compile that fails leaves `COMPILE <code>`.
run_all() {
  local d="$1" p n rc
  mkdir -p "$d"
  for p in $PROGS; do
    n="$(basename "$p" .almd)"
    rc=0
    timeout "$T_RUN" "$ALMIDE" run "$p" --target wasm > "$d/$n.embedded" 2> "$d/$n.embedded.err" < /dev/null || rc=$?
    echo "EXIT $rc" >> "$d/$n.embedded"
    rc=0
    timeout "$T_RUN" "$ALMIDE" build "$p" --target wasm -o "$d/$n.wasm" > "$d/$n.build" 2>&1 < /dev/null || rc=$?
    if [ "$rc" -ne 0 ]; then
      echo "COMPILE $rc" > "$d/$n.stock"
    else
      rc=0
      timeout "$T_RUN" wasmtime run "$d/$n.wasm" > "$d/$n.stock" 2> "$d/$n.stock.err" < /dev/null || rc=$?
      echo "EXIT $rc" >> "$d/$n.stock"
    fi
  done
}

# cargo_tests <label>: the project's own integration tests named in
# CARGO_TESTS (`<test file>:<name filter>`), run against the compiler now
# built. One line per test file in <out>/cargo-tests.tsv.
cargo_tests() {
  local label="$1" t file filter rc
  for t in ${CARGO_TESTS:-}; do
    file="${t%%:*}"; filter="${t#*:}"
    rc=0
    timeout 2400 cargo test -q --release --locked --test "$file" "$filter" > "$OUT/$label.cargo-$file.out" 2>&1 < /dev/null || rc=$?
    printf '%s\t%s\t%s\t%s\n' "$label" "$t" "$rc" "$(grep -m1 -E '^test result' "$OUT/$label.cargo-$file.out" | cut -c1-120)" >> "$OUT/cargo-tests.tsv"
    echo "$label: cargo test $t -> exit $rc: $(grep -E '^test result|FAILED|panicked' "$OUT/$label.cargo-$file.out" | head -n 4 | tr '\n' ' ' | cut -c1-300)"
  done
}

t0=$(date +%s)
build || { echo "baseline build failed"; tail -40 "$OUT/build.err"; exit 1; }
echo "baseline build: $(( $(date +%s) - t0 )) s"
"$ALMIDE" --version
wasmtime --version
# The two checkers are built once, from the unmutated tree. The driver judges
# a build's certificate itself when it finds a verifier beside its own
# executable or on PATH, and then refuses to write a module the verifier
# rejects; each layer is asked separately here, so the driver must find none.
cargo build -q --release --locked -p almide-verify -p almide-anchor-check 2>> "$OUT/build.err" || { echo "checker build failed"; tail -20 "$OUT/build.err"; exit 1; }
mkdir -p "$OUT/bin"
cp "$TD/release/almide-verify" "$TD/release/almide-anchor-check" "$OUT/bin/"
rm -f "$TD/release/almide-verify"
if command -v almide-verify > /dev/null 2>&1; then echo "a verifier is on PATH: $(command -v almide-verify)"; exit 1; fi
run_all "$OUT/base"
inspect "$OUT/base"
echo "== the unmutated compiler: what the checkers answer"
judge "$OUT/base"
echo "== the unmutated compiler"
for p in $PROGS; do
  n="$(basename "$p" .almd)"
  echo "-- $n (embedded)"; cat "$OUT/base/$n.embedded" | cut -c1-200
  echo "-- $n (stock)"; cat "$OUT/base/$n.stock" | cut -c1-200
done
cargo_tests base

RES="$OUT/results.tsv"
printf 'id\tprogram\truntime\tnoticed\tbaseline_last\tmutant_last\terr\n' > "$RES"
while IFS=$'\t' read -r -u 3 id file line genre name; do
  [ -n "$id" ] || continue
  if ! git apply "$MUT/$id.diff" 2> "$OUT/apply.err"; then
    printf '%s\t-\t-\tno-apply\t-\t-\t-\n' "$id" >> "$RES"; echo "$id no-apply"; continue
  fi
  if build; then
    run_all "$OUT/$id"
    inspect "$OUT/$id"
    inspect_diff "$id"
    echo "== $id: what the checkers answer"
    judge "$OUT/$id"
    for p in $PROGS; do
      n="$(basename "$p" .almd)"
      for rt in embedded stock; do
        if cmp -s "$OUT/base/$n.$rt" "$OUT/$id/$n.$rt"; then noticed=no; else noticed=yes; fi
        b="$(tail -1 "$OUT/base/$n.$rt" | tr '\t' ' ' | cut -c1-80)"
        m="$(tail -1 "$OUT/$id/$n.$rt" | tr '\t' ' ' | cut -c1-80)"
        e="$( { cat "$OUT/$id/$n.$rt.err" 2>/dev/null; cat "$OUT/$id/$n.build" 2>/dev/null; } | grep -m1 -i -E 'error|trap|overflow|exhaust|unreachable|panic|out of|killed|signal' | tr '\t' ' ' | cut -c1-160)"
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$n" "$rt" "$noticed" "$b" "$m" "${e:--}" >> "$RES"
      done
    done
    echo "$id: $(awk -F'\t' -v i="$id" '$1 == i && $4 == "yes" { printf "%s/%s ", $2, $3 }' "$RES")"
    case " ${CARGO_TEST_IDS:-} " in *" $id "*) cargo_tests "$id" ;; esac
  else
    printf '%s\t-\t-\tno-build\t-\t-\t-\n' "$id" >> "$RES"; echo "$id no-build"; tail -3 "$OUT/build.err"
  fi
  git apply -R "$MUT/$id.diff"
done 3< "$MUT/index.tsv"

echo "== which mutant each program makes visible"
awk -F'\t' 'NR > 1 && $4 == "yes" { k = $1; a[k] = a[k] " " $2 "(" $3 ")" } END { for (k in a) print k ":" a[k] }' "$RES" | sort
echo "== mutants no program makes visible"
awk -F'\t' 'NR > 1 { all[$1] = 1; if ($4 == "yes") y[$1] = 1 } END { for (k in all) if (!(k in y)) print k }' "$RES" | sort | tr '\n' ' '; echo
