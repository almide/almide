#!/usr/bin/env bash
# Probe only, not a gate: the fresh-block proposals of every contract program,
# as the gate's cutter prints them, kept as a file.
#
#   scripts/probe-fresh-dump.sh <out-dir>
#
# Per program the gate's own three steps (proofs/structural-wall.sh): the
# compiler builds the module, the producer writes its anchor file, and
# `almide-anchor-check --fresh` prints one `FRESH <function> <access
# instruction> <allocator> | <bytes> | <bytes> | <bytes>` line per proposal.
# Writes <out-dir>/fresh-dump (`MODULE <program>` before each module's lines)
# and <out-dir>/summary.txt.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
mkdir -p "$1"
OUT="$(cd "$1" && pwd)"
TD="${CARGO_TARGET_DIR:-$ROOT/target}/release"
ALMIDE="$TD/almide"
PRODUCER="$TD/examples/emit_structural_bundle"
CUT="$TD/almide-anchor-check"
[ -x "$ALMIDE" ] && [ -x "$PRODUCER" ] && [ -x "$CUT" ] || { echo "a binary is missing"; exit 1; }
W="$OUT/work"
mkdir -p "$W"
: > "$OUT/fresh-dump"
built=0; refused=0; unread=0
while IFS= read -r f; do
  case "$f" in ''|'#'*) continue ;; esac
  if ! "$ALMIDE" build "$f" --target wasm -o "$W/a.wasm" > "$W/build.err" 2>&1 < /dev/null; then
    refused=$((refused + 1)); echo "NOBUILD $f" >> "$OUT/fresh-dump"; continue
  fi
  if ! "$PRODUCER" "$f" --artifact "$W/a.wasm" --anchors "$W/a.anchors" > "$W/a.bundle" 2> "$W/producer.err"; then
    refused=$((refused + 1)); echo "NOWITNESS $f" >> "$OUT/fresh-dump"; continue
  fi
  built=$((built + 1))
  printf 'MODULE %s\n' "$f" >> "$OUT/fresh-dump"
  "$CUT" --fresh "$W/a.wasm" "$W/a.anchors" >> "$OUT/fresh-dump" 2> "$W/fresh.err" || unread=$((unread + 1))
done < crates/almide-wasm/tests/golden/witness-fixtures.txt
{
  echo "programs built with a witness: $built ; not built or no witness: $refused ; cutter failed: $unread"
  echo "proposals: $(grep -c '^FRESH [0-9]' "$OUT/fresh-dump") in $(grep -c '^ALLOCATOR ' "$OUT/fresh-dump") module(s) that name an allocator"
} | tee "$OUT/summary.txt"
[ "$built" -gt 0 ] && [ "$unread" -eq 0 ]
