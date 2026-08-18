#!/usr/bin/env bash
# WALL CORPUS (#1481): programs that must STILL be refused.
# =========================================================
#
# Every honest wall in the wasm renderer is a load-bearing refusal: the
# alternative to walling is a leak, a double-free, or invalid wasm (each
# fixture's header says which). The other ratchets watch walls DISAPPEAR
# (proofs/check-wasm-fallback.sh, per file; proofs/corpus-wall.sh, per MIR
# function); this one watches refusals STAY REFUSALS — a change that
# accidentally starts ACCEPTING one of these shapes, without building the
# machinery its header demands, is a silent-corruption bug the suite would
# otherwise miss entirely.
#
# Each proofs/wall-corpus/*.almd carries a `// @wall: <substring>` header
# naming the expected refusal. Per fixture, `almide build --target wasm`
# must FAIL with stderr containing that substring:
#   - builds cleanly       -> GATE FAILS: the shape now lowers. That may be
#                             a genuine advance — then the fixture graduates:
#                             verify the semantics on BOTH targets, move the
#                             program to spec/wasm_cross/ under a contract,
#                             and delete it here IN THE SAME PR (shrink-only,
#                             explicit — never a silent admission).
#   - fails, wrong message -> GATE FAILS: the refusal reason drifted; re-pin.
#
# Usage: check-wall-corpus.sh   (ALMIDE_BIN overrides the binary)
set -euo pipefail
export LC_ALL=C

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ALMIDE_BIN:-}"
if [ -z "$BIN" ]; then
  if [ -x "$ROOT/target/release/almide" ]; then BIN="$ROOT/target/release/almide"; else BIN="almide"; fi
fi

CORPUS="$ROOT/proofs/wall-corpus"
count=$(ls "$CORPUS"/*.almd 2>/dev/null | wc -l | tr -d ' ')
# An emptied corpus must not pass vacuously (#976 class).
[ "$count" -ge 3 ] || { echo "::error::wall-corpus: only $count fixture(s) in $CORPUS — the corpus moved"; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fail=0
for f in "$CORPUS"/*.almd; do
  name=$(basename "$f" .almd)
  want=$(sed -n 's|^// @wall: ||p' "$f" | head -1)
  if [ -z "$want" ]; then
    echo "::error::wall-corpus: $name has no '// @wall:' header"
    fail=1
    continue
  fi
  if "$BIN" build "$f" --target wasm -o "$TMP/$name.wasm" > "$TMP/out.txt" 2>&1; then
    echo "::error::wall-corpus: $name now BUILDS — the refused shape lowers. If intended, graduate it: verify both targets, move to spec/wasm_cross/ under a contract, delete it here in the same PR."
    fail=1
  elif ! grep -qF "$want" "$TMP/out.txt"; then
    echo "::error::wall-corpus: $name still fails but the refusal drifted — expected substring '$want', got:"
    tail -3 "$TMP/out.txt" | sed 's/^/    /'
    fail=1
  else
    echo "ok $name (refused: $want)"
  fi
done

echo "wall-corpus: $count refusal(s) pinned"
exit $fail
