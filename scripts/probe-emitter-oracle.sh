#!/usr/bin/env bash
# Probe only, not a gate: a dynamic heap oracle for source mutants of the
# wasm emitter.
#
#   scripts/probe-emitter-oracle.sh <out-dir> <mutants-dir> <shard> <shards>
#
# The oracle is the project's checked allocator (scripts/wasm-checked-alloc.sh,
# #4009): under ALMIDE_WASM_CHECKED_ALLOC `$free` poisons a freed block,
# `$alloc` checks the poison before reuse, and every block carries a canary
# past its usable capacity. A program that writes a freed block or past the
# end of a block exits 70; one that reads a freed block sees poison. The
# script builds each program twice with ONE compiler, ordinary and checked,
# and reports every program whose two runs differ (`DIFF`). Its two negative
# controls and its sandbox control run first, under the same compiler.
#
# Here that script is run (a) with the unmutated compiler on this shard's
# share of the contract programs, the control that the oracle is quiet, and
# (b) per mutant, with the mutated compiler, on every contract program whose
# module the mutant changes. <mutants-dir> holds index.tsv and <id>.diff.
#
# Writes <out-dir>/oracle.tsv (id, status, programs whose module changed,
# built, identical, different, of them exit 70, first DIFF) and
# <out-dir>/detail/<id>.txt (the script's output).
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
mkdir -p "$1"
OUT="$(cd "$1" && pwd)"
MUT="$(cd "$2" && pwd)"
SHARD="${3:-0}"
SHARDS="${4:-1}"
TD="${CARGO_TARGET_DIR:-$ROOT/target}"
export ALMIDE="$TD/release/almide"
export RUN_TIMEOUT="${RUN_TIMEOUT:-20}"
T_BUILD="${T_BUILD:-1500}"
T_ORACLE="${T_ORACLE:-2700}"
LIST="$OUT/fixtures.txt"
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt > "$LIST"
mkdir -p "$OUT/base" "$OUT/work" "$OUT/detail"

name_of() { printf '%s' "$1" | tr '/' '_'; }
emit() {
  local f="$1" d="$2" n
  n="$(name_of "$f")"
  ( ulimit -v 6000000; timeout 120 "$ALMIDE" build "$f" --target wasm -o "$d/$n.wasm" > /dev/null 2>&1 < /dev/null ) || rm -f "$d/$n.wasm"
}
export -f name_of emit
emit_all() { xargs -P 4 -I{} bash -c 'emit "$1" "$2"' _ {} "$1" < "$LIST"; }
build() { timeout "$T_BUILD" cargo build -q --release --bin almide > "$OUT/build.log" 2>&1; }
# summary <file>: "built identical different exit70 first-diff" of one run of the oracle
summary() {
  local s b i d e first
  s="$(grep -m1 '^wasm-checked-alloc: ' "$1" || true)"
  b="$(printf '%s' "$s" | sed -n 's/^wasm-checked-alloc: \([0-9]*\) built.*/\1/p')"
  i="$(printf '%s' "$s" | sed -n 's/.* \([0-9]*\) identical.*/\1/p')"
  d="$(grep -c '^DIFF ' "$1" || true)"
  e="$(grep -c '^DIFF .* -> 70$' "$1" || true)"
  first="$(grep -m1 '^DIFF ' "$1" | cut -c1-160 | tr '\t' ' ' || true)"
  printf '%s\t%s\t%s\t%s\t%s' "${b:--}" "${i:--}" "$d" "$e" "${first:--}"
}

# oracle_corpus <out-file> <program>...: the corpus loop of
# scripts/wasm-checked-alloc.sh, without its three controls. A mutant can break
# a control (its program no longer builds) and still build corpus programs;
# the controls belong to the unmutated compiler, where step (a) runs them.
oracle_corpus() {
  local outf="$1" tmp built=0 same=0 diff=0 skipped=0 f x
  shift
  tmp="$(mktemp -d)"
  for f in "$@"; do
    if ! env -u ALMIDE_WASM_CHECKED_ALLOC "$ALMIDE" build "$f" --target wasm -o "$tmp/a.wasm" > /dev/null 2>&1; then skipped=$((skipped + 1)); continue; fi
    if ! ALMIDE_WASM_CHECKED_ALLOC=1 "$ALMIDE" build "$f" --target wasm -o "$tmp/b.wasm" > /dev/null 2>&1; then
      echo "DIFF $f: builds ordinarily but not under the switch" >> "$outf"; diff=$((diff + 1)); continue
    fi
    built=$((built + 1))
    for x in a b; do
      rm -rf "$tmp/troot"; mkdir "$tmp/troot"
      ( cd "$tmp" && timeout "$RUN_TIMEOUT" wasmtime run --dir=. --dir="$tmp/troot::/tmp" "$tmp/$x.wasm" < /dev/null > "$tmp/$x.out" 2> "$tmp/$x.err"; echo $? > "$tmp/$x.rc" )
    done
    if cmp -s "$tmp/a.out" "$tmp/b.out" && cmp -s "$tmp/a.err" "$tmp/b.err" && cmp -s "$tmp/a.rc" "$tmp/b.rc"; then
      same=$((same + 1))
    else
      diff=$((diff + 1))
      echo "DIFF $f: exit $(cat "$tmp/a.rc") -> $(cat "$tmp/b.rc")" >> "$outf"
      echo "    $(head -c 300 "$tmp/b.err" | tr '\n' ' ')" >> "$outf"
    fi
  done
  echo "wasm-checked-alloc: $built built, $same identical, $diff different, $skipped not built (no wasm build)" >> "$outf"
  rm -rf "$tmp"
}

export -f oracle_corpus
build || { echo "baseline build failed"; tail -n 20 "$OUT/build.log"; exit 1; }
"$ALMIDE" --version
emit_all "$OUT/base"
echo "unmutated compiler: $(ls "$OUT/base" | grep -c '\.wasm$') of $(wc -l < "$LIST" | tr -d ' ') module(s) built"

printf 'id\tstatus\tchanged\tbuilt\tidentical\tdifferent\texit70\tfirst\n' > "$OUT/oracle.tsv"
# (a) the unmutated compiler on this shard's share of the programs
mapfile -t ALL < "$LIST"
WASM_CHECKED_SHARD="$SHARD/$SHARDS" timeout "$T_ORACLE" bash scripts/wasm-checked-alloc.sh "${ALL[@]}" > "$OUT/detail/base.txt" 2>&1
rc=$?
printf 'base\trc%s\t-\t%s\n' "$rc" "$(summary "$OUT/detail/base.txt")" >> "$OUT/oracle.tsv"
tail -n 3 "$OUT/detail/base.txt"

# (b) every mutant, on the programs whose module it changes
while IFS=$'\t' read -r id file line genre name; do
  [ -n "$id" ] || continue
  t0=$(date +%s)
  if ! git apply "$MUT/$id.diff" 2> /dev/null; then
    printf '%s\tno-apply\t-\t-\t-\t-\t-\t-\n' "$id" >> "$OUT/oracle.tsv"; continue
  fi
  if ! build; then
    printf '%s\tno-build\t-\t-\t-\t-\t-\t-\n' "$id" >> "$OUT/oracle.tsv"
    git apply -R "$MUT/$id.diff"; continue
  fi
  find "$OUT/work" -name '*.wasm' -delete
  emit_all "$OUT/work"
  CH=()
  while IFS= read -r f; do
    n="$(name_of "$f")"
    [ -s "$OUT/base/$n.wasm" ] && [ -s "$OUT/work/$n.wasm" ] || continue
    cmp -s "$OUT/base/$n.wasm" "$OUT/work/$n.wasm" || CH+=("$f")
  done < "$LIST"
  if [ "${#CH[@]}" -eq 0 ]; then
    printf '%s\tunchanged\t0\t-\t-\t-\t-\t-\n' "$id" >> "$OUT/oracle.tsv"
  else
    timeout "$T_ORACLE" bash scripts/wasm-checked-alloc.sh "${CH[@]}" > "$OUT/detail/$id.txt" 2>&1
    rc=$?
    st="rc$rc"
    grep -q '^wasm-checked-alloc: ' "$OUT/detail/$id.txt" || st="no-verdict-rc$rc"
    if grep -q '^FAIL' "$OUT/detail/$id.txt"; then
      # a control failed under this mutant: the corpus loop alone
      mv "$OUT/detail/$id.txt" "$OUT/detail/$id.controls.txt"
      : > "$OUT/detail/$id.txt"
      timeout "$T_ORACLE" bash -c 'oracle_corpus "$@"' _ "$OUT/detail/$id.txt" "${CH[@]}"
      st="controls-skipped-rc$?"
    fi
    printf '%s\t%s\t%s\t%s\n' "$id" "$st" "${#CH[@]}" "$(summary "$OUT/detail/$id.txt")" >> "$OUT/oracle.tsv"
  fi
  git apply -R "$MUT/$id.diff"
  echo "$id: $(tail -n 1 "$OUT/oracle.tsv" | cut -f2-7 | tr '\t' ' ') ($(( $(date +%s) - t0 )) s)"
done < "$MUT/index.tsv"
build > /dev/null 2>&1 || true
