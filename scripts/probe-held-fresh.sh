#!/usr/bin/env bash
# Probe only, not a gate: the fresh-block rule (proofs/GuardFresh.v) on the
# modules a held-out sample of source mutants changed.
#
#   scripts/probe-held-fresh.sh <cutter> <held-checkout> <shards-dir> <out-dir>
#
# <held-checkout> is the tree the sample was drawn over, with its compiler and
# witness producer built (target/release). <shards-dir> holds the sample's
# artifacts: under any depth, `emut/modules/<mutant id>/<program name>.wasm`,
# one file per program whose module the mutant changed (kept only for mutants
# that change at most a hundred programs).
#
# The unmutated compiler emits every contract program again, with its anchor
# file. A mutant's module has no anchor file here, so its allocator is the
# function whose instructions equal, byte for byte, those of the function the
# unmutated module's anchor file names (`--fresh-like`). The kernel then
# judges every proposal of every module, mutated or not.
#
# Writes <out-dir>/dump (per module `MODULE <mutant id | base> <program name>`,
# `FRESHALLOC <index | none | unreadable>` and its `FRESHHEX` lines),
# <out-dir>/kernel/verdicts.tsv, and <out-dir>/held-fresh.tsv and
# held-fresh-programs.tsv (scripts/probe-held-fresh-join.py).
set -uo pipefail
export LC_ALL=C
CUT="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
HELD="$(cd "$2" && pwd)"
SH="$(cd "$3" && pwd)"
mkdir -p "$4"
OUT="$(cd "$4" && pwd)"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALMIDE="$HELD/target/release/almide"
PRODUCER="$HELD/target/release/examples/emit_structural_bundle"
[ -x "$CUT" ] && [ -x "$ALMIDE" ] && [ -x "$PRODUCER" ] || { echo "a binary is missing: $CUT, $ALMIDE or $PRODUCER"; exit 1; }
mkdir -p "$OUT/base" "$OUT/props/base"
LIST="$OUT/list.txt"
grep -v -E '^(#|$)' "$HELD/crates/almide-wasm/tests/golden/witness-fixtures.txt" > "$LIST"
echo "$(wc -l < "$LIST" | tr -d ' ') contract program(s)"

name_of() { printf '%s' "$1" | tr '/' '_'; }
# emit <almd>: the unmutated module and its anchor file.
emit() {
  local f="$1" n
  n="$(name_of "$f")"
  ( cd "$HELD" \
    && timeout 300 "$ALMIDE" build "$f" --target wasm -o "$OUT/base/$n.wasm" > "$OUT/base/$n.cerr" 2>&1 < /dev/null \
    && timeout 300 "$PRODUCER" "$f" --artifact "$OUT/base/$n.wasm" --anchors "$OUT/base/$n.anchors" > "$OUT/base/$n.bundle" 2> "$OUT/base/$n.perr" < /dev/null ) \
    || echo "$f" >> "$OUT/base-failed"
}
# propose <label> <module> <program name>: the proposals of one module.
propose() {
  local label="$1" m="$2" n="$3"
  mkdir -p "$OUT/props/$label"
  if [ -s "$OUT/base/$n.wasm" ] && [ -s "$OUT/base/$n.anchors" ]; then
    timeout 300 "$CUT" --fresh-like "$m" "$OUT/base/$n.wasm" "$OUT/base/$n.anchors" > "$OUT/props/$label/$n.txt" 2> "$OUT/props/$label/$n.err" \
      || echo "FRESHALLOC unreadable" > "$OUT/props/$label/$n.txt"
  else
    echo "FRESHALLOC no-base" > "$OUT/props/$label/$n.txt"
  fi
}
export -f name_of emit propose
export HELD OUT ALMIDE PRODUCER CUT

t0=$(date +%s)
: > "$OUT/base-failed"
xargs -P 4 -I{} bash -c 'emit "$1"' _ {} < "$LIST"
echo "unmutated emission: $(( $(date +%s) - t0 )) s; $(ls "$OUT/base" | grep -c '\.anchors$') anchor file(s), $(wc -l < "$OUT/base-failed" | tr -d ' ') program(s) failed"

# Every unmutated module, then every kept module of every mutant.
while IFS= read -r f; do
  n="$(name_of "$f")"
  [ -s "$OUT/base/$n.wasm" ] && printf 'base\t%s\t%s\n' "$OUT/base/$n.wasm" "$n"
done < "$LIST" > "$OUT/jobs.tsv"
find "$SH" -type f -path '*/emut/modules/*/*.wasm' | sort | while IFS= read -r m; do
  printf '%s\t%s\t%s\n' "$(basename "$(dirname "$m")")" "$m" "$(basename "$m" .wasm)"
done >> "$OUT/jobs.tsv"
echo "$(grep -c '^base' "$OUT/jobs.tsv") unmutated module(s); $(grep -vc '^base' "$OUT/jobs.tsv") kept module(s) of $(grep -v '^base' "$OUT/jobs.tsv" | cut -f1 | sort -u | wc -l | tr -d ' ') mutant(s)"
t0=$(date +%s)
tr '\t' '\n' < "$OUT/jobs.tsv" | xargs -P 4 -n 3 bash -c 'propose "$1" "$2" "$3"' _
echo "proposals: $(( $(date +%s) - t0 )) s"

# A kept module that equals the unmutated one was kept for nothing: the two
# emissions disagree, and the comparison below would not be about the mutant.
same=0
while IFS=$'\t' read -r id m n; do
  [ "$id" = base ] && continue
  if cmp -s "$m" "$OUT/base/$n.wasm"; then same=$((same + 1)); echo "kept module equals the unmutated one: $id $n"; fi
done < "$OUT/jobs.tsv" | head -n 20
: > "$OUT/dump"
while IFS=$'\t' read -r id m n; do
  printf 'MODULE %s %s\n' "$id" "$n" >> "$OUT/dump"
  cat "$OUT/props/$id/$n.txt" >> "$OUT/dump"
done < "$OUT/jobs.tsv"
echo "dump: $(grep -c '^FRESHHEX' "$OUT/dump") proposal(s); allocator not found in $(grep -c '^FRESHALLOC none' "$OUT/dump") module(s), unreadable $(grep -c '^FRESHALLOC unreadable' "$OUT/dump"), no unmutated module $(grep -c '^FRESHALLOC no-base' "$OUT/dump")"

# The kernel is asked once per DISTINCT proposal (allocator and the three
# byte strings), and the verdicts are spread back over the proposals.
python3 -I "$ROOT/scripts/probe-held-fresh-uniq.py" fold "$OUT/dump" "$OUT/uniq-dump" "$OUT/uniq-index.tsv"
bash "$ROOT/scripts/probe-fresh-kernel.sh" "$OUT/uniq-dump" "$OUT/kernel-uniq" > "$OUT/kernel.log" 2>&1 || { echo "the kernel run failed"; tail -n 20 "$OUT/kernel.log"; exit 1; }
grep -E 'proposal\(s\) in the dump|kernel time|Closed under' "$OUT/kernel.log"
mkdir -p "$OUT/kernel"
python3 -I "$ROOT/scripts/probe-held-fresh-uniq.py" spread "$OUT/kernel-uniq/verdicts.tsv" "$OUT/uniq-index.tsv" "$OUT/kernel/verdicts.tsv"
python3 -I "$ROOT/scripts/probe-held-fresh-join.py" "$OUT/dump" "$OUT/kernel/verdicts.tsv" "$SH" "$OUT"

# The loop reads (proofs/GuardLoop.v) of the same modules: `--fresh-like`
# also prints `LOOPINC <index | none>` and the `LOOPHEX` proposals, and the
# kernel is asked once per distinct proposal.
echo "loop dump: $(grep -c '^LOOPHEX' "$OUT/dump") proposal(s); increment routine not found in $(grep -c '^LOOPINC none' "$OUT/dump") module(s)"
bash "$ROOT/scripts/probe-loop-kernel.sh" "$OUT/dump" "$OUT/loop-kernel" > "$OUT/loop-kernel.log" 2>&1 || { echo "the loop kernel run failed"; tail -n 20 "$OUT/loop-kernel.log"; exit 1; }
grep -E 'proposal\(s\) in the dump|kernel time|Closed under' "$OUT/loop-kernel.log"
grep -A 12 '^loop kernel check \[base\]' "$OUT/loop-kernel.log" | head -n 13
python3 -I "$ROOT/scripts/probe-held-loop-join.py" "$OUT/dump" "$OUT/loop-kernel/verdicts.tsv" "$SH" "$OUT"

# The guarded windows (proofs/GuardWindow.v) of the same modules: every access
# a straight-line window proves on its own, cut from the module alone. The
# kernel is asked once per distinct byte string.
wins() {
  local label="$1" m="$2" n="$3"
  mkdir -p "$OUT/wins/$label"
  timeout 300 "$CUT" --windows-all "$m" > "$OUT/wins/$label/$n.txt" 2> /dev/null || : > "$OUT/wins/$label/$n.txt"
}
export -f wins
t0=$(date +%s)
tr '\t' '\n' < "$OUT/jobs.tsv" | xargs -P 4 -n 3 bash -c 'wins "$1" "$2" "$3"' _
echo "window proposals: $(( $(date +%s) - t0 )) s"
python3 -I "$ROOT/scripts/probe-held-windows-join.py" fold "$OUT/jobs.tsv" "$OUT/wins" "$OUT/win-uniq-dump" "$OUT/win-index.tsv"
bash "$ROOT/scripts/probe-window-kernel.sh" "$OUT/win-uniq-dump" "$OUT/win-kernel" > "$OUT/win-kernel.log" 2>&1 || { echo "the window kernel run failed"; tail -n 20 "$OUT/win-kernel.log"; }
grep -E 'window\(s\) in the dump|Closed under' "$OUT/win-kernel.log"
python3 -I "$ROOT/scripts/probe-held-windows-join.py" spread "$OUT/win-kernel/verdicts.tsv" "$OUT/win-index.tsv" "$OUT"
