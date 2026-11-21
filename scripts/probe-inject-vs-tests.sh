#!/usr/bin/env bash
# Probe only: does running the behaviour corpus notice an injected emitter
# defect? For each reinject patch of the wasm witnesses, build the compiler
# with the patch and run every fixture of the witness list on the wasm
# target; a fixture whose stdout or exit code differs from the unpatched
# compiler's is one the behaviour tests would have caught.
#
#   scripts/probe-inject-vs-tests.sh <out-dir> <shard> <shards>
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
OUT="${1:?usage: scripts/probe-inject-vs-tests.sh <out-dir> <shard> <shards>}"
SHARD="${2:?}"
SHARDS="${3:?}"
mkdir -p "$OUT/base" "$OUT/run"
OUT="$(cd "$OUT" && pwd)"
LIST="$OUT/fixtures.txt"
grep -v -E '^(#|$)' crates/almide-wasm/tests/golden/witness-fixtures.txt > "$LIST"

build() { cargo build -q --release --bin almide; }

# one <almd> <dir>: stdout and exit code of the program on the wasm target.
one() {
  local f="$1" d="$2" n
  n="$(printf '%s' "$f" | tr '/' '_')"
  timeout 60 ./target/release/almide run "$f" --target wasm > "$d/$n.out" 2> "$d/$n.err" < /dev/null
  echo "EXIT $?" >> "$d/$n.out"
}
export -f one
runall() { xargs -P 4 -I{} bash -c 'one "$1" "$2"' _ {} "$1" < "$LIST"; }

build || { echo "baseline build failed"; exit 1; }
t0=$(date +%s)
runall "$OUT/base"
echo "baseline: $(wc -l < "$LIST") fixtures in $(( $(date +%s) - t0 )) s; exit codes: $(cat "$OUT"/base/*.out | grep -c '^EXIT 0$') zero, $(cat "$OUT"/base/*.out | grep '^EXIT ' | grep -vc '^EXIT 0$') non-zero"

: > "$OUT/results.tsv"
i=0
for fam in bounds init fields rc anchors; do
  for p in proofs/fixtures/$fam-witness/reinject/*.patch; do
    name="$(basename "$p" .patch)"
    i=$((i + 1))
    [ $(( i % SHARDS )) -eq "$SHARD" ] || continue
    if ! git apply "$p"; then
      printf '%s\t%s\tno-apply\t-\t-\t-\t-\n' "$fam" "$name" >> "$OUT/results.tsv"
      continue
    fi
    if ! build 2> "$OUT/build.err"; then
      printf '%s\t%s\tno-build\t-\t-\t-\t%s\n' "$fam" "$name" "$(head -1 "$OUT/build.err" | tr '\t' ' ')" >> "$OUT/results.tsv"
      git apply -R "$p"
      continue
    fi
    rm -rf "$OUT/run"; mkdir -p "$OUT/run"
    runall "$OUT/run"
    diffs=0; first=""; note=""
    while IFS= read -r f; do
      n="$(printf '%s' "$f" | tr '/' '_')"
      if ! cmp -s "$OUT/base/$n.out" "$OUT/run/$n.out"; then
        diffs=$((diffs + 1))
        if [ -z "$first" ]; then
          first="$f"
          note="$(tail -1 "$OUT/run/$n.out") | $(head -1 "$OUT/run/$n.err" | cut -c1-160 | tr '\t' ' ')"
        fi
      fi
    done < "$LIST"
    # The patch's own reproduction, unpatched output against patched.
    repro="proofs/fixtures/$fam-witness/reinject/$name.almd"
    rd="-"
    if [ -f "$repro" ]; then
      mkdir -p "$OUT/repro"
      one "$repro" "$OUT/run"
      rn="$(printf '%s' "$repro" | tr '/' '_')"
      cp "$OUT/run/$rn.out" "$OUT/repro/$fam-$name.patched.out"; cp "$OUT/run/$rn.err" "$OUT/repro/$fam-$name.patched.err"
      rd="patched: $(tail -1 "$OUT/run/$rn.out") $(head -1 "$OUT/run/$rn.err" | cut -c1-120 | tr '\t' ' ')"
    fi
    printf '%s\t%s\tok\t%s\t%s\t%s\t%s\n' "$fam" "$name" "$diffs" "${first:--}" "${note:--}" "$rd" >> "$OUT/results.tsv"
    git apply -R "$p"
  done
done
# The reproductions under the unpatched compiler (rebuilt), for the comparison.
build
mkdir -p "$OUT/repro"
while IFS=$'\t' read -r fam name st rest; do
  repro="proofs/fixtures/$fam-witness/reinject/$name.almd"
  [ -f "$repro" ] || continue
  one "$repro" "$OUT/run"
  rn="$(printf '%s' "$repro" | tr '/' '_')"
  cp "$OUT/run/$rn.out" "$OUT/repro/$fam-$name.base.out"
  if [ -f "$OUT/repro/$fam-$name.patched.out" ]; then
    if cmp -s "$OUT/repro/$fam-$name.base.out" "$OUT/repro/$fam-$name.patched.out"; then echo "REPRO $fam $name same"; else echo "REPRO $fam $name DIFFERS (base: $(tail -1 "$OUT/repro/$fam-$name.base.out"))"; fi
  fi
done < "$OUT/results.tsv"
echo "== family, patch, status, fixtures whose behaviour changed, first such fixture, its exit and first error line, the reproduction under the patch"
cat "$OUT/results.tsv"
